use std::collections::HashMap;

use futures::{StreamExt, TryStreamExt};
use lazy_static::lazy_static;
use log::info;
use searchllama_types::{Entry, SearchRequest, SearchResponse};
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement; // Import the HtmlInputElement type
use yew::prelude::*;
use yew_markdown::Markdown;
use yew_router::prelude::*;

const API_URL: &str = "http://localhost:3030";

lazy_static! {
    static ref REQWEST_CLIENT: reqwest::Client = reqwest::Client::new();
}

#[derive(Clone, Routable, PartialEq)]
enum Route {
    #[at("/")]
    Home,
    #[at("/:query")]
    Query { query: String },
}

#[derive(Properties, PartialEq)]
pub struct HomeProps {
    pub query: String,
}
struct Model {
    entries: Vec<Entry>,
    summary: String,
    is_loading: bool,
}

#[function_component(Home)]
fn home(props: &HomeProps) -> Html {
    let state = use_state(|| Model {
        entries: Vec::new(),
        summary: String::new(),
        is_loading: false,
    });
    let break_stream = use_state(|| false); // Add a state to track if the stream has been broken

    fn handle_search(
        query: UseStateHandle<String>,
        state: UseStateHandle<Model>,
        break_stream: UseStateHandle<bool>,
    ) {
        let state = state.clone();
        let break_stream = break_stream.clone(); // Clone the state to pass into the closure

        state.set(Model {
            entries: vec![],
            summary: String::new(),
            is_loading: true,
        });

        spawn_local(async move {
            let body = SearchRequest {
                query: (*query).clone(),
            };
            let body = serde_json::to_string(&body).unwrap();

            let response = REQWEST_CLIENT
                .post(format!("{}/search", API_URL))
                .header("Content-Type", "application/json")
                .body(body)
                .send()
                .await
                .expect("Failed to send request");

            let mut response_stream = response.bytes_stream();

            while *break_stream {
                std::thread::sleep(std::time::Duration::from_millis(100)); // Wait for a short time before checking again
            }

            state.set(Model {
                entries: vec![],
                summary: String::new(),
                is_loading: true,
            });

            let mut entries = HashMap::new();
            let mut summary = String::new();
            while let Some(_chunk) = response_stream.next().await {
                if let Ok(chunk) = _chunk {
                    let chunk = String::from_utf8_lossy(&chunk).to_string();

                    let chunks = chunk.split("\n\n").collect::<Vec<&str>>();

                    let filtered_chunks = chunks
                        .into_iter()
                        .filter(|&s| !s.is_empty())
                        .collect::<Vec<&str>>();

                    let trimmed_chunks = filtered_chunks
                        .into_iter()
                        .map(|s| s.trim())
                        .collect::<Vec<&str>>();

                    let chunks = trimmed_chunks;

                    for chunk in chunks {
                        info!("Received chunk: {}", chunk);

                        if let Ok(search_response) = serde_json::from_str::<SearchResponse>(&chunk)
                        {
                            entries.extend(
                                search_response
                                    .results
                                    .into_iter()
                                    .map(|e| (e.url.clone(), e)),
                            );
                            summary.push_str(&search_response.summary);
                        }

                        if *break_stream {
                            break_stream.set(false);
                            return;
                        }
                        let mut vec_entries =
                            Vec::from_iter(entries.iter().map(|(_, e)| e.clone()));
                        vec_entries.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
                        state.set(Model {
                            entries: vec_entries,
                            is_loading: true,
                            summary: summary.clone(),
                        });
                    }
                } else {
                    break;
                }
            }
            info!("All chunks received");

            let mut vec_entries = Vec::from_iter(entries.iter().map(|(_, e)| e.clone()));
            vec_entries.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
            state.set(Model {
                entries: vec_entries,
                summary,
                is_loading: false,
            });

            /*let search_response: SearchResponse =
                serde_json::from_str(&response).expect("Failed to parse response");

            state.set(Model {
                entries: search_response.results,
                is_loading: true,
            });*/
        });
    }

    // Check if starts with q= or not, if starts with q= then remove it and use the rest as query
    let query_param = match props.query.starts_with("q=") {
        true => props.query[2..].to_string(),
        false => "".to_string(),
    };
    let query = use_state(|| query_param.to_string());
    if !query_param.is_empty() && state.entries.len() == 0 {
        info!("Home component rendered with query: {}", &query_param);
        let query = query.clone();
        let state = state.clone();
        let break_stream = break_stream.clone();

        handle_search(query, state, break_stream)
    }

    let oninput = {
        let query = query.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into(); // Convert event target into HtmlInputElement
            query.set(input.value());
        })
    };

    let onkeypress = {
        let state = state.clone();
        let query = query.clone();
        Callback::from(move |e: KeyboardEvent| {
            if e.key() == "Enter" {
                let state = state.clone();
                let query = query.clone();
                let break_stream = break_stream.clone();

                handle_search(query, state, break_stream);
            }
        })
    };

    let entries_html = state
        .entries
        .iter()
        .map(|entry| {
            html! {
                <div class="entry">
                    <div class="entry-header">
                        <div class="entry-title">
                            <a href={entry.url.clone()} target="_blank">{&entry.title}</a>
                        </div>
                        <div class="entry-url">
                            <p>{&entry.url}</p>
                        </div>
                        <div class="entry-score">
                            <p>{format!(" [{:.2}]", entry.score)}</p>
                        </div>
                    </div>
                    <div class="entry-description">
                        <p>{&entry.description}</p>
                    </div>
                </div>
            }
        })
        .collect::<Html>();

    // Show gif when is_loading is true
    html! {
        <div class="search-page">
            <div class="entries">
                {if !state.summary.is_empty(){
                    html! {
                        <div class="summary-container">
                            <div class="summary-text">
                                <Markdown src={state.summary.clone()}/>
                            </div>
                        </div>
                    }
                } else {
                    Html::default()
                }}
                {entries_html}
            </div>
            <div class="search-bar-container">
                <input type="text" class="search-bar" value={(*query).clone()} {oninput} {onkeypress} placeholder="Enter search query" />
                {if state.is_loading {
                    html! { <img src="https://media.tenor.com/uvs84qLH_l8AAAAi/nahh-nah.gif" alt="Loading..." class="loading-gif" /> }
                } else {
                    Html::default()
                }}
            </div>
        </div>
    }
}

fn switch(route: Route) -> Html {
    match route {
        Route::Home => html! {
            <Home query={""} />
        },
        Route::Query { query } => html! {
            <Home query={query.clone()} />
        },
    }
}

#[function_component(App)]
fn app() -> Html {
    html! {
        <BrowserRouter>
            <Switch<Route> render={switch} /> // <- must be child of <BrowserRouter>
        </BrowserRouter>
    }
}

fn main() {
    console_log::init_with_level(log::Level::Debug).expect("Failed to initialize logger");

    yew::Renderer::<App>::new().render();
}
