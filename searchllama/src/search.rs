use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use async_recursion::async_recursion;
use cached::proc_macro::io_cached;
use cached::DiskCache;
use lazy_static::lazy_static;
use log::info;
use ollama_rs::{generation::completion::request::GenerationRequest, Ollama};
use playwright::{
    api::{browser, BrowserContext},
    Playwright,
};
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use tqdm::tqdm;

use crate::{
    embedding::{self, get_website_embedding, vec_cos_sim},
    MAX_EMBEDDING_SIZE, SNIPPET_TARGET_SIZE,
};

lazy_static! {
    static ref DDGS: Py<PyAny> = {
        Python::with_gil(|py| -> Py<PyAny> {
            py.import_bound("duckduckgo_search")
                .unwrap()
                .getattr("DDGS")
                .unwrap()
                .into()
        })
    };
    static ref REQUEST_CLIENT: reqwest::Client = reqwest::Client::new();
    static ref OLLAMA: ollama_rs::Ollama = ollama_rs::Ollama::new("http://192.168.1.199", 11434);
}

#[io_cached(
    map_error = r##" | e | { format!("Failed to cache: {}", e) }"##,
    disk = true,
    convert = r#"{ format!("{}{}", query, max_results) }"#,
    ty = "DiskCache<String, Vec<(String, String, String)>>"
)]
pub async fn query_ddg(
    query: &str,
    max_results: usize,
) -> Result<Vec<(String, String, String)>, String> {
    let mut titles = Vec::new();
    let mut urls = Vec::new();
    let mut bodies = Vec::new();

    lazy_static! {
        static ref DDG_SEMAPHORE: tokio::sync::Semaphore = tokio::sync::Semaphore::new(1);
    }

    let _permit = DDG_SEMAPHORE.acquire().await.unwrap();

    Python::with_gil(|py| {
        let code = PyModule::from_code_bound(
            py,
            "def gert(query, max_results):
           from duckduckgo_search import DDGS
           results = DDGS().text(query, max_results=max_results)
           return results
        ",
            "",
            "",
        )
        .expect("Failed to import module")
        .getattr("gert")
        .expect("Failed to get attribute");

        let results: Vec<HashMap<String, String>> =
            code.call1((query, max_results)).unwrap().extract().unwrap();
        titles = results.iter().map(|r| r["title"].clone()).collect();
        urls = results.iter().map(|r| r["href"].clone()).collect();
        bodies = results.iter().map(|r| r["body"].clone()).collect();

        //info!("Results: {:?}", results);
    });

    Ok(titles
        .into_iter()
        .zip(urls.into_iter())
        .zip(bodies.into_iter())
        .map(|((title, url), body)| (url, title, body))
        .collect())
}

pub fn calculate_entry_similarity(
    query_embedding: &[f64],
    title_embedding: &[f64],
    body_embeddings: &[Vec<f64>],
) -> f64 {
    let max_body_sim = body_embeddings.iter().fold(f64::MIN, |acc, x| {
        let sim = vec_cos_sim(query_embedding, x).unwrap_or(-10.0);
        acc.max(sim)
    });

    max_body_sim + vec_cos_sim(query_embedding, title_embedding).unwrap_or(-10.0) * 0.3
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SnippetInfo {
    pub embedding: Vec<f64>,
    pub text: String,
    pub score: Option<f64>,
    pub images: Vec<(String, String)>,
}
pub async fn get_best_matching_snippet(
    url: &str,
    query_embedding: &[f64],
    pw_context: Arc<BrowserContext>,
) -> Result<SnippetInfo, String> {
    let web_embedding = get_website_embedding(url, pw_context).await?;
    let best_chunk = web_embedding
        .embeddings
        .iter()
        .zip(web_embedding.texts.iter())
        .fold(
            (f64::MIN, vec![], String::new()),
            |acc, (body_emb, body)| {
                let sim = vec_cos_sim(query_embedding, body_emb).unwrap();
                if sim > acc.0 {
                    (sim, body_emb.clone(), body.to_string())
                } else {
                    acc
                }
            },
        );
    let mut best_chunk = SnippetInfo {
        embedding: best_chunk.1,
        text: best_chunk.2,
        score: Some(best_chunk.0),
        images: web_embedding.images,
    };

    #[async_recursion]
    async fn find_best_snippet(
        query_embedding: &[f64],
        current_chunk: &mut SnippetInfo,
        chunk_size: usize,
        target_size: usize,
    ) {
        let embeddings = embedding::generate_large_embedding(&current_chunk.text, Some(chunk_size))
            .await
            .expect("Failed to generate embedding");

        let best_chunk = embeddings.embeddings.iter().zip(embeddings.texts).fold(
            (f64::MIN, vec![], String::new()),
            |acc, (body_emb, body)| {
                let sim = vec_cos_sim(query_embedding, body_emb)
                    .expect("Failed to calculate cosine similarity");
                if sim > acc.0 {
                    (sim, body_emb.clone(), body)
                } else {
                    acc
                }
            },
        );
        let best_chunk = SnippetInfo {
            embedding: best_chunk.1,
            text: best_chunk.2,
            score: Some(best_chunk.0),
            images: current_chunk.images.clone(),
        };
        *current_chunk = best_chunk;

        if current_chunk.text.len() < target_size {
            return;
        } else {
            find_best_snippet(query_embedding, current_chunk, chunk_size / 2, target_size).await;
        }
    }

    find_best_snippet(
        query_embedding,
        &mut best_chunk,
        MAX_EMBEDDING_SIZE / 2,
        SNIPPET_TARGET_SIZE,
    )
    .await;

    Ok(best_chunk)
}

pub async fn get_best_matching_snippets(
    query: &[f64],
    urls: &[String],
    pw_context: Option<Arc<BrowserContext>>,
) -> Result<Vec<SnippetInfo>, String> {
    let mut pw = None;
    let mut chromium = None;
    let mut browser = None;
    let pw_context = match pw_context {
        Some(pw) => pw,
        None => {
            pw = Some(
                Playwright::initialize()
                    .await
                    .expect("Failed to initialize Playwright"),
            );
            chromium = Some(pw.as_ref().unwrap().chromium());
            browser = Some(
                chromium
                    .as_ref()
                    .unwrap()
                    .launcher()
                    .headless(true)
                    .launch()
                    .await
                    .expect("Failed to launch browser"),
            );
            let context = Arc::new(
                browser
                    .as_ref()
                    .unwrap()
                    .context_builder()
                    .build()
                    .await
                    .expect("Failed to create context"),
            );

            context
        }
    };

    let mut snippets = Vec::new();
    for url in urls.iter() {
        snippets.push(get_best_matching_snippet(&url, query, pw_context.clone()).await?);
    }

    // Sort the snippets by their score
    snippets.sort_by(|a, b| {
        a.score
            .unwrap()
            .partial_cmp(&b.score.unwrap())
            .unwrap()
            .reverse()
    });

    pw_context.close().await;
    if let Some(browser) = browser {
        browser.close().await;
    }

    Ok(snippets)
}
