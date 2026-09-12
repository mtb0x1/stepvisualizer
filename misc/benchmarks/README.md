# StepVisualizer Benchmark Suite (`benchmarks`)

Deterministic end-to-end hot-path performance and memory benchmark harness for **StepVisualizer**.

This crate measures the critical path from file loading to WebGPU render completion using `examples/l44mji.step` as the baseline. It enforces strict regression protection with **execution time** (Priority 1) and **memory footprint** (Priority 2).

---

## What It Benchmarks

The harness measures 6 distinct phases of the pipeline:

| Phase | Description | Key Modules / Functions |
|---|---|---|
| **1. Ingestion** | Network fetch or local file read into memory buffer | `gloo_net::http::Request` |
| **2. Probe & AST Parse** | Schema verification and full STEP AST tokenization | `probe_validate_step_buffer`, `ruststep::parser::parse` |
| **3. Index & Tables** | Record indexing, style/color mapping, and table extraction | `ExchangeIndex::build`, `all_usable_sections`, `truck_stepio::Table` |
| **4. Mesh Tessellation** | B-Rep boundary evaluation & surface triangulation | `compute_adaptive_tolerance`, `extract_render_parts` |
| **5. GPU Buffer Upload** | Vertex, index, and uniform buffer allocation | `PartGpu::new`, `write_buffer` |
| **6. WebGPU Render & Sync** | Draw dispatch, queue submission, and GPU completion | `render_wgpu_on_canvas`, `queue.on_submitted_work_done()` |

---

## Metrics Captured

- **Phase Timings (ms)**: High-resolution timing (`performance.now()`) for each individual phase and total wall-clock duration.
- **WASM Linear Memory (MB)**: High-water mark of WebAssembly pages (`core::arch::wasm32::memory_size(0) * 65536`).
- **WebGPU Buffer Footprint (MB)**: Total bytes allocated on GPU device (vertex + index + uniform buffers).
- **Mesh Topology Verification**: Verification of part count, vertex count, and triangle count to ensure deterministic tessellation output.

---

## Regression & Weighting Model

### 1. Composite Weighted Score
M = memory(WASM linear memory + WebGPU buffer footprint)

T = time(execution time)

$$\text{Score} = 0.70 \cdot \left( \frac{T_{\text{current}}}{T_{\text{baseline}}} \right) + 0.30 \cdot \left( \frac{M_{\text{current}}}{M_{\text{baseline}}} \right)$$

- $\text{Score} \le 1.00$: **Improved or Identical** (PASS)
- $1.00 < \text{Score} \le 1.03$: **Acceptable Noise Margin** (PASS)
- $1.03 < \text{Score} \le 1.05$: **Warning Threshold** (WARN: +3% to +5%)
- $\text{Score} > 1.05$: **Hard Regression Failure** (FAIL: > +5%)

### 2. SLA Boundary Gates
Regardless of the composite score, the benchmark will fail if any individual gate is violated:
- **Time Regression Gate**: $+5.0\%$ max increase.
- **WASM Peak Memory Gate**: $+8.0\%$ max increase.
- **WebGPU Buffer Footprint Gate**: $+2.0\%$ max increase (mesh geometry is deterministic).

---

## Quickstart

### Prerequisites
- Node.js (v18+)
- Trunk (`cargo install trunk`)
- `wasm-opt` (from `binaryen`)
- Chromium with WebGPU support (`/usr/bin/chromium` or `google-chrome`)

### 1. Build the Benchmark WebAssembly Bundle
```bash
cd misc/benchmarks
trunk build --release
```

### 2. Run the Benchmark CLI Runner
```bash
node runner.mjs
```

### 3. Update the Baseline
To record the current performance numbers as the authoritative baseline:
```bash
node runner.mjs --update-baseline
```

---

## CLI Options

| Flag | Default | Description |
|---|---|---|
| `--file <path>` | `examples/l44mji.step` | Target STEP file relative to repository root |
| `--runs <N>` | `5` | Number of measured iterations to run |
| `--warmup <N>` | `1` | Number of unmeasured warm-up iterations |
| `--update-baseline` | `false` | Save the current run as `baseline.json` |
| `--port <port>` | `8099` | Local HTTP port for serving test assets |
| `--chromium-path <path>`| `/usr/bin/chromium` | Absolute path to Chromium binary |
| `--timeout <sec>` | `180` | Maximum execution timeout in seconds |

---

## Artifacts

- [`baseline.json`](./baseline.json): Authoritative committed benchmark baseline for `examples/l44mji.step`.
- `last_run.json`: Output from the most recent benchmark run (ignored in git).
