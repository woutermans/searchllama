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
    generation::{
        completion::{request::GenerationRequest, GenerationContext},
        images::Image,
        options::GenerationOptions,
    },
    Ollama,
};
use playwright::{api::BrowserType, Playwright};
use pollster::FutureExt;
use search::calculate_entry_similarity;
use searchllama_types::{ChatRequest, ChatResponse, Entry, SearchRequest, SearchResponse};
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
    pub static ref G_REWEST_CLIENT: reqwest::Client = reqwest::Client::new();
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
        let top_url_titles = top_urls
            .iter()
            .map(|entry| entry.1.clone())
            .collect::<Vec<String>>();
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
                            "
Does the person want ai assitance based on this search query or just a website: '{}'\n\n
Examples of queries that need an ai generated answer: 
    - What is the best way to learn rust
    - How do I get a job as a software engineer
    - Where is my local bank
    - Why is this website not working
    - Who is the best basketball player in the world
    - Wat is de beste manier om RUST te leren
    - Hoe kom ik aan een baan als software engineer
    - Waar is mijn lokale winkel
    - Waarom werkt deze website niet
    - Wie is de beste basketballer ter wereld
    - Rustをベストに学ぶ方法は何ですか
    - ソフトウェアエンジニアとしての仕事を手に入れるにはどうすればよいですか
    - 私の近くの店はどこですか
    - このウェブサイトが動作しないのはなぜですか
    - 世界で一番良いバスケットボール選手は誰ですか
    - Wie lerne ich Rust am besten
    - Wie kann ich als Software-Engineer arbeiten
    - Wo ist mein lokales Geschäft
    - Warum funktioniert diese Website nicht
    - Wer ist der beste Basketballspieler der Welt
    - Quelle est la meilleure façon d'apprendre RUST
    - Comment puis-je trouver un emploi en tant que ingénieur logiciel
    - Où se trouve mon magasin local
    - Pourquoi ce site Web ne fonctionne-t-il pas
    - Qui est le meilleur joueur de basket-ball du monde
    - .entry-score p {{     margin: 0;     font-size: 0.9em;     color: #777; }}

Examples of queries that do not need an ai generated answer: 
    - Youtube
    - Google
    - Bing
    - Reddit
    - Rust wiki
    - Stackoverflow
    - llama3 huggingface
    - Python Documentation
    - PHP manual
    - Rust documentation
    - Facebook
    - Twitter
    - Instagram
    - LinkedIn
    - Pinterest
    - TikTok
    - clothing aliexpress
    - WhatsApp
    - Telegram
    - Discord
    - Quora
    - Medium
    - Tumblr
    - GitHub
    - GitLab
    - Bitbucket
    - Amazon
    - eBay
    - Etsy
    - Alibaba
\n
If the question is in another language than English, translate it to English first.\n\n
                            ",
                            &query.query
                        ),
                    )
                    .system(
                        "You are a helpful assistant. only answer with 'anwer' or 'website'"
                            .to_string(),
                    ),
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

            let best_snippets = search::get_best_matching_snippets(
                &query_embedding,
                &top_urls,
                &top_url_titles,
                None,
            )
            .await
            .expect("Failed to get best matching snippets");

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
                let best_snippets = best_snippets.clone();
                let query = query.clone();
                tokio::spawn(async move {
                    let snippets = best_snippets
                        .iter()
                        .map(|entry| {
                            format!(
                                "From \"{}\" ![]({}):\n\"{}\"",
                                entry.title.as_ref().unwrap_or(&String::from("Unkown")),
                                entry.url.as_ref().unwrap_or(&String::from("Unkown")),
                                entry.text
                            )
                        })
                        .collect::<Vec<String>>();
                    let images = best_snippets
                        .iter()
                        .map(|entry| entry.images.clone())
                        .flatten()
                        .collect::<Vec<(String, String)>>();
                    let prompt = format!(
                        "Sources:\n\"{}\"\n\n
Anwer this question: '{}'.",
                        snippets.join("\n\n"),
                        // images
                        // .iter()
                        // .rev()
                        // .map(|(url, desc)| format!("- {}: '{}'\n", desc, url))
                        // .collect::<String>(),
                        query.query
                    );

                    info!("Prompt: {}", prompt);

                    let mut response_stream = G_OLLAMA
                    .generate_stream(
                        GenerationRequest::new(
                            SEARCH_MODEL.to_string(),
                            prompt,
                        )
                        .system(
"You are a helpful assistant.
You are given a list of snippets from the internet and a question.
You must answer the question based on the snippets whithout mentioning that you received snippets from the internet.
Use correct markdown formatting.
Answer with the language used in the question.
only use emojis for country flags."
                                .to_string()
                        )
                        .options(GenerationOptions::default()),
                    ).await.expect("Failed to generate response");

                    while let Some(response) = response_stream.next().await {
                        if let Ok(response) = response {
                            let mut search_response = SearchResponse {
                                results: Vec::new(),
                                summary: response.iter().map(|s| s.response.clone()).collect(),
                                summary_context: None,
                            };

                            // for chunk in &response {
                            //     if let Some(context) = &chunk.context {
                            //         info!("Context: {}", context.0.len());
                            //         search_response.summary_context = Some(context.clone().0);
                            //     }
                            // }

                            for chunk in &response {
                                if chunk.context.is_some() {
                                    search_response.summary_context = Some(
                                        chunk.context.as_ref().expect("Context is none").clone().0,
                                    );
                                }
                            }

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
            let mut queries = vec![query.clone()];
            related_queries
                .into_iter()
                .for_each(|q| queries.push(SearchRequest { query: q }));

            {
                let queries = queries.clone();
                let user_query = query;
                for (idx, query) in queries.into_iter().enumerate() {
                    let query_embedding = query_embedding.clone();
                    let sender = sender.clone();
                    let best_snippets = Arc::clone(&best_snippets);
                    let need_to_respond = Arc::clone(&need_to_respond);
                    let user_query = user_query.clone();
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
                            .headless(true)
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
                        for result in results.into_iter() {
                            let url = result.url;
                            let title = result.title;
                            let desc = result.body;

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
                                            user_query.clone(),
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
                                    summary_context: None,
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
                                user_query,
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
        summary_context: None,
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

async fn handle_chat_request(
    message: String,
    context: Vec<i32>,
) -> impl Stream<Item = Result<String, Infallible>> {
    let (sender, mut receiver) = mpsc::channel(8);
    let sender = Arc::new(sender); // Create an Arc to share the sender across threads

    tokio::spawn(async move {
        let mut response_stream = G_OLLAMA
            .generate_stream(
                GenerationRequest::new(SEARCH_MODEL.to_string(), message)
                    .context(GenerationContext { 0: context }),
            )
            .await
            .expect("Failed to generate response");

        while let Some(response) = response_stream.next().await {
            if let Ok(response) = response {
                let mut chat_response = ChatResponse {
                    response: response.iter().map(|c| c.response.clone()).collect(),
                    context: None,
                };

                for chunk in response.iter() {
                    if chunk.context.is_some() {
                        chat_response.context = Some(chunk.context.clone().unwrap().0);
                    }
                }

                let response_json = serde_json::to_string(&chat_response)
                    .expect("Failed to serialize response");

                sender.send(response_json).await.unwrap();
            }
        }
    });

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

    let chat_router = warp::path!("chat")
        .and(warp::post())
        .and(warp::body::json())
        .and_then(|query: serde_json::Value| async move {
            let query: ChatRequest = serde_json::from_value(query).unwrap(); // Assuming you have a struct for the request body
            info!("Received chat request: {:?}", query);

            let res_stream = handle_chat_request(query.message, query.context).await;
            let body = warp::hyper::Body::wrap_stream(res_stream);
            let response = warp::http::Response::new(body);

            Ok(response) as Result<_, Infallible>
        });

    let cors = warp::cors()
        .allow_any_origin() // You can specify a particular origin here if needed
        .allow_headers(vec!["Content-Type", "Authorization"])
        .allow_methods(&[warp::http::Method::GET, warp::http::Method::POST]);

    let routes = search_router.or(chat_router).with(cors);

    warp::serve(routes).run(([0, 0, 0, 0], 3030)).await;
}
