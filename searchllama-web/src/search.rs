use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use futures::StreamExt;
use log::{error, info};
use searchllama_types::types::Entry;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

pub struct SearchParams {
    pub query: String,
    pub entries: Vec<Entry>,
    pub on_entries_update: Callback<(Vec<Entry>, String, Option<Vec<i32>>)>,
}

impl SearchParams {
    pub fn new(
        query: String,
        entries: Vec<Entry>,
        on_entries_update: Callback<(Vec<Entry>, String, Option<Vec<i32>>)>,
    ) -> Self {
        Self {
            query,
            entries,
            on_entries_update,
        }
    }

    pub fn search(&self) {
        let query = self.query.clone();
        let on_entries_update = self.on_entries_update.clone();

        spawn_local(async move {
            let mut response_stream = searchllama_types::Searchllama::default()
                .search(&query.to_string())
                .await;

            let mut entries: HashMap<String, Entry> = HashMap::new();
            while let Some(entry) = response_stream.next().await {
                if let Ok(response) = entry {
                    entries.extend(
                        response
                            .results
                            .into_iter()
                            .map(|res| (res.url.clone(), res)),
                    );
                    let mut entries_vec = entries.values().cloned().collect::<Vec<Entry>>();
                    entries_vec.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap().reverse());
                    on_entries_update.emit((
                        entries_vec,
                        response.summary,
                        response.summary_context,
                    ));
                }
            }

            info!("Done searching");
        });
    }
}

pub struct ChatParams {
    pub prompt: String,
    pub context: Option<Vec<i32>>,
    pub on_chat_update: Callback<(String, Option<Vec<i32>>)>,
}

impl ChatParams {
    pub fn new(
        prompt: String,
        context: Option<Vec<i32>>,
        on_chat_update: Callback<(String, Option<Vec<i32>>)>,
    ) -> Self {
        Self {
            prompt,
            context,
            on_chat_update,
        }
    }
    pub fn send_chat(&self) {
        let prompt = self.prompt.clone();
        let context = self.context.clone();
        let on_chat_update = self.on_chat_update.clone();

        spawn_local(async move {
            let mut response_stream = searchllama_types::Searchllama::default()
                .chat(&prompt, context)
                .await;

            while let Some(response) = response_stream.next().await {
                match response {
                    Ok(responses) => {
                        for chunk in responses {
                            on_chat_update.emit((chunk.response, chunk.context));
                        }
                    }
                    Err(e) => error!("Error: {}", e),
                }
            }
        });
    }
}
