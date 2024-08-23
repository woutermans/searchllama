use yew::prelude::*;
use yew_router::prelude::*;
use web_sys::window;

#[derive(Clone, Routable, PartialEq)]
enum Route {
    #[at("/")]
    Home,
    #[at("/:value")]
    Value(i64),
}

struct Model {
    value: i64,
}

enum Msg {
    AddOne,
    SetValue(i64),
}

impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        let value = match Route::from_path(window().unwrap().location().pathname().unwrap().as_str()) {
            Route::Value(val) => val,
            _ => 0,
        };
        Self {
            value,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::AddOne => {
                self.value += 1;
                true
            }
            Msg::SetValue(val) => {
                self.value = val;
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

#[function_component(App)]
fn app() -> Html {
    html! {
        <Router<Route, ()>
            render = Router::render(|switch: Route| {
                match switch {
                    Route::Home => html! { <Model /> },
                    Route::Value(val) => html! { <Model value={val} /> },
                }
            })
        />
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
