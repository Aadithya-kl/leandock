# Reproducible Benchmark Methodology

LeanDock includes a built-in benchmarking command (`leandock bench`) so every user can verify real, local Before vs. After measurements on their exact hardware and Compose stack — rather than trusting opaque marketing numbers.

---

## Running a Reproducible Benchmark

```bash
cd examples/fullstack-mongo-api

# Output as an aligned terminal table
leandock bench --settle-secs 15

# Output as GitHub-ready Markdown table
leandock bench --format markdown --settle-secs 15

# Output as machine-readable JSON for CI tracking
leandock bench --format json --settle-secs 15 > bench-results.json
```

## What `leandock bench` Measures

1. **Baseline Run (`Raw Compose`):**
   - Executes unmodified `<compose> -f <file> up -d` and times cold startup (`startup_ms`).
   - Waits `--settle-secs` (default `8s`, recommended `15s`–`30s` for heavy DBs) for caches and workers to warm up.
   - Captures host RAM used (MB), Swap/zram used (MB), primary disk used (GB), and host process count.
   - Cleanly tears down the raw stack (`down`).
2. **Optimized Run (`LeanDock Optimized`):**
   - Generates `.leandock/compose.override.yml` using the exact hardware profile.
   - Executes `<compose> -f <file> -f .leandock/compose.override.yml up -d` and times startup (`startup_ms`).
   - Waits the identical `--settle-secs` duration and captures the exact same metrics.
3. **Delta Computation:**
   - Computes signed deltas for RAM (MB and %), Swap/zram (MB), Disk (GB), Process Count, and Startup Time (ms).

## Reference Methodology Notes

For fair, apples-to-apples VPS benchmarks:
- Run on a freshly rebooted VPS or drop page caches between runs (`sync && echo 3 | sudo tee /proc/sys/vm/drop_caches`).
- Keep container images pre-pulled before running `leandock bench` so network download variance does not skew `startup_ms`.
- To benchmark **Wake-on-Request (Scale-to-Zero)** savings across multi-project hosts, record `leandock status` once idle containers transition to `Sleeping (0 MB)` and measure first-request cold wake latency via `curl -w "%{time_total}\n" http://localhost:8081/`.

## Example Output (`--format markdown`)

```markdown
| Metric | Before (`Raw Compose`) | After (`LeanDock Optimized`) | Delta |
|---|---:|---:|---:|
| RAM Used (MB) | 1842 | 964 | -878 (-47.7%) |
| Swap/zram Used (MB) | 0 | 48 | +48 |
| Disk Used (GB) | 11.40 | 10.12 | -1.28 |
| Processes | 148 | 132 | -16 |
| Startup Time (ms) | 4120 | 4280 | +160 |
```
