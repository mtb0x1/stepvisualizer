use crate::trace_span;
use web_sys::Event;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct UploadBarProps {
    pub is_processing: bool,
    pub on_file_change: Callback<Event>,
}

#[function_component(UploadBar)]
pub fn upload_bar(props: &UploadBarProps) -> Html {
    trace_span!("upload_bar");
    html! {
        <div class="file-input-container">
            <label for="file-input">{ "Select a STEP file: " }</label>
            <input
                type="file"
                accept=".step,.stp"
                id="file-input"
                disabled={props.is_processing}
                onchange={props.on_file_change.clone()}
            />
            {
                if props.is_processing {
                    html! { <span class="processing-hint">{ "Processing STEP..." }</span> }
                } else {
                    Html::default()
                }
            }
        </div>
    }
}
