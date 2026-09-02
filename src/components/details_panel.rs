use crate::common::constants::NA;
use crate::common::utils::{format_list_or_na, format_or_na};
use crate::common::{BoundingBox, Metadata};
use crate::trace_span;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct DetailsPanelProps {
    pub metadata: Option<Metadata>,
    pub on_calculate_volume: Callback<()>,
    pub on_calculate_surface: Callback<()>,
}

/// Helper to render a consistent key-value definition row.
fn detail_row(label: &'static str, value: impl Into<Html>) -> Html {
    html! {
        <div class="detail-item">
            <dt class="detail-label">{ label }</dt>
            <dd>{ value.into() }</dd>
        </div>
    }
}

/// Helper to format bounding box minimum and maximum coordinates.
fn format_bbox(bounding_box: Option<&BoundingBox>, unit: Option<&str>) -> Html {
    if let Some(bb) = bounding_box {
        let unit_suffix = match unit {
            Some(u) if !u.is_empty() => format!(" {}", u),
            _ => String::new(),
        };
        html! {
            <>
                <span class="bbox-value">
                    { format!("min: {:.3}, {:.3}, {:.3}{}", bb.min[0], bb.min[1], bb.min[2], unit_suffix) }
                </span>
                <br/>
                <span class="bbox-value">
                    { format!("max: {:.3}, {:.3}, {:.3}{}", bb.max[0], bb.max[1], bb.max[2], unit_suffix) }
                </span>
            </>
        }
    } else {
        html! { { NA } }
    }
}

/// Helper to format volume with units or render the calculate trigger.
fn format_volume(volume: Option<f64>, unit: Option<&str>, on_calc: Callback<()>) -> Html {
    if let Some(vol) = volume {
        let unit_display = match unit {
            Some(u) if !u.is_empty() => format!(" {}³", u),
            _ => String::new(),
        };
        html! { format!("{:.4}{}", vol, unit_display) }
    } else {
        html! {
            <span
                class="calculate-link"
                onclick={move |_| on_calc.emit(())}
            >
                { "Calculate..." }
            </span>
        }
    }
}

/// Helper to format surface area with units or render the calculate trigger.
fn format_surface(surface: Option<f64>, unit: Option<&str>, on_calc: Callback<()>) -> Html {
    if let Some(area) = surface {
        let unit_display = match unit {
            Some(u) if !u.is_empty() => format!(" {}²", u),
            _ => String::new(),
        };
        html! { format!("{:.4}{}", area, unit_display) }
    } else {
        html! {
            <span
                class="calculate-link"
                onclick={move |_| on_calc.emit(())}
            >
                { "Calculate..." }
            </span>
        }
    }
}

#[function_component(DetailsPanel)]
pub fn details_panel(props: &DetailsPanelProps) -> Html {
    trace_span!("details_panel");
    let unit_suffix = props
        .metadata
        .as_ref()
        .and_then(|m| m.units.map(|u| u.symbol()))
        .filter(|s| !s.is_empty())
        .map(|s| format!(" {s}"))
        .unwrap_or_default();

    html! {
        <div class="panel panel-details">
            <div class="panel-header">
                <div class="panel-header-title">
                    <span class="icon fas fa-circle-info"></span>
                    <span>{ "Details" }</span>
                </div>
            </div>
            <div class="panel-content">
                if let Some(meta) = &props.metadata {
                    <dl class="details-list">
                        { detail_row("File name :", format_or_na(&meta.header.file_name)) }
                        { detail_row("Implementation level :", format_or_na(&meta.header.implementation_level)) }
                        { detail_row("Time stamp :", format_or_na(&meta.header.time_stamp)) }
                        { detail_row("Author(s) :", format_list_or_na(&meta.header.author)) }
                        { detail_row("Organization(s) :", format_list_or_na(&meta.header.organization)) }
                        { detail_row("Preprocessor :", format_or_na(&meta.header.preprocessor_version)) }
                        { detail_row("Originating system :", format_or_na(&meta.header.originating_system)) }
                        { detail_row("Authorization :", format_or_na(&meta.header.authorization)) }
                        { detail_row("Description :", format_or_na(&meta.header.file_description)) }
                        { detail_row("Schema :", format_or_na(&meta.header.file_schema)) }
                        { detail_row("Entity count :", meta.entity_count) }
                        { detail_row("Bounding box :", format_bbox(meta.bounding_box.as_ref(), meta.units.map(|u| u.symbol()))) }
                        { detail_row("Units :", meta.units.map(|u| u.symbol()).unwrap_or(NA)) }
                        { detail_row("Vertices :", meta.vertex_count) }
                        { detail_row("Triangles :", meta.triangle_count) }
                        if let Some(bb) = &meta.bounding_box {
                            { detail_row("Size X :", format!("{:.2}{}", bb.size_x(), unit_suffix)) }
                            { detail_row("Size Y :", format!("{:.2}{}", bb.size_y(), unit_suffix)) }
                            { detail_row("Size Z :", format!("{:.2}{}", bb.size_z(), unit_suffix)) }
                        }
                        { detail_row("Volume :", format_volume(meta.volume, meta.units.map(|u| u.symbol()), props.on_calculate_volume.clone())) }
                        { detail_row("Surface :", format_surface(meta.surface_area, meta.units.map(|u| u.symbol()), props.on_calculate_surface.clone())) }
                    </dl>
                } else {
                    <div class="empty-files-message">{ "No file loaded/Selected yet. Please select or upload a file" }</div>
                }
            </div>
        </div>
    }
}
