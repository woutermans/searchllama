use std::{
    convert::Infallible,
    process::id,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use embedding::vec_cos_sim;
use futures::{stream, Stream, StreamExt};
use lazy_static::lazy_static;
use log::{debug, info, warn};
use ollama_rs::{
    generation::{completion::request::GenerationRequest, options::GenerationOptions},
    Ollama,
};
use playwright::{api::BrowserType, Playwright};
use pollster::FutureExt;
use search::calculate_entry_similarity;
use searchllama_types::{Entry, SearchRequest, SearchResponse};
use tokio::sync::mpsc::{self, Sender};
use warp::Filter;

mod database;
mod embedding;
mod search;

pub const MAX_ENTRIES: usize = 50;
pub const EMBEDDING_MODEL: &str = "nomic-embed-text:latest";
pub const SEARCH_MODEL: &str = "llama3.1:latest";
pub const MAX_EMBEDDING_SIZE: usize = 1024;
pub const SNIPPET_TARGET_SIZE: usize = 512;
pub const SNIPPET_NUMBER: usize = 10;
pub const MIN_CONFIDENCE: f64 = 0.72;
lazy_static! {
    pub static ref G_OLLAMA: Ollama = Ollama::default();
}

async fn handle_search_request(
    query: SearchRequest,
) -> impl Stream<Item = Result<String, Infallible>> {
    let query_embedding = embedding::generate_embedding(&query.query)
        .await
        .expect("Failed to generate embedding");

    let mut results = database::query_db(&query_embedding).await;

    let (sender, receiver) = mpsc::channel(10);
    let sender = Arc::new(sender);
    {
        let sender = sender.clone();
        let query_embedding = query_embedding.clone();
        let query = query.clone();
        let mut top_urls = results.clone();
        top_urls.truncate(SNIPPET_NUMBER);
        let top_urls = top_urls
            .iter()
            .map(|entry| entry.0.clone())
            .collect::<Vec<String>>();
        tokio::spawn(async move {
            let related_queries = G_OLLAMA.generate(
        GenerationRequest::new(SEARCH_MODEL.to_string(), format!("Generate search queries for: {}", query.query))
                    .system("You are a helpful assistant. Show each query on a new line. without any explanation or numbering.".to_string())
                ).await.unwrap().response.split('\n').filter(|q| !q.is_empty()).map(|q| q.trim().to_string()).collect::<Vec<String>>();

            let explanation_needed_string = G_OLLAMA
                .generate(
                    GenerationRequest::new(
                        SEARCH_MODEL.to_string(),
                        format!(
                            "Does the person want an ai generated answer based on this search query or just a website: '{}'?",
                            &query.query
                        ),
                    )
                    .system("only answer with 'anwer' or 'website'".to_string()),
                )
                .await
                .unwrap()
                .response
                .to_lowercase()
                .trim()
                .to_owned();

            info!("Explanation needed: {}", explanation_needed_string);

            let explanation_needed = !explanation_needed_string.contains("website");

            info!("Related queries: {:?}", related_queries);

            let best_snippets =
                search::get_best_matching_snippets(&query_embedding, &top_urls, None)
                    .await
                    .expect("Failed to get best snippets");

            let mean_score = best_snippets
                .iter()
                .map(|entry| entry.score.unwrap())
                .sum::<f64>()
                / best_snippets.len() as f64;

            info!("Mean score: {}", mean_score); // Log the mean score

            async fn spawn_lm_thread(
                sender: Arc<Sender<String>>,
                query: SearchRequest,
                best_snippets: Vec<search::SnippetInfo>,
            ) {
                let sender = sender.clone();
                let best_snippets = best_snippets
                    .into_iter()
                    .map(|s| s.text)
                    .collect::<Vec<String>>();
                let query = query.clone();
                tokio::spawn(async move {
                    let prompt = format!(
                        "{}\n\n\n\n Question: '{}'.\n\n Use nice formatting",
                        best_snippets.join("\n\n"),
                        query.query
                    );

                    debug!("Prompt: {}", prompt);

                    let mut response_stream = G_OLLAMA
                    .generate_stream(
                        GenerationRequest::new(
                            SEARCH_MODEL.to_string(),
                            prompt,
                        )
                        .system(
                            "You are a helpful assistant.\n You are given a list of snippets from the internet and a question.\n You must answer the question based on the snippets whithout mentioning that you received snippets from the internet.\n Answer with the language used in the question"
                                .to_string()
                        )
                        .options(GenerationOptions::default().temperature(0.1)),
                    ).await.expect("Failed to generate response");

                    while let Some(response) = response_stream.next().await {
                        if let Ok(response) = response {
                            let search_response = SearchResponse {
                                results: Vec::new(),
                                summary: response.into_iter().map(|s| s.response).collect(),
                            };

                            let search_response_string = serde_json::to_string(&search_response)
                                .expect("Failed to serialize search response");

                            sender
                                .send(search_response_string)
                                .await
                                .expect("Failed to send search response");
                        }
                    }
                });
            }

            let need_to_respond = Arc::new(AtomicBool::new(true));
            if mean_score > MIN_CONFIDENCE && explanation_needed {
                need_to_respond.store(false, Ordering::Relaxed);
                spawn_lm_thread(sender.clone(), query.clone(), best_snippets.clone()).await;
            }

            let best_snippets = Arc::new(tokio::sync::Mutex::new(best_snippets));
            let mut queries = vec![query];
            related_queries
                .into_iter()
                .for_each(|q| queries.push(SearchRequest { query: q }));

            {
                let queries = queries.clone();
                for (idx, query) in queries.into_iter().enumerate() {
                    let query_embedding = query_embedding.clone();
                    let sender = sender.clone();
                    let best_snippets = Arc::clone(&best_snippets);
                    let need_to_respond = Arc::clone(&need_to_respond);
                    tokio::spawn(async move {
                        let results = search::query_ddg(
                            &query.query,
                            match idx {
                                0 => 10,
                                _ => 3,
                            },
                        )
                        .await
                        .expect("Failed to query DDG");

                        let pw = Playwright::initialize()
                            .block_on()
                            .expect("Failed to initialize Playwright");
                        let chromium = pw.chromium();
                        let browser = chromium
                            .launcher()
                            .headless(false)
                            .launch()
                            .await
                            .expect("Failed to launch browser");
                        let context = Arc::new(
                            browser
                                .context_builder()
                                .build()
                                .await
                                .expect("Failed to create context"),
                        );

                        let mut join_set = tokio::task::JoinSet::new();
                        for (url, title, desc) in results.into_iter() {
                            let entry = Entry {
                                score: 0.0,
                                url: url.clone(),
                                title,
                                description: desc,
                            };
                            let url = url.clone();
                            let context = context.clone();
                            join_set.spawn(async move {
                                (embedding::get_website_embedding(&url, context).await, entry)
                            });
                        }
                        //let mut pbar = tqdm::pbar(Some(join_set.len()));
                        while let Some(Ok((embedding, entry))) = join_set.join_next().await {
                            if let Ok(embedding) = embedding {
                                if need_to_respond.load(Ordering::Relaxed) && explanation_needed {
                                    let snippet = search::get_best_matching_snippet(
                                        &entry.url,
                                        &query_embedding,
                                        context.clone(),
                                    )
                                    .await
                                    .expect("Failed to get best matching snippet");

                                    let mut lock = best_snippets.lock().await;
                                    lock.push(snippet);
                                    lock.sort_by(|a, b| {
                                        a.score.partial_cmp(&b.score).unwrap().reverse()
                                    });
                                    lock.truncate(SNIPPET_NUMBER);

                                    let mean_score =
                                        lock.iter().map(|entry| entry.score.unwrap()).sum::<f64>()
                                            / lock.len() as f64;

                                    info!("Mean score: {}", mean_score);

                                    if mean_score > MIN_CONFIDENCE
                                        && need_to_respond.load(Ordering::Relaxed)
                                    {
                                        info!(
                                            "Mean score [{}] > MIN_CONFIDENCE [{}]",
                                            mean_score, MIN_CONFIDENCE
                                        );

                                        need_to_respond.store(false, Ordering::Relaxed);
                                        spawn_lm_thread(
                                            sender.clone(),
                                            query.clone(),
                                            lock.clone(),
                                        )
                                        .await;
                                    }
                                }

                                let title_embedding = embedding::generate_embedding(&entry.title)
                                    .await
                                    .expect("Failed to generate embedding for title");

                                let mut entry_with_score = entry.clone();
                                entry_with_score.score = calculate_entry_similarity(
                                    &query_embedding,
                                    &title_embedding,
                                    &embedding.embeddings,
                                );
                                if entry_with_score.score > 10.0 || entry_with_score.score < -10.0 {
                                    continue;
                                }

                                debug!(
                                    "Entry: {} - Score: {}",
                                    entry_with_score.title, entry_with_score.score
                                );

                                let search_response = SearchResponse {
                                    results: vec![entry_with_score],
                                    summary: String::new(),
                                };
                                let response_str = serde_json::to_string(&search_response).unwrap();
                                sender
                                    .send(response_str)
                                    .await
                                    .expect("Failed to send response");

                                database::update_entry(
                                    &entry.url,
                                    &entry.title,
                                    &entry.description,
                                    embedding::generate_embedding(&entry.title).await.unwrap(),
                                    embedding.embeddings,
                                )
                                .await;
                            } else {
                                warn!("Failed to get embedding for url: {}", entry.url);
                            }
                            //pbar.update(1).unwrap();
                        }

                        if need_to_respond.load(Ordering::Relaxed) && explanation_needed {
                            need_to_respond.store(false, Ordering::Relaxed);
                            spawn_lm_thread(
                                sender.clone(),
                                query,
                                best_snippets.lock().await.clone(),
                            )
                            .await;
                        }

                        context.close().await.expect("Failed to close context");
                        browser.close().await.expect("Failed to close browser");

                        //pbar.close().unwrap();
                    });
                }
            }
        });
    }

    results.truncate(MAX_ENTRIES); // Truncate the results to MAX_ENTRIES
    let response = SearchResponse {
        results: results
            .into_iter()
            .map(|(url, text, desc, score)| Entry {
                score,
                url,
                title: text,
                description: desc,
            })
            .collect(),
        summary: String::new(),
    };

    let json = serde_json::to_string(&response).unwrap();
    sender.send(json).await.expect("Failed to send response");

    stream::unfold(receiver, |mut receiver| async move {
        receiver
            .recv()
            .await
            .map(|item| (Ok(item + "\n\n"), receiver))
    })
}

#[tokio::main]
async fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    {
        // prepare playwright
        let pw = Playwright::initialize()
            .await
            .expect("Failed to initialize playwright");
        pw.prepare().expect("Failed to prepare playwright");
    }

    // GET /search with json body that will be serialized into a struct with serde_json
    let search_router = warp::path!("search")
        .and(warp::post())
        .and(warp::body::json())
        .and_then(|query: serde_json::Value| async move {
            let query: SearchRequest = serde_json::from_value(query).unwrap();
            info!("Received search request: {:?}", query);

            let res_stream = handle_search_request(query).await;
            let body = warp::hyper::Body::wrap_stream(res_stream);
            let response = warp::http::Response::new(body);

            Ok(response) as Result<_, Infallible>

            //Ok::<_, Infallible>(warp::reply::json(&response))
        });

    let cors = warp::cors()
        .allow_any_origin() // You can specify a particular origin here if needed
        .allow_headers(vec!["Content-Type", "Authorization"])
        .allow_methods(&[warp::http::Method::GET, warp::http::Method::POST]);

    let routes = search_router.with(cors);

    warp::serve(routes).run(([0, 0, 0, 0], 3030)).await;
}
