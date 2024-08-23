use yew::prelude::*;
use yew_router::prelude::*;

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

    fn create(ctx: &Context<Self>) -> Self {
        let mut value = 0;
        if let Some(query) = ctx.link().location().query() {
            if let Some(q_value) = query.get("q") {
                if let Ok(parsed_value) = q_value.parse::<i64>() {
                    value = parsed_value;
                }
            }
        }
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
            Msg::SetValue(new_value) => {
                self.value = new_value;
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

#[derive(Clone, Routable, PartialEq)]
enum Route {
    #[at("/")]
    Home,
}

fn switch(routes: &Route) -> Html {
    match routes {
        Route::Home => html! { <Model /> },
    }
}

#[function_component(App)]
fn app() -> Html {
    html! {
        <BrowserRouter>
            <Switch<Route> render={switch} />
        </BrowserRouter>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
