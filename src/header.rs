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
    html! {
        <header class="app-header">
            // <div class="header-toolbar">
            //     <i class="fas fa-folder-open"></i>
            //     <i class="fas fa-save"></i>
            //     <span class="divider" />
            //     <i class="fas fa-undo"></i>
            //     <i class="fas fa-redo"></i>
            // </div>
            <div class="file-name">
                { props.file_name.clone().unwrap_or_else(|| "".to_string()) }
            </div>
            <div class="header-toolbar" />
        </header>
    }
}
