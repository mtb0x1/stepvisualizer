//! Full-app fallback shown when the browser cannot provide WebGPU.
//!
//! Every feature of the app funnels into the GPU renderer, so instead of
//! leaving a half-broken shell running (uploads that can never be displayed,
//! camera controls over a dead canvas), the whole UI is replaced by this
//! explanatory page.

use crate::apptracing::{AppTracer, AppTracerTrait};
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct WebGpuUnavailableProps {
    /// Human-readable explanation of why WebGPU is unusable.
    pub reason: AttrValue,
}

#[function_component(WebGpuUnavailable)]
pub fn webgpu_unavailable(props: &WebGpuUnavailableProps) -> Html {
    // The fatal reason is rare by construction; log it once so a console
    // dump (or `?tracing=on`) captures the root cause alongside the UI.
    {
        let reason = props.reason.clone();
        use_effect_with((), move |_| {
            AppTracer::error(&format!("WebGPU unavailable: {reason}"));
            || ()
        });
    }

    html! {
        <div class="webgpu-unavailable">
            <div class="webgpu-unavailable-card">
                <i class="fa-solid fa-triangle-exclamation webgpu-unavailable-icon"></i>
                <h1>{ "WebGPU is not available" }</h1>
                <p class="webgpu-unavailable-reason">{ props.reason.clone() }</p>
                <p>
                    { "StepViz renders 3D models entirely in your browser through WebGPU, \
                        so there is nothing it can do without it." }
                </p>
                <p>
                    { "Try a recent version of Chrome, Edge, or Safari, and make sure \
                        hardware acceleration is enabled in your browser settings." }
                </p>
            </div>
        </div>
    }
}
