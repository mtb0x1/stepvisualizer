//! Minimal Chrome-style FPS overlay: a current-FPS readout plus a small
//! sparkline of recent samples, pinned to the bottom-left of the canvas.
//!
//! This is a **pure display component** — it owns no timer and holds no
//! internal state. The parent (`main_panel`) pushes a fresh [`FpsSnapshot`]
//! immediately after each GPU render batch completes, so the overlay updates
//! exactly when frames are produced and is completely idle the rest of the time.
use yew::prelude::*;

use crate::common::{
    fps_meter::FpsSnapshot,
    utils::{build_svg_polyline_points, fps_color},
};

#[derive(Properties, Clone, PartialEq)]
pub struct FpsGraphProps {
    pub snapshot: FpsSnapshot,
}

/// Sparkline dimensions in CSS pixels.
const GRAPH_W: u32 = 120;
const GRAPH_H: u32 = 32;
const GRAPH_W_STR: &str = "120";
const GRAPH_H_STR: &str = "32";
const VIEWBOX_STR: &str = "0 0 120 32";
/// FPS value mapped to the top of the graph; higher clamps to the top.
const MAX_FPS: f32 = 120.0;

#[function_component(FpsGraph)]
pub fn fps_graph(props: &FpsGraphProps) -> Html {
    let points =
        build_svg_polyline_points(&props.snapshot.samples, GRAPH_W as f32, GRAPH_H as f32, MAX_FPS);
    let stroke = fps_color(props.snapshot.current_fps);

    html! {
        <div class="fps-graph">
            <div class="fps-graph-label">{ props.snapshot.current_fps.round() as i32 }{ " FPS" }</div>
            <svg
                class="fps-graph-svg"
                width={GRAPH_W_STR}
                height={GRAPH_H_STR}
                viewBox={VIEWBOX_STR}
            >
                <polyline points={points} fill="none" stroke={stroke} stroke-width="1.5" />
            </svg>
        </div>
    }
}
