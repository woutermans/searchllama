use yew::prelude::*;
use yew_router::prelude::*;

// Define properties for the Model component
#[derive(Properties, PartialEq)]
struct ModelProps {
    value: i64,
}

// Define the main model structure
struct Model {
    value: i64,
}

// Define messages for component state updates
enum Msg {
    AddOne,
}

// Define the routing enum
#[derive(Routable, PartialEq, Eq, Clone, Debug)]
pub enum Route {
    #[at("/")]
    Home,
    #[at("/:value")]
    Search { value: i64 },
}

// Implement the Component trait for Model
impl Component for Model {
    type Message = Msg;
    type Properties = ModelProps;

    fn create(ctx: &Context<Self>) -> Self {
        Self {
            value: ctx.props().value
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::AddOne => {
                self.value += 1;
                
                // Get access to the navigator for programmatic navigation
                if let Some(navigator) = ctx.link().navigator() {
                    navigator.push(&Route::Search { value: self.value });
                }

                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        html! {
            <div>
                <button onclick={link.callback(|_| Msg::AddOne)}>{ "+1" }</button>
                <p>{ self.value }</p>
            </div>
        }
    }
}

// Define the switch function for routing
fn switch(routes: Route) -> Html {
    match routes {
        Route::Home => html! { <Model value=0 /> },
        Route::Search { value } => html! { <Model value={value} /> },
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
