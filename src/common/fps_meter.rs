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

use crate::common::time::now_ms;

/// Sparkline history length (samples). ~10s at the 100ms snapshot cadence.
const MAX_SAMPLES: usize = 60;
/// Window used to estimate instantaneous FPS from frame timestamps.
const WINDOW_MS: f64 = 500.0;
/// Cadence at which a snapshot is pushed to the sparkline history.
const SAMPLE_INTERVAL_MS: f64 = 100.0;
/// After this long without a frame, the meter reports 0 FPS (idle).
const IDLE_TIMEOUT_MS: f64 = 800.0;

/// Point-in-time snapshot of the FPS meter state for the UI overlay.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct FpsSnapshot {
    pub current_fps: f32,
    pub samples: Vec<f32>,
}

pub struct FpsMeter {
    inner: RefCell<FpsInner>,
}

struct FpsInner {
    /// Timestamps (performance.now ms) of frames within the sliding window.
    /// Append-only and older-first, so aging out is an O(1) `pop_front`.
    frame_times: VecDeque<f64>,
    /// Recent FPS snapshots for the sparkline, oldest first.
    samples: VecDeque<f32>,
    /// performance.now ms of the last snapshot push.
    last_sample_at: f64,
}

#[allow(dead_code)]
impl FpsMeter {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(FpsInner {
                frame_times: VecDeque::with_capacity(64),
                samples: VecDeque::with_capacity(MAX_SAMPLES),
                last_sample_at: 0.0,
            }),
        }
    }

    /// Call exactly once per rendered frame to feed the meter.
    pub fn record_frame(&self) {
        let now = now_ms();
        let mut inner = self.inner.borrow_mut();
        inner.frame_times.push_back(now);

        // Drop timestamps that have aged out of the sliding window. `frame_times`
        // is kept oldest-first, so eviction is an O(1) `pop_front` (not the O(n)
        // `Vec::remove(0)` this replaced).
        let cutoff = now - WINDOW_MS;
        while inner
            .frame_times
            .front()
            .is_some_and(|&oldest| oldest < cutoff)
        {
            inner.frame_times.pop_front();
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
        match inner.frame_times.back() {
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

    /// Atomic snapshot of current FPS and recent history samples in a single borrow.
    pub fn snapshot(&self) -> FpsSnapshot {
        let inner = self.inner.borrow();
        let now = now_ms();
        let current_fps = match inner.frame_times.back() {
            Some(&newest) if now - newest <= IDLE_TIMEOUT_MS => {
                Self::fps_from_times(&inner.frame_times, now)
            }
            _ => 0.0,
        };
        let samples = inner.samples.iter().copied().collect();
        FpsSnapshot {
            current_fps,
            samples,
        }
    }

    /// FPS from a deque of frame timestamps: frames-per-second across the span
    /// from the first (oldest) to `now`. Needs at least two frames to be meaningful.
    fn fps_from_times(times: &VecDeque<f64>, now: f64) -> f32 {
        if times.len() < 2 {
            return 0.0;
        }
        let span = now - *times.front().unwrap();
        if span <= 0.0 {
            return 0.0;
        }
        ((times.len() - 1) as f64 / (span / 1000.0)) as f32
    }
}
