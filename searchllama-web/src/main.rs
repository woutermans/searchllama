use yew::prelude::*;
use yew_router::prelude::*;
use lazy_static::lazy_static;
use std::collections::HashMap;

struct Model {
    value: i64,
}

enum Msg {
    SetValue(i64),
    AddOne,
}

impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            value: 0,
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

#[derive(Routable, Clone)]
enum Route {
    #[at("/")]
    Home,
    #[not_found]
    #[at("/404")]
    NotFound,
}

struct RouterComponent;

impl Component for RouterComponent {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let navigator = ctx.link().navigator().unwrap();
        if let Some((_, query)) = navigator.location().query_pairs().find(|&(ref key, _)| key == "q") {
            if let Ok(val) = query.parse::<i64>() {
                ctx.link().send_message(Msg::SetValue(val));
            }
        }
        Self {}
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        false
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <Router<Route> render={Switch::render(switch)} />
        }
    }
}

fn switch(routes: Route) -> Html {
    match routes {
        Route::Home => html! { <Model /> },
        Route::NotFound => html! { <h1>{ "404" }</h1> },
    }
}

fn main() {
    yew::Renderer::<RouterComponent>::new().render();
}
