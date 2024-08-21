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
}