//! Lightweight FPS meter tuned for on-demand rendering.
//!
//! The viewer does not run a continuous animation loop; it re-renders only
//! when inputs change (orbit drag, zoom, part visibility, model load). So the
//! meter never assumes a steady stream of frames. [`FpsMeter::record_frame`]
//! is called once per *actual* rendered frame; the instantaneous rate is
//! estimated from a short rolling window of frame timestamps, and periodic
//! snapshots feed the sparkline. When no frame has arrived for a while the
//! meter reports 0 FPS (idle) instead of a stale value.
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use crate::common::time::now_ms;

/// Sparkline history length (samples). ~10s at the 100ms snapshot cadence.
const MAX_SAMPLES: usize = 60;
/// Window used to estimate instantaneous FPS from frame timestamps.
const WINDOW_MS: f64 = 500.0;
/// Cadence at which a snapshot is pushed to the sparkline history.
const SAMPLE_INTERVAL_MS: f64 = 100.0;
/// After this long without a frame, the meter reports 0 FPS (idle).
const IDLE_TIMEOUT_MS: f64 = 800.0;

pub struct FpsMeter {
    inner: RefCell<FpsInner>,
}

struct FpsInner {
    /// Timestamps (performance.now ms) of frames within the sliding window.
    frame_times: Vec<f64>,
    /// Recent FPS snapshots for the sparkline, oldest first.
    samples: VecDeque<f32>,
    /// performance.now ms of the last snapshot push.
    last_sample_at: f64,
}

impl FpsMeter {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            inner: RefCell::new(FpsInner {
                frame_times: Vec::with_capacity(64),
                samples: VecDeque::with_capacity(MAX_SAMPLES),
                last_sample_at: 0.0,
            }),
        })
    }

    /// Call exactly once per rendered frame to feed the meter.
    pub fn record_frame(&self) {
        let now = now_ms();
        let mut inner = self.inner.borrow_mut();
        inner.frame_times.push(now);

        // Drop timestamps that have aged out of the sliding window.
        let cutoff = now - WINDOW_MS;
        while inner
            .frame_times
            .first()
            .is_some_and(|&oldest| oldest < cutoff)
        {
            inner.frame_times.remove(0);
        }

        // Snapshot the rate at most every SAMPLE_INTERVAL_MS so the graph
        // advances on a steady cadence even during short interaction bursts.
        if inner.last_sample_at == 0.0 || now - inner.last_sample_at >= SAMPLE_INTERVAL_MS {
            let fps = Self::fps_from_times(&inner.frame_times, now);
            inner.samples.push_back(fps);
            if inner.samples.len() > MAX_SAMPLES {
                inner.samples.pop_front();
            }
            inner.last_sample_at = now;
        }
    }

    /// Instantaneous FPS derived from frame timestamps, or 0.0 when idle.
    pub fn current_fps(&self) -> f32 {
        let inner = self.inner.borrow();
        let now = now_ms();
        match inner.frame_times.last() {
            Some(&newest) if now - newest <= IDLE_TIMEOUT_MS => {
                Self::fps_from_times(&inner.frame_times, now)
            }
            _ => 0.0,
        }
    }

    /// Recent FPS snapshots (oldest first) for the sparkline.
    pub fn samples(&self) -> Vec<f32> {
        self.inner.borrow().samples.iter().copied().collect()
    }

    /// FPS from a slice of frame timestamps: frames-per-second across the span
    /// from the first to `now`. Needs at least two frames to be meaningful.
    fn fps_from_times(times: &[f64], now: f64) -> f32 {
        if times.len() < 2 {
            return 0.0;
        }
        let span = now - times[0];
        if span <= 0.0 {
            return 0.0;
        }
        ((times.len() - 1) as f64 / (span / 1000.0)) as f32
    }
}
