use yew::prelude::*;

struct Model {
    count: i32,
}

enum Msg {
    AddOne,
}

impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        Model { count: 0 }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::AddOne => {
                self.count += 1;
                true // Indicates that the state has changed and a re-render is needed
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <div>
                <h1>{ "Hello, Yew!" }</h1>
                <p>{ self.count }</p>
                <button onclick=ctx.link().callback(|_| Msg::AddOne)>{ "+1" }</button>
            </div>
        }
    }
}

fn main() {
    yew::start_app::<Model>();
}
