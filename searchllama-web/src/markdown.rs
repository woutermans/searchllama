use log::info;
use markdown::to_html;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct MarkdownProps {
    pub src: String,
}

pub enum Msg {}

pub struct Markdown {
    src: String,
}

impl Component for Markdown {
    type Message = Msg;
    type Properties = MarkdownProps;

    fn create(ctx: &Context<Self>) -> Self {
        info!("Creating Markdown component");

        let props = ctx.props();
        Self {
            src: props.src.clone(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, _msg: Self::Message) -> bool {
        info!("Updating Markdown component");

        let props = ctx.props();
        if props.src != self.src {
            self.src = props.src.clone();
        }
        true
    }

    fn changed(&mut self, ctx: &Context<Self>, _old_props: &Self::Properties) -> bool {
        info!("Markdown props changed");

        let props = ctx.props();
        if props.src != self.src {
            self.src = props.src.clone();
        }
        true
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        info!("Rendering Markdown component");

        let html_content = to_html(&self.src);
        Html::from_html_unchecked(AttrValue::from(html_content))
    }
}
