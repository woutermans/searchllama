use log::info;
use search::{ChatParams, SearchParams};
use searchllama_types::types::Entry;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_hooks::prelude::*;
use yew_router::prelude::*;

use markdown::Markdown;

mod markdown;
mod search;

// Define properties for the Model component
#[derive(Properties, PartialEq)]
struct ModelProps {
    query: String,
}

// Define the main model structure
struct Model {
    query: String,
    entries: Vec<Entry>,
    summary: String,
    chat_prompt: String,
    summary_embedding: Option<Vec<i32>>,
}

// Define messages for component state updates
enum Msg {
    SearchInput(InputEvent),
    Search,
    UpdateEntries((Vec<Entry>, String, Option<Vec<i32>>)),
    Chat,
    ChatInput(InputEvent),
    UpdateChat((String, Option<Vec<i32>>)),
}

// Define the routing enum
#[derive(Routable, PartialEq, Eq, Clone, Debug)]
pub enum Route {
    #[at("/")]
    Home,
    #[at("/:query")]
    Search { query: String },
}

// Implement the Component trait for Model
impl Component for Model {
    type Message = Msg;
    type Properties = ModelProps;

    fn create(ctx: &Context<Self>) -> Self {
        Self {
            query: ctx.props().query.clone(),
            entries: Vec::new(),
            summary: String::new(),
            chat_prompt: String::new(),
            summary_embedding: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SearchInput(input) => {
                let element: HtmlInputElement = input.target_unchecked_into();
                self.query = element.value();
                true
            }
            Msg::Search => {
                // Get access to the navigator for programmatic navigation
                if let Some(navigator) = ctx.link().navigator() {
                    navigator.push(&Route::Search {
                        query: self.query.clone(),
                    });
                }
                self.summary = String::new();
                self.entries = Vec::new();

                let on_entries_update = ctx.link().callback(|entries| Msg::UpdateEntries(entries));
                SearchParams::new(self.query.clone(), self.entries.clone(), on_entries_update)
                    .search();

                true
            }
            Msg::Chat => {
                self.summary
                    .push_str(format!("\n\n\n\n## User: {}\n\nLLM: ", self.chat_prompt).as_str());

                let on_chat_update = ctx.link().callback(|chat| Msg::UpdateChat(chat));
                ChatParams::new(
                    self.chat_prompt.clone(),
                    self.summary_embedding.clone(),
                    on_chat_update,
                )
                .send_chat();
                self.chat_prompt = String::new();

                true
            }
            Msg::UpdateEntries((entries, summary, summary_embedding)) => {
                self.entries = entries;
                self.summary.push_str(&summary);
                if summary_embedding.is_some() {
                    self.summary_embedding = summary_embedding;
                }
                true
            }
            Msg::ChatInput(input) => {
                let element: HtmlInputElement = input.target_unchecked_into();
                self.chat_prompt = element.value(); // Update the query with the new value from the chat box
                true
            }
            Msg::UpdateChat((message, context)) => {
                self.summary.push_str(&message);
                if context.is_some() {
                    self.summary_embedding = context;
                }
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        use_favicon("./assets/favicon.ico".to_string());

        info!("entries.len(): {}", self.entries.len());

        let link = ctx.link();
        let entries = self
            .entries
            .iter()
            .map(|entry| {
                html! {
                    <div class="entry-card">
                        <h3 class="entry-title">
                            <a href={entry.url.clone()} target="_blank" rel="noopener noreferrer">
                                { &entry.title }
                            </a>
                        </h3>
                        <span class="entry-score">{ format!("[{:.2}]", entry.score) }</span>
                        <p class="entry-description">{ &entry.description }</p>
                    </div>
                }
            })
            .collect::<Html>();

        html! {
            <div class="app-container">
                <div class="content-area">
                    <div class="summary-section">
                        {if !self.summary.is_empty(){
                            html! {
                        <>
                        <h2>{"Summary"}</h2>
                        <div class="markdown-body">
                            <Markdown src={ self.summary.clone() } />
                        </div>
                        </>
                            }
                        } else {
                            html! {
                                <h2>{"Chat"}</h2>
                            }
                        }}
                        <div class="chat-input-area">
                            <input
                                type="text"
                                class="chat-input"
                                placeholder="Ask a question"
                                value={self.chat_prompt.clone()}
                                oninput={link.callback(|e: InputEvent| Msg::ChatInput(e))}
                            />
                            <button class="chat-button" onclick={link.callback(|_| Msg::Chat)}>
                                {"Ask"}
                            </button>
                        </div>
                    </div>
                    <div class="entries-section">
                        <h2>{"Entries"}</h2>
                        { entries }
                    </div>
                </div>
                <div class="search-bar">
                    <input
                        type="text"
                        class="search-input"
                        placeholder="Search..."
                        value={self.query.clone()}
                        oninput={link.callback(|e: InputEvent| Msg::SearchInput(e))}
                    />
                    <button class="search-button" onclick={link.callback(|_| Msg::Search)}>
                        {"Search"}
                    </button>
                </div>
            </div>
        }
    }
}

// Define the switch function for routing
fn switch(routes: Route) -> Html {
    match routes {
        Route::Home => html! { <Model query={String::new()} /> },
        Route::Search { query } => html! { <Model query={query} /> },
    }
}

// Define the main app component
#[function_component(App)]
fn app() -> Html {
    html! {
        <BrowserRouter>
            <Switch<Route> render={switch} />
        </BrowserRouter>
    }
}

// Entry point
fn main() {
    console_log::init_with_level(log::Level::Trace).expect("Failed to initialize logger");

    yew::Renderer::<App>::new().render();
}
