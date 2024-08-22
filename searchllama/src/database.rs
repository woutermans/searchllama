use std::sync::Arc;

use bytemuck::{cast_slice, try_cast_slice};
use futures::TryStreamExt;
use log::info;
use playwright::api::BrowserContext;
use pollster::FutureExt;
use sqlx::Row;
use sqlx::{migrate::MigrateDatabase, Executor, Sqlite, SqlitePool};

use crate::embedding::{self, vec_cos_sim};
use crate::search;

struct DBWrapper {
    pool: sqlx::Pool<Sqlite>,
}
impl Drop for DBWrapper {
    fn drop(&mut self) {
        self.pool.close().block_on();
    }
}

async fn get_db_pool() -> DBWrapper {
    if !tokio::fs::try_exists("data.db").await.unwrap() {
        info!("Creating database...");
        Sqlite::create_database("sqlite://data.db").await.unwrap();

        let pool = SqlitePool::connect("sqlite://data.db").await.unwrap();

        pool.execute(
            "CREATE TABLE IF NOT EXISTS indices (
                url TEXT NOT NULL UNIQUE,
                title TEXT,
                title_embedding BLOB,
                body_embedding_count INTEGER,
                body_embeddings BLOB,
                summary TEXT
        )
        ",
        )
        .await
        .unwrap();

        pool.close().await;
        info!("closed: {}", pool.is_closed());
    }
    DBWrapper {
        pool: SqlitePool::connect("sqlite://data.db").await.unwrap(),
    }
}

pub async fn update_entry(
    url: &str,
    title: &str,
    summary: &str,
    title_embedding: Vec<f64>,
    body_embeddings: Vec<Vec<f64>>,
) {
    let wrapper = get_db_pool().await;

    let title_bytes = cast_slice(&title_embedding);

    let body_count = body_embeddings.len() as i64;

    let mut body_bytes: Vec<u8> = Vec::new();
    body_embeddings.into_iter().for_each(|v| {
        let v_bytes: &[u8] = cast_slice(&v);
        body_bytes.extend(v_bytes);
    });

    wrapper.pool.execute(
        sqlx::query("INSERT INTO indices (url, title, title_embedding, body_embedding_count, body_embeddings, summary) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(url) DO UPDATE SET title=excluded.title, title_embedding=excluded.title_embedding, body_embedding_count=excluded.body_embedding_count, body_embeddings=excluded.body_embeddings, summary=excluded.summary")
            .bind(url)
            .bind(title)
            .bind(&title_bytes)
            .bind(body_count)
            .bind(&body_bytes)
            .bind(summary)
    ).await.unwrap();
}

pub async fn query_db(query_embedding: &[f64]) -> Vec<(String, String, String, f64)> {
    let wrapper = get_db_pool().await;
    let pool = &wrapper.pool;

    // Fetch all rows from the users table
    let mut rows = sqlx::query("SELECT * FROM indices").fetch(pool);

    let mut entries_with_sim = Vec::new();
    while let Some(row) = rows.try_next().await.unwrap() {
        // map the row into a user-defined domain type
        let url: &str = row.try_get("url").unwrap();
        let title: &str = row.try_get("title").unwrap();
        let title_embedding: Vec<u8> = row.try_get("title_embedding").unwrap();
        let body_embedding_count: i64 = row.try_get("body_embedding_count").unwrap();
        let body_embeddings: Vec<u8> = row.try_get("body_embeddings").unwrap();
        let summary: &str = row.try_get("summary").unwrap();

        let title_embedding: &[f64] = match try_cast_slice(&title_embedding) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let body_embedding: Vec<f64> = match try_cast_slice(&body_embeddings) {
            Ok(v) => v.to_vec(),
            Err(_) => continue,
        };
        // Split body_embeddings in 'body_embedding_count' parts
        let body_embeddings: Vec<Vec<f64>> = body_embedding
            .chunks(body_embedding.len() / body_embedding_count as usize)
            .map(|chunk| chunk.to_vec())
            .collect();

        let similarity =
            search::calculate_entry_similarity(&query_embedding, title_embedding, &body_embeddings);

        // Remove entry if similarity weird
        if similarity < -10.0 || similarity > 10.0 {
            continue;
        }

        entries_with_sim.push((
            url.to_owned(),
            title.to_owned(),
            summary.to_owned(),
            similarity,
        ));
    }

    entries_with_sim.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap());

    entries_with_sim
}
