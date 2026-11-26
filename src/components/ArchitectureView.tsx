import { ArrowUpRight } from 'lucide-react';

interface ArchitectureViewProps {
  onOpenFile: (path: string) => void;
}

const PIPELINE_PHASES = [
  {
    step: '01. Phase 0 — Pre-Flight Profiler',
    module: 'src/profiler/platform.rs',
    summary:
      'Detects OS (Linux, Windows, macOS), CPU architecture (x86_64, aarch64), RAM/swap/disk, kernel & systemd versions, virtualization (KVM, LXC, OpenVZ, WSL2), and container sockets.',
  },
  {
    step: '02. Phase 1 — Pre-Run Interceptor',
    module: 'src/prerun/db_tuner.rs',
    summary:
      'Applies OS-appropriate host tuning via transactional backups and emits isolated .leandock/compose.override.yml with dynamic RAM caps, log rotation, and safe DB flags without modifying original Compose files.',
  },
  {
    step: '03. Phase 2 — Post-Run & Health Guard',
    module: 'src/postrun/health_guard.rs',
    summary:
      'Configures real Linux zram (zstd) and KSM (kernel >= 6.1 + systemd >= 254 + MemoryKSM=yes), monitors stability for 10s with automatic raw-Compose rollback on crash-loop, and prunes only dangling layers/cache.',
  },
  {
    step: '04. Phase 3 — Wake-on-Request Proxy',
    module: 'src/proxy/wake_manager.rs',
    summary:
      'Async Tokio + Hyper v1 proxy backed by cross-platform ContainerEngine (Unix Socket on Unix, CLI adapter on Windows). Stops idle containers (0 MB RAM) and coalesces concurrent wake requests via singleflight.',
  },
];

const EDGE_CASES = [
  {
    id: '01',
    title: 'Per-service escape hatch (leandock.ignore=true)',
    guarantee: 'Supports both YAML map and list labels; guarantees zero mutations across DB tuning, offload, minify, and proxy.',
    testFile: 'tests/db_tuner_tests.rs',
  },
  {
    id: '02',
    title: 'Non-root execution',
    guarantee: 'Never crashes without sudo; skips host/kernel tweaks cleanly while keeping Tier 1 & Tier 2 fully active.',
    testFile: 'tests/platform_tier_tests.rs',
  },
  {
    id: '03',
    title: 'Restricted VPS (OpenVZ / LXC / WSL2)',
    guarantee: 'Detects shared kernels reliably and auto-skips zram/KSM without aborting leandock up.',
    testFile: 'tests/platform_tier_tests.rs',
  },
  {
    id: '04',
    title: 'Windows & macOS cross-compilation safety',
    guarantee: 'All Linux paths/syscalls gated via #[cfg(target_os = "linux")]; nix gated under [target.\'cfg(unix)\'.dependencies].',
    testFile: 'src/proxy/engine_client.rs',
  },
  {
    id: '05',
    title: 'Custom DB commands & shell wrappers',
    guarantee: 'Preserves array args (e.g. --replSet rs0), deduplicates user flags, and skips ambiguous sh -c strings with a warning.',
    testFile: 'tests/db_tuner_tests.rs',
  },
  {
    id: '06',
    title: 'Compose YAML v2/v3 & ${VAR:-default}',
    guarantee: 'Original Compose file stays untouched on disk; preserves mem_limit, deploy.resources.limits.memory, and build-only services.',
    testFile: 'tests/compose_override_tests.rs',
  },
  {
    id: '07',
    title: 'Ultra-low RAM VPS (512 MB / 1 GB)',
    guarantee: 'Hard safety floors (Mongo 0.25 GB, Postgres 64 MB, Redis 32 MB, MySQL 64 MB) + overcommit warnings.',
    testFile: 'tests/db_tuner_tests.rs',
  },
  {
    id: '08',
    title: '50-request Wake-on-Request stampede',
    guarantee: 'Singleflight Notify ensures 50 concurrent HTTP requests trigger exactly ONE start_container call; timeouts return HTTP 503.',
    testFile: 'tests/proxy_singleflight_tests.rs',
  },
  {
    id: '09',
    title: 'Crash-loop detection & auto-rollback',
    guarantee: 'Detects restarting/unhealthy/non-zero exits post-deploy, dumps 30 log lines, and restores raw Compose automatically.',
    testFile: 'tests/compose_override_tests.rs',
  },
  {
    id: '10',
    title: 'Atomic config writes & leandock revert',
    guarantee: 'Sibling temp-file + atomic rename with timestamped manifest snapshots; one command restores all touched host files.',
    testFile: 'src/backup/mod.rs',
  },
];

const CLI_COMMANDS = [
  ['leandock analyze', '-f <file>, --ephemeral-cache', 'Zero-touch dry run showing hardware tier, ignored services, and proposed DB/memory caps'],
  ['leandock up', '--engine, --no-kernel, --no-cleanup, --minify, --extract-static, --auto-rollback, --dry-run', 'Generate isolated override, tune supported host tiers, deploy, verify health, clean dangling cache'],
  ['leandock down', '-f <file>, --engine docker|podman', 'Stop and remove the Compose stack cleanly'],
  ['leandock proxy', '--port-map svc:listen:target[:timeout], --install-service', 'Run cross-platform Wake-on-Request HTTP proxy (or install Linux systemd unit) and static server'],
  ['leandock quadlet', '-f <file>, -o <dir>', 'Export daemonless systemd Quadlet .container units for Linux Podman'],
  ['leandock status', '—', 'Live container RAM/CPU, zram compression ratio, KSM deduplicated pages, and Wake states'],
  ['leandock bench', '--format table|markdown|json, --settle-secs <n>', 'Reproducible Before vs. After resource benchmark'],
  ['leandock revert', '--all', 'Restore backed-up host configs (daemon.json, sysctl, zram, KSM, .wslconfig) and remove .leandock/'],
  ['leandock doctor', '—', 'Full diagnostic check covering OS tier, virtualization, sockets, and permissions'],
];

export function ArchitectureView({ onOpenFile }: ArchitectureViewProps) {
  return (
    <div className="space-y-10">
      <div className="border-b border-slate-800 pb-6">
        <h1 className="text-2xl font-semibold tracking-tight text-slate-50">
          Architecture, Safeguards & CLI Reference
        </h1>
        <p className="mt-1.5 max-w-3xl text-sm text-slate-400">
          Built around three non-negotiable oaths: zero workflow friction, non-destructive reversibility, and graceful degradation over clever breakage.
        </p>
      </div>

      {/* 4-Phase Pipeline */}
      <section className="space-y-4">
        <h2 className="text-base font-semibold text-slate-100">
          Execution Pipeline
        </h2>
        <div className="grid grid-cols-1 gap-px overflow-hidden rounded-lg border border-slate-800 bg-slate-800 md:grid-cols-2">
          {PIPELINE_PHASES.map((phase) => (
            <div key={phase.step} className="flex flex-col justify-between bg-slate-950 p-5">
              <div>
                <h3 className="text-sm font-semibold text-slate-100">{phase.step}</h3>
                <p className="mt-2 text-xs leading-5 text-slate-400">{phase.summary}</p>
              </div>
              <div className="mt-4">
                <button
                  type="button"
                  onClick={() => onOpenFile(phase.module)}
                  className="inline-flex items-center gap-1 font-mono text-xs text-sky-400 hover:text-sky-300"
                >
                  Inspect {phase.module}
                  <ArrowUpRight className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>
          ))}
        </div>
      </section>

      {/* Edge-Case & Automated Test Verification Matrix */}
      <section className="overflow-hidden rounded-lg border border-slate-800">
        <div className="border-b border-slate-800 bg-slate-900/60 px-4 py-3">
          <h2 className="text-sm font-semibold text-slate-100">
            Mandatory Edge Cases & Automated Test Coverage
          </h2>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs">
            <thead>
              <tr className="border-b border-slate-800 bg-slate-950 text-slate-400">
                <th className="w-12 px-4 py-2.5 font-medium">#</th>
                <th className="px-4 py-2.5 font-medium">Scenario</th>
                <th className="px-4 py-2.5 font-medium">Engineering Guarantee</th>
                <th className="px-4 py-2.5 text-right font-medium">Source / Test</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800/80 bg-slate-950/60">
              {EDGE_CASES.map((ec) => (
                <tr key={ec.id} className="hover:bg-slate-900/50">
                  <td className="px-4 py-3 font-mono tabular-nums text-slate-500">{ec.id}</td>
                  <td className="px-4 py-3 font-medium text-slate-100">{ec.title}</td>
                  <td className="px-4 py-3 text-slate-400">{ec.guarantee}</td>
                  <td className="px-4 py-3 text-right">
                    <button
                      type="button"
                      onClick={() => onOpenFile(ec.testFile)}
                      className="inline-flex items-center gap-1 whitespace-nowrap font-mono text-sky-400 hover:text-sky-300"
                    >
                      {ec.testFile}
                      <ArrowUpRight className="h-3 w-3" />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      {/* CLI Reference */}
      <section className="overflow-hidden rounded-lg border border-slate-800">
        <div className="border-b border-slate-800 bg-slate-900/60 px-4 py-3">
          <h2 className="text-sm font-semibold text-slate-100">
            CLI Command Reference
          </h2>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs">
            <thead>
              <tr className="border-b border-slate-800 bg-slate-950 text-slate-400">
                <th className="px-4 py-2.5 font-medium">Command</th>
                <th className="px-4 py-2.5 font-medium">Key Flags</th>
                <th className="px-4 py-2.5 font-medium">Behavior</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800/80 bg-slate-950/60">
              {CLI_COMMANDS.map(([cmd, flags, desc]) => (
                <tr key={cmd} className="hover:bg-slate-900/50">
                  <td className="whitespace-nowrap px-4 py-3 font-mono font-medium text-sky-300">
                    {cmd}
                  </td>
                  <td className="px-4 py-3 font-mono text-slate-400">{flags}</td>
                  <td className="px-4 py-3 text-slate-300">{desc}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}
