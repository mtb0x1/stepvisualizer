//! Minimal Chrome-style FPS overlay: a current-FPS readout plus a small
//! sparkline of recent samples, pinned to the bottom-left of the canvas.
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use yew::prelude::*;

use crate::common::fps_meter::FpsMeter;

#[derive(Properties, Clone)]
pub struct FpsGraphProps {
    pub meter: Rc<FpsMeter>,
}

impl PartialEq for FpsGraphProps {
    fn eq(&self, other: &Self) -> bool {
        // The meter is a long-lived singleton; identity equality is enough.
        Rc::ptr_eq(&self.meter, &other.meter)
    }
}

/// Sparkline dimensions in CSS pixels.
const GRAPH_W: u32 = 120;
const GRAPH_H: u32 = 32;
/// FPS value mapped to the top of the graph; higher clamps to the top.
const MAX_FPS: f32 = 120.0;
/// How often the overlay polls the meter (ms). Cheap: just reads two values.
const POLL_INTERVAL_MS: i32 = 100;

#[function_component(FpsGraph)]
pub fn fps_graph(props: &FpsGraphProps) -> Html {
    let fps = use_state(|| 0.0f32);
    let samples = use_state(Vec::<f32>::new);

    {
        let meter = props.meter.clone();
        let fps = fps.clone();
        let samples = samples.clone();
        use_effect_with((), move |_| {
            let callback = Closure::wrap(Box::new(move || {
                fps.set(meter.current_fps());
                samples.set(meter.samples());
            }) as Box<dyn Fn()>);

            let window = web_sys::window().expect("window must exist");
            let handle = window
                .set_interval_with_callback_and_timeout_and_arguments_0(
                    callback.as_ref().unchecked_ref(),
                    POLL_INTERVAL_MS,
                )
                .expect("failed to schedule FPS poll");

            // Keep the closure alive until the effect is torn down.
            Box::new(move || {
                window.clear_interval_with_handle(handle);
                let _ = &callback;
            }) as Box<dyn Fn()>
        });
    }

    let points = build_points(&samples);
    let stroke = fps_color(*fps);

    html! {
        <div class="fps-graph">
            <div class="fps-graph-label">{ format!("{:.0} FPS", *fps) }</div>
            <svg
                class="fps-graph-svg"
                width={GRAPH_W.to_string()}
                height={GRAPH_H.to_string()}
                viewBox={format!("0 0 {GRAPH_W} {GRAPH_H}")}
            >
                <polyline points={points} fill="none" stroke={stroke} stroke-width="1.5" />
            </svg>
        </div>
    }
}

/// Map samples (oldest first) to SVG `x,y x,y ...` polyline points. The newest
/// sample sits at the right edge; values are clamped to [`MAX_FPS`].
fn build_points(samples: &[f32]) -> String {
    if samples.is_empty() {
        return String::new();
    }
    let n = samples.len();
    let w = GRAPH_W as f32;
    let h = GRAPH_H as f32;
    samples
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let x = if n == 1 { 0.0 } else { (i as f32 / (n - 1) as f32) * w };
            let y = h - (v.min(MAX_FPS) / MAX_FPS) * h;
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Green when smooth, yellow when sluggish, red when effectively stalled.
fn fps_color(fps: f32) -> &'static str {
    if fps >= 50.0 {
        "#4ade80"
    } else if fps >= 30.0 {
        "#facc15"
    } else {
        "#f87171"
    }
}
