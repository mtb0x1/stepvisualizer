//! Top bar showing the loaded file's name.
use crate::trace_span;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct HeaderProps {
    #[prop_or_default]
    pub file_name: Option<String>,
}

#[function_component(Header)]
pub fn header(props: &HeaderProps) -> Html {
    trace_span!("header");
    let display_name = props.file_name.as_deref().unwrap_or_default();
    html! {
        <header class="app-header">
            <div class="file-name">
                { display_name }
            </div>
            <div class="header-toolbar" />
        </header>
    }
}
