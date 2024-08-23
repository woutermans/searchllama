use std::sync::Mutex;

use futures::{future, Stream, StreamExt};
use lazy_static::lazy_static;
use log::{debug, info};
use types::{ChatRequest, ChatResponse, SearchRequest, SearchResponse};

pub mod types;

lazy_static! {
    static ref CLIENT: reqwest::Client = reqwest::Client::new();
}

pub struct Searchllama {
    api_url: String,
    context: Mutex<Option<Vec<i32>>>,
}

impl Searchllama {
    pub fn new(api_url: &str) -> Self {
        Self {
            api_url: api_url.to_string(),
            context: Mutex::new(None),
        }
    }
    pub async fn search(&self, query: &str) -> impl Stream<Item = Result<SearchResponse, String>> {
        let query = SearchRequest {
            query: query.into(),
        };
        //let query_json = serde_json::to_string(&query).unwrap();

        let stream = CLIENT
            .post(&format!("{}/search", self.api_url))
            .header("Content-Type", "application/json")
            .json(&query)
            .send()
            .await
            .expect("Failed to send request")
            .bytes_stream();

        debug!("Sent request: {:?}", query);

        let stream = stream.filter(|res| future::ready(res.is_ok())).map(|res| {
            serde_json::from_slice::<SearchResponse>(&res.unwrap()).map_err(|e| e.to_string())
        });

        stream
    }
    pub async fn chat(
        &self,
        message: &str,
        context: Option<Vec<i32>>,
    ) -> impl Stream<Item = Result<Vec<ChatResponse>, String>> {
        let query = ChatRequest {
            message: message.into(),
            context: context.unwrap_or(vec![]),
        };

        let stream = CLIENT
            .post(&format!("{}/chat", self.api_url))
            .header("Content-Type", "application/json")
            .json(&query)
            .send()
            .await
            .expect("Failed to send request")
            .bytes_stream();

        let stream = stream.filter(|res| future::ready(res.is_ok())).map(|res| {
            let str = String::from_utf8(res.unwrap().to_vec()).expect("Invalid UTF-8");
            let responses = str.split("\t").collect::<Vec<&str>>();

            //info!("Received responses: {:?}", responses);

            let responses = responses
                .into_iter()
                .filter(|s| !s.is_empty())
                .map(|s| serde_json::from_str(s.trim()).map_err(|e| e.to_string()))
                .collect::<Result<Vec<ChatResponse>, String>>();

            responses
        });

        stream
    }
}

impl Default for Searchllama {
    fn default() -> Self {
        Searchllama::new("http://127.0.0.1:3030")
    }
}
