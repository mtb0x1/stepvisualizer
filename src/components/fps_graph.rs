//! Minimal Chrome-style FPS overlay: a current-FPS readout plus a small
//! sparkline of recent samples, pinned to the bottom-left of the canvas.
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use yew::prelude::*;

use crate::common::fps_meter::{FpsMeter, FpsSnapshot};
use crate::common::utils::{build_svg_polyline_points, fps_color};

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

    let points = build_svg_polyline_points(
        &snapshot.samples,
        GRAPH_W as f32,
        GRAPH_H as f32,
        MAX_FPS,
    );
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
