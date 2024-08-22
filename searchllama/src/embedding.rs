use std::sync::Arc;

use cached::proc_macro::{cached, io_cached};
use cached::DiskCache;
use lazy_static::lazy_static;
use log::{info, warn};
use ollama_rs::generation::options::GenerationOptions;
use playwright::{
    api::{Browser, BrowserContext, BrowserType},
    Playwright,
};
use pollster::FutureExt;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Semaphore};

use crate::{embedding, EMBEDDING_MODEL, G_OLLAMA, MAX_EMBEDDING_SIZE};

#[io_cached(
    map_error = r##" | e | { format!("Failed to cache: {}", e) }"##,
    disk = true,
    convert = r#"{ format!("{}", text) }"#,
    ty = "DiskCache<String, Vec<f64>>"
)]
pub async fn generate_embedding(text: &str) -> Result<Vec<f64>, String> {
    Ok(G_OLLAMA
        .generate_embeddings(
            EMBEDDING_MODEL.to_string(),
            text.to_string(),
            None, //Some(GenerationOptions::default().num_gpu(0)),
        )
        .await
        .map_err(|e| format!("Failed to generate embedding: {}", e))?
        .embeddings)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LargeEmbedding {
    pub embeddings: Vec<Vec<f64>>,
    pub texts: Vec<String>,
}
pub async fn generate_large_embedding(
    text: &str,
    chunk_size: Option<usize>,
) -> Result<LargeEmbedding, String> {
    let chunk_size = chunk_size.unwrap_or(MAX_EMBEDDING_SIZE);
    let chars = text.chars().collect::<Vec<char>>();
    let mut char_chunks: Vec<String> = Vec::new();
    let mut start_idx = 0;

    while start_idx < chars.len() {
        let end_idx = std::cmp::min(start_idx + chunk_size, chars.len());
        let mut split_idx = end_idx;

        // Try to find a whitespace within the last chunk
        if end_idx < chars.len() {
            if let Some(whitespace_idx) = chars[start_idx..end_idx]
                .iter()
                .rposition(|&c| c.is_whitespace())
            {
                split_idx = start_idx + whitespace_idx + 1; // +1 to include the whitespace in the current chunk
            }
        }

        let chunk = chars[start_idx..split_idx].iter().collect::<String>();
        char_chunks.push(chunk);
        start_idx = split_idx;
    }

    let mut embeddings: Vec<Vec<f64>> = Vec::new();
    for chunk in char_chunks.clone() {
        let embedding = generate_embedding(&chunk).await?;
        embeddings.push(embedding);
    }

    Ok(LargeEmbedding {
        embeddings,
        texts: char_chunks,
    })
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WebsiteEmbedding {
    pub url: String,
    pub embeddings: Vec<Vec<f64>>,
    pub texts: Vec<String>,
    pub images: Vec<(String, String)>,
    pub link: String,
}

pub async fn get_website_embedding(
    url: &str,
    pw_context: Arc<BrowserContext>,
) -> Result<WebsiteEmbedding, String> {
    lazy_static! {
        static ref PW_SEMAPHORE: Semaphore = Semaphore::new(8);
    }

    #[io_cached(
        map_error = r##" | e | { format!("Failed to cache: {}", e) }"##,
        disk = true,
        convert = r#"{ format!("{}", url) }"#,
        ty = "DiskCache<String, WebsiteEmbedding>"
    )]
    async fn get_website_embedding_cached(
        url: &str,
        pw_context: Arc<BrowserContext>,
    ) -> Result<WebsiteEmbedding, String> {
        let page = pw_context
            .new_page()
            .await
            .map_err(|e| format!("Failed to create page: {}", e))?;
        if page
            .goto_builder(&url)
            .timeout(15000.0)
            .wait_until(playwright::api::DocumentLoadState::NetworkIdle)
            .goto()
            .await
            .is_err()
        {
            page.close(None)
                .await
                .map_err(|_| "Failed to navigate to URL".to_string())?;
            return Err("Failed to navigate to URL".to_string());
        }

        //page.wait_for_timeout(10000.0).await;

        let text_content: String = page
            .eval("document.body.innerText")
            .await
            .map_err(|e| format!("Failed to evaluate JS: {}", e))?;

        let image_data: Vec<(String, Option<String>)> = page
            .eval(
                "
                Array.from(document.querySelectorAll('img')).map((img) => {
                    return [img.src, img.alt || img.title || null];
                })
            ",
            )
            .await
            .map_err(|e| format!("Failed to evaluate JS: {}", e))?;

        //info!("image links: {:?}", image_data);

        page.close(None).await.expect("Failed to close page");

        let res = generate_large_embedding(&text_content, None).await?;
        let embedding = WebsiteEmbedding {
            embeddings: res.embeddings,
            url: url.to_string(),
            texts: res.texts,
            images: image_data
                .into_iter()
                .map(|x| (x.0, x.1.unwrap_or_default()))
                .filter(|x| {
                    x.0.len() < 256
                        && !x.0.is_empty()
                        && x.1.len() < 256
                        && !x.1.is_empty()
                        && x.0.starts_with("http")
                })
                .collect(),
            link: url.to_string(),
        };

        Ok(embedding)
    }

    let _permit = PW_SEMAPHORE
        .acquire()
        .await
        .expect("Failed to acquire semaphore");

    get_website_embedding_cached(url.clone(), pw_context).await
}

//#[io_cached(
//    map_error = r##" | e | { format!("Failed to cache: {}", e) }"##,
//    disk = true,
//    convert = r#"{ format!("{:?}", vec1) }"#,
//    ty = "DiskCache<String, f64>"
//)]
pub fn vec_cos_sim(vec1: &[f64], vec2: &[f64]) -> Result<f64, String> {
    if vec1.len() != vec2.len() {
        warn!("Vector lengths do not match");
        return Err("Vector lengths do not match".to_string());
    }

    let mut dot_product = 0.0;
    let mut vec1_magnitude_squared = 0.0;
    let mut vec2_magnitude_squared = 0.0;

    for i in 0..vec1.len() {
        dot_product += vec1[i] * vec2[i];
        vec1_magnitude_squared += vec1[i] * vec1[i];
        vec2_magnitude_squared += vec2[i] * vec2[i];
    }

    let vec1_magnitude = vec1_magnitude_squared.sqrt();
    let vec2_magnitude = vec2_magnitude_squared.sqrt();

    if vec1_magnitude == 0.0 || vec2_magnitude == 0.0 {
        return Ok(0.0); // Avoid division by zero
    }

    Ok(dot_product / (vec1_magnitude * vec2_magnitude))
}
