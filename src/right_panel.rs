use crate::trace_span;
use crate::{common::Metadata, components::details_panel::DetailsPanel};
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct RightPanelProps {
    pub metadata: Option<Metadata>,
    pub on_calculate_volume: Callback<()>,
    pub on_calculate_surface: Callback<()>,
}

#[function_component(RightPanel)]
pub fn right_panel(props: &RightPanelProps) -> Html {
    trace_span!("right_panel");
    html! {
        <div class="right-panel">
            <DetailsPanel
                metadata={props.metadata.clone()}
                on_calculate_volume={props.on_calculate_volume.clone()}
                on_calculate_surface={props.on_calculate_surface.clone()}
            />
        </div>
    }
}
