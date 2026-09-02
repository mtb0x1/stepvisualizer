//! Minimal Chrome-style FPS overlay: a current-FPS readout plus a small
//! sparkline of recent samples, pinned to the bottom-left of the canvas.
use std::fmt::Write;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use yew::prelude::*;

use crate::common::fps_meter::{FpsMeter, FpsSnapshot};

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
const GRAPH_W_STR: &str = "120";
const GRAPH_H_STR: &str = "32";
const VIEWBOX_STR: &str = "0 0 120 32";
/// FPS value mapped to the top of the graph; higher clamps to the top.
const MAX_FPS: f32 = 120.0;
/// How often the overlay polls the meter (ms). Cheap: just reads two values.
const POLL_INTERVAL_MS: i32 = 100;

#[function_component(FpsGraph)]
pub fn fps_graph(props: &FpsGraphProps) -> Html {
    let snapshot = use_state(FpsSnapshot::default);

    {
        let meter = props.meter.clone();
        let snapshot = snapshot.clone();
        use_effect_with((), move |_| {
            // The Closure::wrap here captures `callback` by move into the teardown Box.
            // If window is None (non-browser test environment), handle is None and the
            // interval is never started, but `callback` is still kept alive in the teardown
            // closure (intentional — harmless but non-obvious). The teardown fires on
            // component unmount via use_effect's cleanup return.
            let callback = Closure::wrap(Box::new(move || {
                snapshot.set(meter.snapshot());
            }) as Box<dyn Fn()>);

            let window = web_sys::window();
            let handle = window.as_ref().and_then(|w| {
                w.set_interval_with_callback_and_timeout_and_arguments_0(
                    callback.as_ref().unchecked_ref(),
                    POLL_INTERVAL_MS,
                )
                .ok()
            });

            // Keep the closure alive until the effect is torn down.
            Box::new(move || {
                if let (Some(w), Some(h)) = (window.as_ref(), handle) {
                    w.clear_interval_with_handle(h);
                }
                let _ = &callback;
            }) as Box<dyn Fn()>
        });
    }

    let points = build_points(&snapshot.samples);
    let stroke = fps_color(snapshot.current_fps);

    html! {
        <div class="fps-graph">
            <div class="fps-graph-label">{ snapshot.current_fps.round() as i32 }{ " FPS" }</div>
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

/// Map samples (oldest first) to SVG `x,y x,y ...` polyline points. The newest
/// sample sits at the right edge; values are clamped to [`MAX_FPS`].
fn build_points(samples: &[f32]) -> String {
    if samples.is_empty() {
        return String::new();
    }
    let n = samples.len();
    let w = GRAPH_W as f32;
    let h = GRAPH_H as f32;
    let mut out = String::with_capacity(n * 12);
    for (i, &v) in samples.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let x = if n == 1 {
            0.0
        } else {
            (i as f32 / (n - 1) as f32) * w
        };
        let y = h - (v.min(MAX_FPS) / MAX_FPS) * h;
        let _ = write!(out, "{x:.1},{y:.1}");
    }
    out
}

/// Green when smooth, yellow when sluggish, red when effectively stalled.
const fn fps_color(fps: f32) -> &'static str {
    if fps >= 50.0 {
        "#4ade80"
    } else if fps >= 30.0 {
        "#facc15"
    } else {
        "#f87171"
    }
}
