use yew::prelude::*;
use yew_router::{BrowserRouter, Switch};
use web_sys::UrlSearchParams;
use std::str::FromStr;

struct Model {
    value: String,
}

enum Msg {
    UpdateValue(String),
}

impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        let location = web_sys::window().unwrap().location();
        let search_params = UrlSearchParams::new(Some(&location.search().unwrap())).unwrap();
        let value = search_params.get("q").unwrap_or_default();
        
        Self {
            value,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::UpdateValue(new_value) => {
                self.value = new_value;
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        html! {
            <div>
                <input type="text" value={self.value.clone()} oninput={link.callback(|e: InputEvent| Msg::UpdateValue(event_target_value(&e).unwrap()))} />
                <button onclick={link.callback(|_| Msg::UpdateValue("".to_string()))}>{ "Clear" }</button>
            </div>
        }
    }
}

#[function_component(App)]
fn app() -> Html {
    html! {
        <BrowserRouter>
            <Switch<Route> render={switch::render!(route => html!{<>})} />
        </BrowserRouter>
    }
}

enum Route {
    Search { query: String },
}

impl From<Route> for Option<HtmlElement> {
    fn from(_route: Route) -> Self {
        unimplemented!()
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
