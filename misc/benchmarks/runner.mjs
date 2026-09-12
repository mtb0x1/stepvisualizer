#!/usr/bin/env node

/**
 * StepVisualizer Hot-Path WebGPU Benchmark CLI Runner
 *
 * Automates headless Chromium execution to measure the entire pipeline:
 * File Ingestion -> AST Parsing -> Table Extraction -> Tessellation ->
 * WebGPU Buffer Creation -> WebGPU Render Pass -> GPU Execution Completion.
 *
 * Compares against baseline.json with 70% Time / 30% Memory weighted scoring.
 */

import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '../..');
const DIST_DIR = path.resolve(__dirname, 'dist');
const EXAMPLES_DIR = path.resolve(REPO_ROOT, 'examples');
const BASELINE_FILE = path.resolve(__dirname, 'baseline.json');
const LAST_RUN_FILE = path.resolve(__dirname, 'last_run.json');

// Color helpers for terminal output
const colors = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  dim: '\x1b[2m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  red: '\x1b[31m',
  cyan: '\x1b[36m',
  magenta: '\x1b[35m',
  blue: '\x1b[34m',
};

// Parse CLI Arguments
const args = process.argv.slice(2);
function getArg(flag, defaultValue) {
  const idx = args.indexOf(flag);
  if (idx !== -1 && idx + 1 < args.length) {
    return args[idx + 1];
  }
  return defaultValue;
}
const hasFlag = (flag) => args.includes(flag);

const runs = parseInt(getArg('--runs', '5'), 10);
const warmup = parseInt(getArg('--warmup', '1'), 10);
const targetFile = getArg('--file', 'examples/l44mji.step');
const port = parseInt(getArg('--port', '8099'), 10);
const updateBaseline = hasFlag('--update-baseline');
const chromiumPath = getArg('--chromium-path', '/usr/bin/chromium');
const timeoutSec = parseInt(getArg('--timeout', '180'), 10);

// SLA Gate Thresholds
const TIME_WEIGHT = 0.70;
const MEM_WEIGHT = 0.30;
const SCORE_WARN_THRESHOLD = 1.03; // +3%
const SCORE_FAIL_THRESHOLD = 1.05; // +5%
const TIME_MAX_REGRESSION_PCT = 5.0; // +5%
const MEM_MAX_REGRESSION_PCT = 8.0;  // +8%
const VRAM_MAX_REGRESSION_PCT = 2.0; // +2%

const MIME_TYPES = {
  '.html': 'text/html',
  '.js': 'application/javascript',
  '.mjs': 'application/javascript',
  '.wasm': 'application/wasm',
  '.css': 'text/css',
  '.json': 'application/json',
  '.step': 'application/octet-stream',
  '.stp': 'application/octet-stream',
};

// 1. Start Local HTTP Server
function startServer() {
  return new Promise((resolve, reject) => {
    const server = http.createServer((req, res) => {
      let reqPath = decodeURI(req.url.split('?')[0]);
      let filePath;

      if (reqPath.startsWith('/examples/')) {
        filePath = path.join(REPO_ROOT, reqPath);
      } else {
        if (reqPath === '/' || reqPath === '') reqPath = '/index.html';
        filePath = path.join(DIST_DIR, reqPath);
      }

      if (!fs.existsSync(filePath)) {
        res.writeHead(404, { 'Content-Type': 'text/plain' });
        res.end(`Not found: ${reqPath}`);
        return;
      }

      const stat = fs.statSync(filePath);
      if (stat.isDirectory()) {
        filePath = path.join(filePath, 'index.html');
      }

      const ext = path.extname(filePath).toLowerCase();
      const contentType = MIME_TYPES[ext] || 'application/octet-stream';

      res.writeHead(200, {
        'Content-Type': contentType,
        'Content-Length': stat.size,
        'Access-Control-Allow-Origin': '*',
        'Cross-Origin-Opener-Policy': 'same-origin',
        'Cross-Origin-Embedder-Policy': 'require-corp',
      });

      fs.createReadStream(filePath).pipe(res);
    });

    server.listen(port, '127.0.0.1', () => {
      resolve(server);
    });

    server.on('error', (err) => reject(err));
  });
}

// 2. Launch Chromium and Capture Benchmark JSON
function runChromium(targetUrl) {
  return new Promise((resolve, reject) => {
    console.log(`${colors.cyan}[1/4] Spawning Chromium headless WebGPU runner...${colors.reset}`);
    const chromeFlags = [
      '--headless=new',
      '--enable-unsafe-webgpu',
      '--use-webgpu-adapter=swiftshader',
      '--enable-features=Vulkan,DefaultANGLEVulkan',
      '--no-sandbox',
      '--disable-gpu-sandbox',
      '--disable-dev-shm-usage',
      '--enable-logging=stderr',
      '--v=1',
      targetUrl,
    ];

    const proc = spawn(chromiumPath, chromeFlags);

    let fullOutput = '';
    let stderrBuffer = '';

    const timer = setTimeout(() => {
      proc.kill('SIGKILL');
      reject(new Error(`Benchmark timed out after ${timeoutSec} seconds.`));
    }, timeoutSec * 1000);

    const onData = (data) => {
      const text = data.toString();
      fullOutput += text;

      const startTag = '=== [BENCHMARK_OUTPUT_START] ===';
      const endTag = '=== [BENCHMARK_OUTPUT_END] ===';

      const startIdx = fullOutput.indexOf(startTag);
      if (startIdx !== -1) {
        const endIdx = fullOutput.indexOf(endTag, startIdx + startTag.length);
        if (endIdx !== -1) {
          const rawJson = fullOutput.slice(startIdx + startTag.length, endIdx).trim();
          clearTimeout(timer);
          proc.kill();
          try {
            const parsed = JSON.parse(rawJson);
            resolve(parsed);
          } catch (err) {
            reject(new Error(`Failed to parse benchmark JSON output: ${err.message}\nRaw JSON snippet:\n${rawJson.slice(0, 300)}`));
          }
        }
      }
    };

    proc.stdout.on('data', onData);
    proc.stderr.on('data', (data) => {
      stderrBuffer += data.toString();
      // In some Chromium builds console.log routes to stderr
      onData(data);
    });

    proc.on('close', (code) => {
      clearTimeout(timer);
      if (!foundStart) {
        reject(
          new Error(
            `Chromium exited (code ${code}) without emitting benchmark results.\nStderr:\n${stderrBuffer.slice(-2000)}`
          )
        );
      }
    });

    proc.on('error', (err) => {
      clearTimeout(timer);
      reject(err);
    });
  });
}

// 3. Format Table & Comparison Engine
function formatDiff(current, baseline, unit = 'ms', invertGood = false) {
  if (baseline === undefined || baseline === null || baseline === 0) {
    return { diffStr: 'N/A', pctStr: 'N/A', status: 'INFO' };
  }
  const diff = current - baseline;
  const pct = (diff / baseline) * 100;
  const sign = diff > 0 ? '+' : '';

  const diffStr = `${sign}${diff.toFixed(2)} ${unit}`;
  const pctStr = `${sign}${pct.toFixed(2)}%`;

  let status = 'OK';
  if (pct > 5.0) status = invertGood ? 'GOOD' : 'FAIL';
  else if (pct > 2.0) status = invertGood ? 'GOOD' : 'WARN';
  else if (pct < -5.0) status = invertGood ? 'FAIL' : 'GOOD';
  else if (pct < -2.0) status = invertGood ? 'WARN' : 'GOOD';

  return { diff, pct, diffStr, pctStr, status };
}

function printComparison(currentReport, baselineReport) {
  const isNewBaseline = !baselineReport;

  console.log(`\n${colors.bold}═════════════════════════════════════════════════════════════════════════════════════${colors.reset}`);
  console.log(` ${colors.cyan}${colors.bold}StepVisualizer WebGPU Hot-Path Benchmark Results${colors.reset}`);
  console.log(` File: ${colors.bold}${currentReport.file_name}${colors.reset} (${(currentReport.file_size_bytes / (1024 * 1024)).toFixed(2)} MB)`);
  console.log(` Iterations: ${currentReport.runs} measured runs`);
  console.log(` Output: ${currentReport.geometry.part_count} parts, ${currentReport.geometry.vertex_count.toLocaleString()} vertices, ${currentReport.geometry.triangle_count.toLocaleString()} triangles`);
  console.log(`${colors.bold}═════════════════════════════════════════════════════════════════════════════════════${colors.reset}\n`);

  const rows = [
    {
      name: 'Phase 1: Ingestion / Fetch',
      curr: currentReport.summary.ingestion.mean,
      base: baselineReport?.summary?.ingestion?.mean,
      unit: 'ms',
    },
    {
      name: 'Phase 2: Probe & AST Parse',
      curr: currentReport.summary.parse_ast.mean,
      base: baselineReport?.summary?.parse_ast?.mean,
      unit: 'ms',
    },
    {
      name: 'Phase 3: Index & Table Extract',
      curr: currentReport.summary.index_tables.mean,
      base: baselineReport?.summary?.index_tables?.mean,
      unit: 'ms',
    },
    {
      name: 'Phase 4: Mesh Tessellation',
      curr: currentReport.summary.tessellation.mean,
      base: baselineReport?.summary?.tessellation?.mean,
      unit: 'ms',
    },
    {
      name: 'Phase 5: Buffer Alloc & Dispatch',
      curr: currentReport.summary.gpu_upload.mean,
      base: baselineReport?.summary?.gpu_upload?.mean,
      unit: 'ms',
    },
    {
      name: 'Phase 6: WebGPU Hardware Sync',
      curr: currentReport.summary.gpu_render_sync.mean,
      base: baselineReport?.summary?.gpu_render_sync?.mean,
      unit: 'ms',
    },
    {
      name: '--------------------------------',
      curr: null,
      base: null,
      unit: '',
    },
    {
      name: 'TOTAL HOT-PATH DURATION',
      curr: currentReport.summary.total_hotpath.mean,
      base: baselineReport?.summary?.total_hotpath?.mean,
      unit: 'ms',
      isPrimaryTime: true,
    },
    {
      name: '--------------------------------',
      curr: null,
      base: null,
      unit: '',
    },
    {
      name: 'WASM Linear Memory (Peak)',
      curr: currentReport.memory.wasm_linear_memory_bytes / (1024 * 1024),
      base: baselineReport ? baselineReport.memory.wasm_linear_memory_bytes / (1024 * 1024) : null,
      unit: 'MB',
      isPrimaryMem: true,
    },
    {
      name: 'WebGPU Buffer Footprint',
      curr: currentReport.memory.total_gpu_buffer_bytes / (1024 * 1024),
      base: baselineReport ? baselineReport.memory.total_gpu_buffer_bytes / (1024 * 1024) : null,
      unit: 'MB',
      isVram: true,
    },
  ];

  console.log(
    ` ${colors.bold}${'Metric'.padEnd(32)} ${'Baseline'.padStart(12)} ${'Current'.padStart(12)} ${'Delta'.padStart(14)} ${'% Change'.padStart(10)} ${'Status'.padStart(8)}${colors.reset}`
  );
  console.log(`${'-'.repeat(93)}`);

  let timeRegressionPct = 0;
  let memRegressionPct = 0;
  let vramRegressionPct = 0;

  for (const row of rows) {
    if (row.name.startsWith('---')) {
      console.log(`${'-'.repeat(93)}`);
      continue;
    }

    const currStr = `${row.curr.toFixed(2)} ${row.unit}`;
    const baseStr = row.base !== null && row.base !== undefined ? `${row.base.toFixed(2)} ${row.unit}` : 'N/A';

    const diff = formatDiff(row.curr, row.base, row.unit);

    let statusColor = colors.reset;
    if (diff.status === 'GOOD') statusColor = colors.green;
    else if (diff.status === 'WARN') statusColor = colors.yellow;
    else if (diff.status === 'FAIL') statusColor = colors.red;

    if (row.isPrimaryTime && diff.pct !== undefined) timeRegressionPct = diff.pct;
    if (row.isPrimaryMem && diff.pct !== undefined) memRegressionPct = diff.pct;
    if (row.isVram && diff.pct !== undefined) vramRegressionPct = diff.pct;

    const boldRow = row.isPrimaryTime ? colors.bold : '';

    console.log(
      ` ${boldRow}${row.name.padEnd(32)}${colors.reset} ${baseStr.padStart(12)} ${currStr.padStart(12)} ${diff.diffStr.padStart(14)} ${statusColor}${diff.pctStr.padStart(10)}${colors.reset} ${statusColor}${diff.status.padStart(8)}${colors.reset}`
    );
  }

  console.log(`${colors.bold}═════════════════════════════════════════════════════════════════════════════════════${colors.reset}`);

  if (isNewBaseline) {
    console.log(`\n${colors.yellow}[INFO] No prior baseline found. This run will be used to establish the baseline.${colors.reset}`);
    return { passed: true, score: 1.0 };
  }

  // Calculate Weighted Score: 70% Time, 30% Memory
  const baseTime = baselineReport.summary.total_hotpath.mean;
  const currTime = currentReport.summary.total_hotpath.mean;
  const baseMem = baselineReport.memory.wasm_linear_memory_bytes;
  const currMem = currentReport.memory.wasm_linear_memory_bytes;

  const timeRatio = currTime / baseTime;
  const memRatio = currMem / baseMem;
  const compositeScore = TIME_WEIGHT * timeRatio + MEM_WEIGHT * memRatio;

  console.log(`\n${colors.bold}Regression Analysis & SLA Gates:${colors.reset}`);
  console.log(` • Time Ratio:        ${(timeRatio).toFixed(4)} (weight: ${TIME_WEIGHT * 100}%)`);
  console.log(` • Memory Ratio:      ${(memRatio).toFixed(4)} (weight: ${MEM_WEIGHT * 100}%)`);
  
  let scoreColor = colors.green;
  if (compositeScore > SCORE_FAIL_THRESHOLD) scoreColor = colors.red;
  else if (compositeScore > SCORE_WARN_THRESHOLD) scoreColor = colors.yellow;

  console.log(` • Composite Score:   ${scoreColor}${colors.bold}${compositeScore.toFixed(4)}${colors.reset} (Threshold: <= ${SCORE_FAIL_THRESHOLD.toFixed(2)})`);

  let passed = true;
  const failureReasons = [];

  if (compositeScore > SCORE_FAIL_THRESHOLD) {
    passed = false;
    failureReasons.push(`Composite score ${compositeScore.toFixed(4)} exceeds failure threshold of ${SCORE_FAIL_THRESHOLD.toFixed(2)} (+${((compositeScore - 1) * 100).toFixed(1)}%)`);
  }

  if (timeRegressionPct > TIME_MAX_REGRESSION_PCT) {
    passed = false;
    failureReasons.push(`Hot-path execution time regressed by +${timeRegressionPct.toFixed(2)}% (SLA gate: <= +${TIME_MAX_REGRESSION_PCT}%)`);
  }

  if (memRegressionPct > MEM_MAX_REGRESSION_PCT) {
    passed = false;
    failureReasons.push(`Peak WASM memory regressed by +${memRegressionPct.toFixed(2)}% (SLA gate: <= +${MEM_MAX_REGRESSION_PCT}%)`);
  }

  if (vramRegressionPct > VRAM_MAX_REGRESSION_PCT) {
    passed = false;
    failureReasons.push(`WebGPU buffer footprint increased by +${vramRegressionPct.toFixed(2)}% (SLA gate: <= +${VRAM_MAX_REGRESSION_PCT}%)`);
  }

  if (!passed) {
    console.log(`\n${colors.red}${colors.bold}[FAIL] Performance regression detected:${colors.reset}`);
    for (const r of failureReasons) {
      console.log(`  ${colors.red}✖ ${r}${colors.reset}`);
    }
  } else if (compositeScore > SCORE_WARN_THRESHOLD) {
    console.log(`\n${colors.yellow}${colors.bold}[WARN] Performance degradation within warning margin (+${((compositeScore - 1) * 100).toFixed(2)}%).${colors.reset}`);
  } else {
    console.log(`\n${colors.green}${colors.bold}[PASS] All SLA regression gates passed!${colors.reset}`);
  }

  return { passed, score: compositeScore };
}

// Main Runner Routine
async function main() {
  console.log(`${colors.bold}${colors.blue}=== StepVisualizer Performance & Memory Benchmark ===${colors.reset}`);
  console.log(`File: ${targetFile} | Runs: ${runs} (+${warmup} warmup)`);

  if (!fs.existsSync(DIST_DIR)) {
    console.error(`${colors.red}[ERROR] dist directory '${DIST_DIR}' does not exist.${colors.reset}`);
    console.error(`Please run 'trunk build --release' in 'misc/benchmarks' first.`);
    process.exit(1);
  }

  let server;
  try {
    server = await startServer();
    console.log(`${colors.green}✓ Local HTTP test server listening on http://127.0.0.1:${port}${colors.reset}`);

    const fileParam = targetFile.startsWith('/') ? targetFile : `/${targetFile}`;
    const targetUrl = `http://127.0.0.1:${port}/?file=${fileParam}&runs=${runs}&warmup=${warmup}`;

    const currentResult = await runChromium(targetUrl);

    // Read baseline if available
    let baselineResult = null;
    if (fs.existsSync(BASELINE_FILE)) {
      try {
        baselineResult = JSON.parse(fs.readFileSync(BASELINE_FILE, 'utf8'));
      } catch (err) {
        console.warn(`[WARN] Could not parse ${BASELINE_FILE}: ${err.message}`);
      }
    }

    // Evaluate diff & print report
    const { passed } = printComparison(currentResult, baselineResult);

    // Always record last_run.json
    fs.writeFileSync(LAST_RUN_FILE, JSON.stringify(currentResult, null, 2), 'utf8');
    console.log(`\nSaved current run details to: ${colors.dim}${LAST_RUN_FILE}${colors.reset}`);

    // Update baseline if requested or if none existed
    if (updateBaseline || !baselineResult) {
      fs.writeFileSync(BASELINE_FILE, JSON.stringify(currentResult, null, 2), 'utf8');
      console.log(`${colors.green}${colors.bold}[✓] Baseline successfully updated at: ${BASELINE_FILE}${colors.reset}`);
    }

    server.close();
    process.exit(passed ? 0 : 1);
  } catch (err) {
    if (server) server.close();
    console.error(`\n${colors.red}${colors.bold}[ERROR] Benchmark runner failed:${colors.reset} ${err.message}`);
    process.exit(1);
  }
}

main();
