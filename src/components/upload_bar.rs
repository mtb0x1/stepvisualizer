//! STEP file picker with its processing hint and tessellation-quality preset.
use crate::common::constants::QualityPreset;
use crate::trace_span;
use web_sys::Event;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct UploadBarProps {
    pub is_processing: bool,
    pub on_file_change: Callback<Event>,
    pub quality_preset: QualityPreset,
    pub on_quality_change: Callback<QualityPreset>,
}

#[function_component(UploadBar)]
pub fn upload_bar(props: &UploadBarProps) -> Html {
    trace_span!("upload_bar");

    let presets = [QualityPreset::Coarse, QualityPreset::Balanced, QualityPreset::Fine];

    let preset_buttons = presets.iter().map(|&preset| {
        let on_quality_change = props.on_quality_change.clone();
        let is_active = props.quality_preset == preset;
        let class = if is_active {
            "quality-btn quality-btn-active"
        } else {
            "quality-btn"
        };
        let onclick = Callback::from(move |_| on_quality_change.emit(preset));
        let title = preset.tooltip();
        html! {
            <button {class} {onclick} type="button" {title}>
                { preset.label() }
            </button>
        }
    }).collect::<Html>();

    html! {
        <div class="file-input-container">
            <div class="quality-preset-bar">
                <span class="quality-label">{ "Quality:" }</span>
                { preset_buttons }
            </div>
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
