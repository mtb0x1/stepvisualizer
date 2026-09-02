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

const PRESETS: [QualityPreset; 3] = [
    QualityPreset::Coarse,
    QualityPreset::Balanced,
    QualityPreset::Fine,
];

#[function_component(UploadBar)]
pub fn upload_bar(props: &UploadBarProps) -> Html {
    trace_span!("upload_bar");

    let preset_buttons = PRESETS
        .iter()
        .map(|&preset| {
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
        })
        .collect::<Html>();

    html! {
        <div class="file-input-container">
            <div class="quality-preset-bar">
                <span class="quality-label">{ "Quality:" }</span>
                <div class="quality-btn-group">
                    { preset_buttons }
                </div>
            </div>
            <div class="file-upload-group">
                <label for="file-input" class="custom-file-upload">
                    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="upload-icon">
                        <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
                        <polyline points="17 8 12 3 7 8"/>
                        <line x1="12" y1="3" x2="12" y2="15"/>
                    </svg>
                    { "Choose STEP File" }
                </label>
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
        </div>
    }
}
