use serde::{Deserialize, Serialize};


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SearchRequest {
    pub query: String
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Entry {
    pub score: f64,
    pub url: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SearchResponse {
    pub results: Vec<Entry>,
    pub summary: String,
    pub summary_context: Option<Vec<i32>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatRequest {
    pub message: String,
    pub context: Vec<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatResponse {
    pub response: String,
    pub context: Option<Vec<i32>>,
}