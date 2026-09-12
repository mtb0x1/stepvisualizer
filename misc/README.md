# Miscellaneous Tools & Experiments (`misc/`)

This directory contains standalone tools, developer benchmarks, profilers, and auxiliary utilities that support the development and maintenance of **StepVisualizer** without impacting the production application or CI builds.

## Directory Contents

| Directory | Description | Type |
|---|---|---|
| [`benchmarks/`](./benchmarks) | End-to-end performance & memory benchmark suite measuring the hot path from STEP loading to WebGPU render completion. | Standalone Crate |

---

## Guidelines for Adding Tools to `misc/`

1. **Isolation**: Tools in this directory should be decoupled from the core application crate (`stepvisualizer`). They consume `stepvisualizer` via path dependencies (`path = "../.."`) rather than forcing workspace coupling.
2. **Self-Contained Documentation**: Every sub-tool or subcrate must include its own `README.md` detailing prerequisites, execution commands, and operational parameters.
3. **Zero Production Overhead**: Assets and dependencies in `misc/` must never be bundled into the production GitHub Pages deployment.
