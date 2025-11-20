use crate::trace_span;
use crate::{common::Metadata, components::details_panel::DetailsPanel};
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct RightPanelProps {
    pub metadata: Option<Metadata>,
}

#[function_component(RightPanel)]
pub fn right_panel(props: &RightPanelProps) -> Html {
    trace_span!("right_panel");
    html! {
        <div class="right-panel">
            <DetailsPanel metadata={props.metadata.clone()} />
        </div>
    }
}
