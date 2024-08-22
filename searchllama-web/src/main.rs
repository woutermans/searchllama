use std::collections::HashMap;

use futures::{StreamExt, TryStreamExt};
use lazy_static::lazy_static;
use log::{debug, info, warn};
use searchllama_types::{ChatRequest, ChatResponse, Entry, SearchRequest, SearchResponse};
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement; // Import the HtmlInputElement type
use yew::prelude::*;
use yew_hooks::prelude::*;
use yew_markdown::Markdown;
use yew_router::prelude::*;

static MARKDOWN_SOURCE: &str = r#"
## Code
```rust
fn main() {
    println!("hello world !")
}
```

## Math
1) $1+1=2$

2) $e^{i\pi}+1=0$

3)
$$\int_0^{+\infty}\dfrac{\sin(t)}{t}\,dt=\dfrac{\sqrt{\pi}}{2}$$


## Links and images
![](https://raw.githubusercontent.com/wooorm/markdown-rs/8924580/media/logo-monochromatic.svg?sanitize=true)

for markdown documentation, see https://github.com/wooorm/markdown or [here](https://commonmark.org/help/)

## Style
| unstyled | styled    |
| :-----:  | ------    |
| bold     | **bold**  |
| italics  | *italics* |
| strike   | ~strike~  |

> Hey, I am a quote !
> - I don't like numbers
"#;

const API_URL: &str = "http://localhost:3030";

lazy_static! {
    static ref REQWEST_CLIENT: reqwest::Client = reqwest::Client::new();
}

fn handle_search(
    query: UseStateHandle<String>,
    state: UseStateHandle<Model>,
    break_stream: UseStateHandle<bool>,
    summary_context: UseStateHandle<Vec<i32>>,
) {
    let state = state.clone();
    let break_stream = break_stream.clone(); // Clone the state to pass into the closure
    let summary_context = summary_context.clone(); // Clone the context to pass into the closure

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
                    debug!("Received chunk: {}", chunk);

                    if let Ok(search_response) = serde_json::from_str::<SearchResponse>(&chunk) {
                        if let Some(context) = &search_response.summary_context {
                            summary_context.set(context.clone());
                        }
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
                    let mut vec_entries = Vec::from_iter(entries.iter().map(|(_, e)| e.clone()));
                    vec_entries.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

                    // let summary_links = summary
                    // .clone()
                    // .split_whitespace()
                    // .filter(|&s| {
                    // s.starts_with('<') && s.ends_with('>') || s.starts_with("http")
                    // })
                    // .map(|s| s.replace("<", "").replace(">", "").to_string())
                    // .collect::<Vec<String>>();

                    // for link in &summary_links {
                    // summary = summary.replace(link, &format!("\n![]({})", link));
                    // }

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

fn handle_chat(
    message: UseStateHandle<String>,
    context: UseStateHandle<Vec<i32>>,
    state: UseStateHandle<Model>,
) {
    spawn_local(async move {
        let request = ChatRequest {
            message: message.to_string(),
            context: context.to_vec(),
        };

        let body = serde_json::to_string(&request).expect("Failed to serialize request");

        let mut stream = REQWEST_CLIENT
            .post(format!("{}/chat", API_URL))
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .expect("Failed to send request")
            .bytes_stream(); // Convert the response into a stream of bytes

        message.set("".to_string());

        let mut summary = state.summary.to_string();
        summary.push_str(&format!("\n\n\n### User:\n{}\n\n", message.to_string()));
        while let Some(chunk) = stream.next().await {
            if let Ok(chunk) = chunk {
                let chunk_str =
                    std::str::from_utf8(&chunk).expect("Failed to convert bytes to string"); // Convert the byte slice into a UTF-8 string

                let chat_response = match serde_json::from_str::<ChatResponse>(chunk_str.trim()) {
                    Ok(response) => response, // If the JSON string can be parsed into a ChatResponse object, return it
                    Err(err) => {
                        warn!("Failed to parse JSON response: {}", err); // If there's an error parsing the JSON string, log it as a warning and continue with the loop
                        continue;
                    }
                };

                summary.push_str(&chat_response.response); // Append the message to the summary string

                state.set(Model {
                    entries: state.entries.clone(),
                    summary: summary.clone(),
                    is_loading: state.is_loading,
                });
            }
        }
    });
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
    use_favicon("./src/favicon.ico".to_string());

    let state = use_state(|| Model {
        entries: Vec::new(),
        summary: String::new(),
        is_loading: false,
    });
    let break_stream = use_state(|| false); // Add a state to track if the stream has been broken
    let summary_context: UseStateHandle<Vec<i32>> = use_state(|| Vec::new()); // Add a context for the summary

    // Check if starts with q= or not, if starts with q= then remove it and use the rest as query
    let query_param = match props.query.starts_with("q=") {
        true => props.query[2..].to_string(),
        false => "".to_string(),
    };
    let query = use_state(|| query_param.to_string());
    let markdown_switch = use_state(|| true); // TODO: Remove this and use the markdown switch from props instead

    let chat_prompt = use_state(|| "".to_string()); // TODO: Remove this and use the chat prompt from props instead

    if !query_param.is_empty() && state.entries.len() == 0 {
        info!("Home component rendered with query: {}", &query_param);
        let query = query.clone();
        let state = state.clone();
        let break_stream = break_stream.clone();
        let summary_context = summary_context.clone();

        handle_search(query, state, break_stream, summary_context)
    }

    let on_switch = {
        let markdown_switch = markdown_switch.clone();
        Callback::from(move |e: InputEvent| {
            info!("Markdown Switch changed to {}", !*markdown_switch); // TODO: Remove this and use the markdown switch from props instead

            let input: HtmlInputElement = e.target_unchecked_into();
            let checked = input.checked(); // Get the checked state directly
            markdown_switch.set(checked);
        })
    };

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
        let break_stream = break_stream.clone();
        let summary_context = summary_context.clone();
        Callback::from(move |e: KeyboardEvent| {
            if e.key() == "Enter" {
                handle_search(
                    query.clone(),
                    state.clone(),
                    break_stream.clone(),
                    summary_context.clone(),
                );
            }
        })
    };

    let on_chat_input = {
        let chat_prompt = chat_prompt.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into(); // Convert event target into HtmlInputElement
            chat_prompt.set(input.value());
        })
    };

    let on_chat_keypress = {
        let chat_prompt = chat_prompt.clone();
        let summary_context = summary_context.clone();
        let state = state.clone();
        Callback::from(move |e: KeyboardEvent| {
            if e.key() == "Enter" {
                handle_chat(chat_prompt.clone(), summary_context.clone(), state.clone());
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
                            <div class="toggle-container">
                                <input type="checkbox" id="toggle-switch" class="toggle-switch" oninput={on_switch}/>
                                <label for="toggle-switch" class="toggle-label">
                                    <span class="toggle-text markdown-text">{"Markdown"}</span>
                                    <span class="toggle-text raw-text">{"Raw"}</span>
                                </label>
                            </div>
                            <div class="summary-text">
                                //<Markdown src={MARKDOWN_SOURCE}/>
                                {if !*markdown_switch {
                                    html! {<Markdown src={state.summary.clone()}/>}
                                } else{
                                    html!{{&state.summary}}
                                }}
                            </div>
                            <div class="summary-message-box-container">
                                <input type="text" class="summary-message-box" value={(*chat_prompt).clone()} oninput={on_chat_input} onkeypress={on_chat_keypress}/>
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
