use futures_channel::oneshot;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{HtmlCanvasElement, HtmlElement, window};

use stepvisualizer::common::color::StepColorMap;
use stepvisualizer::common::constants::{MAX_TOLERANCE, MIN_TOLERANCE, compute_adaptive_tolerance};
use stepvisualizer::common::exchange_index::ExchangeIndex;
use stepvisualizer::common::fps_meter::FpsMeter;
use stepvisualizer::common::parser::{
    all_usable_sections, compute_bounding_box, extract_header_and_count, probe_validate_step_buffer,
};
use stepvisualizer::common::render::{GpuVertex, extract_render_parts};
use stepvisualizer::common::step_names::StepNameMap;
use stepvisualizer::rendering::camera::CameraState;
use stepvisualizer::rendering::renderer::render_wgpu_on_canvas;
use stepvisualizer::rendering::wgpu_state::{WgpuState, init_wgpu};

fn now_ms() -> f64 {
    window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

fn get_wasm_memory_pages() -> u32 {
    #[cfg(target_arch = "wasm32")]
    {
        core::arch::wasm32::memory_size(0) as u32
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PhaseTimings {
    pub ingestion_ms: f64,
    pub parse_ast_ms: f64,
    pub index_tables_ms: f64,
    pub tessellation_ms: f64,
    pub gpu_upload_ms: f64,
    pub gpu_render_sync_ms: f64,
    pub total_hotpath_ms: f64,
    pub total_wallclock_ms: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MemoryMetrics {
    pub wasm_linear_memory_bytes: u64,
    pub wasm_linear_memory_pages: u32,
    pub gpu_vertex_bytes: u64,
    pub gpu_index_bytes: u64,
    pub gpu_uniform_bytes: u64,
    pub total_gpu_buffer_bytes: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct GeometryMetrics {
    pub part_count: usize,
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub skipped_shells: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IterationResult {
    pub run_index: usize,
    pub timings: PhaseTimings,
    pub memory: MemoryMetrics,
    pub geometry: GeometryMetrics,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TimingSummary {
    pub mean: f64,
    pub median: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PhaseTimingSummaries {
    pub ingestion: TimingSummary,
    pub parse_ast: TimingSummary,
    pub index_tables: TimingSummary,
    pub tessellation: TimingSummary,
    pub gpu_upload: TimingSummary,
    pub gpu_render_sync: TimingSummary,
    pub total_hotpath: TimingSummary,
    pub total_wallclock: TimingSummary,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BenchmarkReport {
    pub file_name: String,
    pub file_size_bytes: usize,
    pub runs: usize,
    pub iterations: Vec<IterationResult>,
    pub summary: PhaseTimingSummaries,
    pub memory: MemoryMetrics,
    pub geometry: GeometryMetrics,
    pub status: String,
}

fn compute_stats(values: &[f64]) -> TimingSummary {
    if values.is_empty() {
        return TimingSummary::default();
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let count = sorted.len() as f64;
    let sum: f64 = sorted.iter().sum();
    let mean = sum / count;

    let variance: f64 = sorted.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count;
    let std_dev = variance.sqrt();

    let median = if sorted.len() % 2 == 1 {
        sorted[sorted.len() / 2]
    } else {
        let mid = sorted.len() / 2;
        (sorted[mid - 1] + sorted[mid]) / 2.0
    };

    TimingSummary {
        mean,
        median,
        std_dev,
        min: sorted.first().copied().unwrap_or(0.0),
        max: sorted.last().copied().unwrap_or(0.0),
    }
}

async fn sync_gpu_queue(state: &WgpuState) -> Result<(), String> {
    let (tx, rx) = oneshot::channel::<()>();
    state.queue.on_submitted_work_done(move || {
        let _ = tx.send(());
    });
    rx.await
        .map_err(|e| format!("GPU queue synchronization error: {e}"))
}

fn update_dom(status_text: &str, output_text: Option<&str>) {
    let win = match window() {
        Some(w) => w,
        None => return,
    };
    let doc = match win.document() {
        Some(d) => d,
        None => return,
    };
    if let Some(status_el) = doc
        .get_element_by_id("status")
        .and_then(|e| e.dyn_into::<HtmlElement>().ok())
    {
        status_el.set_inner_text(status_text);
    }
    if let Some(output_str) = output_text {
        if let Some(output_el) = doc
            .get_element_by_id("output")
            .and_then(|e| e.dyn_into::<HtmlElement>().ok())
        {
            output_el.set_inner_text(output_str);
        }
    }
}

fn set_window_benchmark_result(report: &BenchmarkReport) {
    if let Some(win) = window() {
        if let Ok(json_str) = serde_json::to_string(report) {
            let js_val = js_sys::JSON::parse(&json_str).unwrap_or(JsValue::NULL);
            let _ = js_sys::Reflect::set(
                &win,
                &JsValue::from_str("__BENCHMARK_RESULT__"),
                &js_val,
            );
            let _ = js_sys::Reflect::set(
                &win,
                &JsValue::from_str("__BENCHMARK_STATUS__"),
                &JsValue::from_str("DONE"),
            );
        }
    }
}

fn set_window_benchmark_error(err_msg: &str) {
    if let Some(win) = window() {
        let _ = js_sys::Reflect::set(
            &win,
            &JsValue::from_str("__BENCHMARK_ERROR__"),
            &JsValue::from_str(err_msg),
        );
        let _ = js_sys::Reflect::set(
            &win,
            &JsValue::from_str("__BENCHMARK_STATUS__"),
            &JsValue::from_str("ERROR"),
        );
    }
    web_sys::console::error_1(&JsValue::from_str(err_msg));
}

fn get_url_param(param: &str) -> Option<String> {
    let search = window()?.location().search().ok()?;
    let query = search.trim_start_matches('?');
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k.eq_ignore_ascii_case(param) {
                return Some(v.to_string());
            }
        } else if pair.eq_ignore_ascii_case(param) {
            return Some("true".to_string());
        }
    }
    None
}

#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    // Read query parameters: ?file=...&runs=...&warmup=...
    let file_url = get_url_param("file").unwrap_or_else(|| "examples/l44mji.step".to_string());
    let runs_count: usize = get_url_param("runs")
        .and_then(|r| r.parse().ok())
        .unwrap_or(5);
    let warmup_count: usize = get_url_param("warmup")
        .and_then(|w| w.parse().ok())
        .unwrap_or(1);

    update_dom(
        &format!("Fetching STEP file '{file_url}'..."),
        Some("Preparing benchmark run..."),
    );

    wasm_bindgen_futures::spawn_local(async move {
        if let Err(err) = execute_benchmark(&file_url, runs_count, warmup_count).await {
            let msg = format!("Benchmark execution failed: {err}");
            update_dom(&msg, Some(&msg));
            set_window_benchmark_error(&msg);
        }
    });

    Ok(())
}

async fn execute_benchmark(
    file_url: &str,
    runs_count: usize,
    warmup_count: usize,
) -> Result<(), String> {
    let win = window().ok_or("No global window found")?;
    let doc = win.document().ok_or("No document found")?;
    let canvas = doc
        .get_element_by_id("benchmark-canvas")
        .ok_or("Canvas #benchmark-canvas not found")?
        .dyn_into::<HtmlCanvasElement>()
        .map_err(|_| "Element is not an HtmlCanvasElement")?;

    // Phase 1: Ingestion / Fetch
    let t_ingest_start = now_ms();
    let response = gloo_net::http::Request::get(file_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch file '{file_url}': {e}"))?;

    let file_text = response
        .text()
        .await
        .map_err(|e| format!("Failed to decode file text: {e}"))?;
    let ingestion_duration_ms = now_ms() - t_ingest_start;
    let file_size_bytes = file_text.len();

    update_dom(
        &format!(
            "Loaded file ({:.2} MB). Initializing WebGPU...",
            file_size_bytes as f64 / (1024.0 * 1024.0)
        ),
        None,
    );

    let wgpu_state = Rc::new(
        init_wgpu(canvas)
            .await
            .map_err(|e| format!("WebGPU init failed: {e}"))?,
    );

    let total_executions = warmup_count + runs_count;
    let mut measured_iterations = Vec::with_capacity(runs_count);

    let camera = CameraState::DEFAULT;
    let fps_meter = Rc::new(FpsMeter::new());

    for iter in 0..total_executions {
        let is_warmup = iter < warmup_count;
        let iter_label = if is_warmup {
            format!("Warm-up iteration {}/{}", iter + 1, warmup_count)
        } else {
            format!(
                "Measured run {}/{}",
                iter + 1 - warmup_count,
                runs_count
            )
        };

        update_dom(&format!("Running {iter_label}..."), None);

        let t_run_start = now_ms();

        // Phase 2: Probe & AST Parse
        let t_parse_start = now_ms();
        probe_validate_step_buffer(&file_text)
            .map_err(|e| format!("probe_validate failed: {e}"))?;
        let mut parsed = stepvisualizer::ruststep::parser::parse(&file_text)
            .map_err(|e| format!("ruststep parser error: {e}"))?;
        let parse_ast_ms = now_ms() - t_parse_start;

        // Phase 3: ExchangeIndex & Entity Tables
        let t_index_start = now_ms();
        let index = ExchangeIndex::build(&mut parsed);
        let color_map = StepColorMap::from_index(&index);
        let name_map = StepNameMap::from_index(&index);
        let _units = index.resolved_unit();
        drop(index);

        let (_header, _count) = extract_header_and_count("benchmark_model.step", &parsed)
            .map_err(|e| format!("Header extraction failed: {e}"))?;

        let sections = all_usable_sections(&parsed)
            .map_err(|e| format!("Section extraction failed: {e}"))?;

        let step_tables: Vec<truck_stepio::r#in::Table> = sections
            .into_iter()
            .map(truck_stepio::r#in::Table::from_data_section)
            .collect();
        drop(parsed);

        let bbox = compute_bounding_box(&step_tables);
        let index_tables_ms = now_ms() - t_index_start;

        // Phase 4: Mesh Tessellation
        let t_tess_start = now_ms();
        let base_tolerance = compute_adaptive_tolerance(bbox.as_ref());
        let tolerance = base_tolerance.clamp(MIN_TOLERANCE, MAX_TOLERANCE);

        let tess_output = extract_render_parts(
            &step_tables,
            Some(&color_map),
            Some(&name_map),
            tolerance,
        );
        let parts = tess_output.parts;
        let skipped_shells = tess_output.skipped_shells;
        let tessellation_ms = now_ms() - t_tess_start;

        // Phase 5: GPU Buffer Allocation, Uniform Upload & Draw Command Dispatch
        let t_dispatch_start = now_ms();
        let visibility = vec![true; parts.len()];

        render_wgpu_on_canvas(
            wgpu_state.clone(),
            &parts,
            &visibility,
            &camera,
            fps_meter.clone(),
        )
        .await
        .map_err(|e| format!("Render failed: {e}"))?;

        let gpu_upload_ms = now_ms() - t_dispatch_start;

        // Phase 6: WebGPU Hardware Execution & Sync (queue.on_submitted_work_done)
        let t_sync_start = now_ms();
        sync_gpu_queue(&wgpu_state).await?;
        let gpu_render_sync_ms = now_ms() - t_sync_start;

        let total_hotpath_ms = parse_ast_ms
            + index_tables_ms
            + tessellation_ms
            + gpu_upload_ms
            + gpu_render_sync_ms;

        let total_wallclock_ms = (now_ms() - t_run_start) + ingestion_duration_ms;

        // Collect Geometry & Memory Metrics
        let part_count = parts.len();
        let vertex_count: usize = parts.iter().map(|p| p.vertex_count()).sum();
        let triangle_count: usize = parts.iter().map(|p| p.triangle_count()).sum();

        let gpu_vertex_bytes: u64 = parts
            .iter()
            .map(|p| (p.vertices.len() * std::mem::size_of::<GpuVertex>()) as u64)
            .sum();

        let gpu_index_bytes: u64 = parts
            .iter()
            .map(|p| (p.indices.len() * std::mem::size_of::<u32>()) as u64)
            .sum();

        // Uniforms: view_proj (64) + each part (model 64 + color 16 = 80)
        let gpu_uniform_bytes: u64 = 64 + (parts.len() as u64 * 80);
        let total_gpu_buffer_bytes = gpu_vertex_bytes + gpu_index_bytes + gpu_uniform_bytes;

        let wasm_pages = get_wasm_memory_pages();
        let wasm_bytes = wasm_pages as u64 * 65536;

        let iter_result = IterationResult {
            run_index: iter + 1,
            timings: PhaseTimings {
                ingestion_ms: ingestion_duration_ms,
                parse_ast_ms,
                index_tables_ms,
                tessellation_ms,
                gpu_upload_ms,
                gpu_render_sync_ms,
                total_hotpath_ms,
                total_wallclock_ms,
            },
            memory: MemoryMetrics {
                wasm_linear_memory_bytes: wasm_bytes,
                wasm_linear_memory_pages: wasm_pages,
                gpu_vertex_bytes,
                gpu_index_bytes,
                gpu_uniform_bytes,
                total_gpu_buffer_bytes,
            },
            geometry: GeometryMetrics {
                part_count,
                vertex_count,
                triangle_count,
                skipped_shells,
            },
        };

        if !is_warmup {
            measured_iterations.push(iter_result);
        }
    }

    // Compute statistical aggregates across measured iterations
    let parse_ast_vals: Vec<f64> = measured_iterations
        .iter()
        .map(|i| i.timings.parse_ast_ms)
        .collect();
    let index_tables_vals: Vec<f64> = measured_iterations
        .iter()
        .map(|i| i.timings.index_tables_ms)
        .collect();
    let tess_vals: Vec<f64> = measured_iterations
        .iter()
        .map(|i| i.timings.tessellation_ms)
        .collect();
    let upload_vals: Vec<f64> = measured_iterations
        .iter()
        .map(|i| i.timings.gpu_upload_ms)
        .collect();
    let sync_vals: Vec<f64> = measured_iterations
        .iter()
        .map(|i| i.timings.gpu_render_sync_ms)
        .collect();
    let hotpath_vals: Vec<f64> = measured_iterations
        .iter()
        .map(|i| i.timings.total_hotpath_ms)
        .collect();
    let wallclock_vals: Vec<f64> = measured_iterations
        .iter()
        .map(|i| i.timings.total_wallclock_ms)
        .collect();

    let summary = PhaseTimingSummaries {
        ingestion: compute_stats(&[ingestion_duration_ms]),
        parse_ast: compute_stats(&parse_ast_vals),
        index_tables: compute_stats(&index_tables_vals),
        tessellation: compute_stats(&tess_vals),
        gpu_upload: compute_stats(&upload_vals),
        gpu_render_sync: compute_stats(&sync_vals),
        total_hotpath: compute_stats(&hotpath_vals),
        total_wallclock: compute_stats(&wallclock_vals),
    };

    let latest_mem = measured_iterations
        .last()
        .map(|i| i.memory.clone())
        .unwrap_or_default();
    let latest_geom = measured_iterations
        .last()
        .map(|i| i.geometry.clone())
        .unwrap_or_default();

    let report = BenchmarkReport {
        file_name: file_url.to_string(),
        file_size_bytes,
        runs: runs_count,
        iterations: measured_iterations,
        summary,
        memory: latest_mem,
        geometry: latest_geom,
        status: "SUCCESS".to_string(),
    };

    let json_output = serde_json::to_string(&report)
        .map_err(|e| format!("Failed to serialize report: {e}"))?;

    // Output with distinct delimiters for automated scraping on a single console line
    web_sys::console::log_1(&JsValue::from_str(&format!(
        "=== [BENCHMARK_OUTPUT_START] ==={}=== [BENCHMARK_OUTPUT_END] ===",
        json_output
    )));

    let display_summary = format!(
        "Benchmark Completed ({} runs)\n\
         File: {} ({:.2} MB)\n\
         Geometry: {} parts, {} vertices, {} triangles\n\
         --------------------------------------------------\n\
         Parse AST:           {:>8.2} ms (std: {:.2})\n\
         Index & Tables:      {:>8.2} ms (std: {:.2})\n\
         Tessellation:        {:>8.2} ms (std: {:.2})\n\
         GPU Buffer Upload:   {:>8.2} ms (std: {:.2})\n\
         GPU Render & Sync:   {:>8.2} ms (std: {:.2})\n\
         --------------------------------------------------\n\
         Total Hot-Path:      {:>8.2} ms (std: {:.2})\n\
         --------------------------------------------------\n\
         WASM Linear Memory:  {:>8.2} MB ({} pages)\n\
         WebGPU Buffer Memory:{:>8.2} MB",
        report.runs,
        report.file_name,
        report.file_size_bytes as f64 / (1024.0 * 1024.0),
        report.geometry.part_count,
        report.geometry.vertex_count,
        report.geometry.triangle_count,
        report.summary.parse_ast.mean,
        report.summary.parse_ast.std_dev,
        report.summary.index_tables.mean,
        report.summary.index_tables.std_dev,
        report.summary.tessellation.mean,
        report.summary.tessellation.std_dev,
        report.summary.gpu_upload.mean,
        report.summary.gpu_upload.std_dev,
        report.summary.gpu_render_sync.mean,
        report.summary.gpu_render_sync.std_dev,
        report.summary.total_hotpath.mean,
        report.summary.total_hotpath.std_dev,
        report.memory.wasm_linear_memory_bytes as f64 / (1024.0 * 1024.0),
        report.memory.wasm_linear_memory_pages,
        report.memory.total_gpu_buffer_bytes as f64 / (1024.0 * 1024.0),
    );

    update_dom("Benchmark Completed Successfully!", Some(&display_summary));
    set_window_benchmark_result(&report);

    Ok(())
}
