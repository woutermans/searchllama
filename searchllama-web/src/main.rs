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

impl Routable for Route {
    fn from_path(path: &str) -> Option<Self> {
        if let Some(value) = path.strip_prefix('/').and_then(|s| s.parse::<i64>().ok()) {
            Some(Route::Value(value))
        } else {
            None
        }
    }
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
        let value = if let Some(Route::Value(val)) = Router::current_route() {
            val
        } else {
            0
        };
        Self { value }
    }

    // ... rest of the code remains unchanged
}
