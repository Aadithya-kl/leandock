import * as yaml from 'js-yaml';

export type HostPresetId =
  | 'linux-kvm-4gb'
  | 'linux-kvm-1gb'
  | 'linux-kvm-512mb'
  | 'linux-lxc-2gb'
  | 'linux-nonroot-4gb'
  | 'windows-wsl2-16gb'
  | 'macos-arm64-16gb';

export interface HostPreset {
  id: HostPresetId;
  label: string;
  os: 'Linux' | 'Windows' | 'macOS';
  arch: 'x86_64' | 'aarch64';
  distro: string;
  virtualization: string;
  totalRamMb: number;
  effectiveRamMb: number;
  cores: number;
  swapMb: number;
  isRoot: boolean;
  kernel: string;
  systemd: string;
  tiers: {
    tier1: 'Full' | 'Partial' | 'Auto-Skipped';
    tier2: 'Full' | 'Partial' | 'Auto-Skipped';
    tier3: 'Full' | 'Partial' | 'Auto-Skipped';
    tier3Note: string;
    tier4: 'Full' | 'Partial' | 'Auto-Skipped';
    tier4Note: string;
  };
}

export const HOST_PRESETS: HostPreset[] = [
  {
    id: 'linux-kvm-4gb',
    label: 'Linux VPS (KVM · 4 GB · root)',
    os: 'Linux',
    arch: 'x86_64',
    distro: 'Ubuntu 24.04 LTS',
    virtualization: 'kvm',
    totalRamMb: 4096,
    effectiveRamMb: 4096,
    cores: 2,
    swapMb: 2048,
    isRoot: true,
    kernel: '6.8.0-45-generic',
    systemd: '255',
    tiers: {
      tier1: 'Full',
      tier2: 'Full',
      tier3: 'Full',
      tier3Note: 'daemon.json deep-merge (userland-proxy=false, log cap, live-restore)',
      tier4: 'Full',
      tier4Note: 'zram (zstd, 2048 MB) + KSM (kernel >= 6.1 & systemd >= 254, MemoryKSM=yes)',
    },
  },
  {
    id: 'linux-kvm-1gb',
    label: 'Budget Linux VPS (KVM · 1 GB · root)',
    os: 'Linux',
    arch: 'aarch64',
    distro: 'Debian 12 (ARM64)',
    virtualization: 'kvm',
    totalRamMb: 1024,
    effectiveRamMb: 1024,
    cores: 1,
    swapMb: 1024,
    isRoot: true,
    kernel: '6.6.15-arm64',
    systemd: '254',
    tiers: {
      tier1: 'Full',
      tier2: 'Full',
      tier3: 'Full',
      tier3Note: 'daemon.json deep-merge + low-RAM MySQL performance-schema=OFF',
      tier4: 'Full',
      tier4Note: 'zram (zstd, 1024 MB) + KSM active',
    },
  },
  {
    id: 'linux-kvm-512mb',
    label: 'Ultra-Low VPS (512 MB · No Swap)',
    os: 'Linux',
    arch: 'x86_64',
    distro: 'Alpine / Debian 12',
    virtualization: 'kvm',
    totalRamMb: 512,
    effectiveRamMb: 512,
    cores: 1,
    swapMb: 0,
    isRoot: true,
    kernel: '6.1.0-22-amd64',
    systemd: '252',
    tiers: {
      tier1: 'Full',
      tier2: 'Full',
      tier3: 'Full',
      tier3Note: 'daemon.json tuned + strict DB floor guards',
      tier4: 'Partial',
      tier4Note: 'zram (512 MB) active; KSM skipped cleanly (systemd 252 < 254)',
    },
  },
  {
    id: 'linux-lxc-2gb',
    label: 'Container VPS (LXC / OpenVZ · 2 GB)',
    os: 'Linux',
    arch: 'x86_64',
    distro: 'Ubuntu 22.04 LTS',
    virtualization: 'lxc',
    totalRamMb: 2048,
    effectiveRamMb: 2048,
    cores: 2,
    swapMb: 512,
    isRoot: true,
    kernel: '6.8.0 (shared)',
    systemd: '255',
    tiers: {
      tier1: 'Full',
      tier2: 'Full',
      tier3: 'Partial',
      tier3Note: 'daemon.json available if root; container-level log caps active',
      tier4: 'Auto-Skipped',
      tier4Note: 'Shared kernel (read-only /sys, no modprobe): zram & KSM safely skipped',
    },
  },
  {
    id: 'linux-nonroot-4gb',
    label: 'Linux User (Non-Root · 4 GB)',
    os: 'Linux',
    arch: 'x86_64',
    distro: 'Ubuntu 24.04 LTS',
    virtualization: 'kvm',
    totalRamMb: 4096,
    effectiveRamMb: 4096,
    cores: 2,
    swapMb: 0,
    isRoot: false,
    kernel: '6.8.0',
    systemd: '255',
    tiers: {
      tier1: 'Full',
      tier2: 'Full',
      tier3: 'Partial',
      tier3Note: 'Skipped /etc/docker/daemon.json without sudo; container log caps active',
      tier4: 'Auto-Skipped',
      tier4Note: 'Non-root mode: zram/KSM skipped cleanly with notice',
    },
  },
  {
    id: 'windows-wsl2-16gb',
    label: 'Windows 11 (WSL2 · 16 GB Host)',
    os: 'Windows',
    arch: 'x86_64',
    distro: 'Windows 11 + Docker Desktop',
    virtualization: 'wsl2',
    totalRamMb: 16384,
    effectiveRamMb: 6144,
    cores: 8,
    swapMb: 2048,
    isRoot: false,
    kernel: 'n/a (WSL2 VM)',
    systemd: 'n/a',
    tiers: {
      tier1: 'Full',
      tier2: 'Full',
      tier3: 'Full',
      tier3Note: '.wslconfig tuned (memory=6GB, autoMemoryReclaim=gradual, sparseVhd=true) + NTFS warning',
      tier4: 'Auto-Skipped',
      tier4Note: 'Windows host: Linux kernel tier skipped cleanly',
    },
  },
  {
    id: 'macos-arm64-16gb',
    label: 'macOS Apple Silicon (ARM64 · 16 GB)',
    os: 'macOS',
    arch: 'aarch64',
    distro: 'macOS 15 + OrbStack / Colima',
    virtualization: 'macos-vm',
    totalRamMb: 16384,
    effectiveRamMb: 6144,
    cores: 10,
    swapMb: 0,
    isRoot: false,
    kernel: 'n/a (Linux VM)',
    systemd: 'n/a',
    tiers: {
      tier1: 'Full',
      tier2: 'Full',
      tier3: 'Partial',
      tier3Note: 'Detects OrbStack / Colima / Docker Desktop socket & reports VM RAM guidance',
      tier4: 'Auto-Skipped',
      tier4Note: 'macOS host: Linux kernel tier skipped cleanly',
    },
  },
];

export interface AnalyzedService {
  name: string;
  role: string;
  dbKind: string | null;
  ignored: boolean;
  offloaded: boolean;
  memLimitMb: number | null;
  memReservationMb: number | null;
  logRotation: boolean;
  tunedCommand: string[] | null;
  extraEnv: Record<string, string>;
  notes: string[];
}

export interface AnalysisResult {
  error: string | null;
  effectiveRamMb: number;
  osReserveMb: number;
  allocatableRamMb: number;
  totalBudgetedMb: number;
  estimatedDefaultMb: number;
  estimatedSavingsMb: number;
  services: AnalyzedService[];
  warnings: string[];
  overrideYaml: string;
}

function parseLabels(raw: unknown): Record<string, string> {
  const map: Record<string, string> = {};
  if (Array.isArray(raw)) {
    for (const item of raw) {
      if (typeof item === 'string') {
        const idx = item.indexOf('=');
        if (idx >= 0) {
          map[item.slice(0, idx).trim()] = item.slice(idx + 1).trim();
        } else {
          map[item.trim()] = 'true';
        }
      }
    }
  } else if (raw && typeof raw === 'object') {
    for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
      map[k.trim()] = v == null ? '' : String(v);
    }
  }
  return map;
}

function labelTruthy(labels: Record<string, string>, key: string): boolean {
  const val = labels[key];
  if (!val) return false;
  return ['true', '1', 'yes', 'on'].includes(val.trim().toLowerCase());
}

function detectDb(image?: string): string | null {
  if (!image) return null;
  const leaf = image.trim().toLowerCase().split('@')[0].split(':')[0].split('/').pop() ?? '';
  if (leaf === 'mongo' || leaf === 'mongodb-community-server') return 'MongoDB';
  if (['postgres', 'postgis', 'timescaledb', 'pgvector'].includes(leaf)) return 'PostgreSQL';
  if (['redis', 'redis-stack', 'redis-stack-server', 'valkey'].includes(leaf)) return 'Redis/Valkey';
  if (['mysql', 'mariadb', 'percona'].includes(leaf)) return 'MySQL/MariaDB';
  if (leaf === 'qdrant') return 'Qdrant';
  return null;
}

function classifyRole(name: string, image: string | undefined, db: string | null, labels: Record<string, string>): string {
  if (db === 'Redis/Valkey') return 'cache';
  if (db) return 'database';
  if (labels['leandock.static_dir']) return 'frontend';
  const l = `${name} ${image ?? ''}`.toLowerCase();
  if (/frontend|web|nginx|caddy|httpd/.test(l)) return 'frontend';
  if (/worker|celery|queue|cron|ocr|scraper/.test(l)) return 'worker';
  return 'app';
}

function isShellCommand(cmd: unknown): boolean {
  if (typeof cmd === 'string') {
    const t = cmd.trim();
    const l = t.toLowerCase();
    return (
      l.startsWith('sh ') ||
      l.startsWith('/bin/sh ') ||
      l.startsWith('bash ') ||
      l.startsWith('/bin/bash ') ||
      l.includes(' -c ') ||
      /[;&|$]/.test(t)
    );
  }
  if (Array.isArray(cmd) && cmd.length > 0) {
    const first = String(cmd[0]).toLowerCase();
    return (
      ['sh', '/bin/sh', 'bash', '/bin/bash'].includes(first) &&
      cmd.map(String).includes('-c')
    );
  }
  return false;
}

function extractTokens(cmd: unknown): string[] {
  if (Array.isArray(cmd)) return cmd.map(String);
  if (typeof cmd === 'string') return cmd.trim().split(/\s+/).filter(Boolean);
  return [];
}

function roleWeight(role: string, db: string | null): number {
  if (db === 'MongoDB' || db === 'MySQL/MariaDB') return 5;
  if (role === 'database') return 4;
  if (role === 'app' || role === 'worker') return 3;
  if (role === 'cache') return 2;
  return 1;
}

function minRam(role: string, db: string | null): number {
  if (db === 'MongoDB') return 384;
  if (db === 'MySQL/MariaDB') return 256;
  if (role === 'database') return 192;
  if (role === 'cache' || role === 'frontend') return 64;
  return 128;
}

function maxRam(role: string, hostMb: number): number {
  const cap = Math.max(512, Math.floor(hostMb / 2));
  if (role === 'frontend') return 256;
  if (role === 'cache') return Math.min(cap, 1024);
  if (role === 'database') return Math.min(cap, 4096);
  return Math.min(cap, 2048);
}

function estimateDefaultMb(role: string, db: string | null, hostMb: number): number {
  if (db === 'MongoDB') return Math.max(256, Math.floor(Math.max(0, hostMb - 1024) / 2)) + 200;
  if (db === 'PostgreSQL') return Math.max(384, Math.floor(hostMb / 5));
  if (db === 'MySQL/MariaDB') return Math.max(450, Math.floor(hostMb / 4));
  if (db === 'Redis/Valkey') return Math.max(256, Math.floor(hostMb / 8));
  if (db === 'Qdrant') return Math.max(512, Math.floor(hostMb / 4));
  if (role === 'frontend') return 96;
  if (role === 'worker') return 384;
  return 320;
}

export function analyzeComposeYaml(
  yamlText: string,
  host: HostPreset,
  options: { ephemeralCache: boolean; extractStatic: boolean }
): AnalysisResult {
  try {
    const doc = yaml.load(yamlText) as Record<string, unknown> | null;
    if (!doc || typeof doc !== 'object' || !doc.services || typeof doc.services !== 'object') {
      return {
        error: 'Compose YAML must contain a top-level `services:` mapping.',
        effectiveRamMb: host.effectiveRamMb,
        osReserveMb: 384,
        allocatableRamMb: 0,
        totalBudgetedMb: 0,
        estimatedDefaultMb: 0,
        estimatedSavingsMb: 0,
        services: [],
        warnings: [],
        overrideYaml: '',
      };
    }

    const totalRamMb = Math.max(256, host.effectiveRamMb);
    const osReserveMb = Math.max(384, Math.round(totalRamMb * 0.15));
    const allocatableRamMb = Math.max(256, totalRamMb - osReserveMb);
    const svcEntries = Object.entries(doc.services as Record<string, Record<string, unknown>>);

    const weights: Record<string, number> = {};
    let weightSum = 0;

    for (const [name, cfg] of svcEntries) {
      if (!cfg || typeof cfg !== 'object') continue;
      const labels = parseLabels(cfg.labels);
      const ignored = labelTruthy(labels, 'leandock.ignore');
      const offloaded = options.extractStatic && !ignored && Boolean(labels['leandock.static_dir']);
      if (ignored || offloaded) continue;
      const db = detectDb(typeof cfg.image === 'string' ? cfg.image : undefined);
      const role = classifyRole(name, typeof cfg.image === 'string' ? cfg.image : undefined, db, labels);
      const w = roleWeight(role, db);
      weights[name] = w;
      weightSum += w;
    }

    const services: AnalyzedService[] = [];
    const warnings: string[] = [];
    const overrideServices: Record<string, Record<string, unknown>> = {};
    let totalBudgetedMb = 0;
    let estimatedDefaultMb = 0;

    for (const [name, cfg] of svcEntries) {
      if (!cfg || typeof cfg !== 'object') continue;
      const image = typeof cfg.image === 'string' ? cfg.image : undefined;
      const labels = parseLabels(cfg.labels);
      const db = detectDb(image);
      const role = classifyRole(name, image, db, labels);
      const ignored = labelTruthy(labels, 'leandock.ignore');

      if (ignored) {
        services.push({
          name,
          role,
          dbKind: db,
          ignored: true,
          offloaded: false,
          memLimitMb: null,
          memReservationMb: null,
          logRotation: false,
          tunedCommand: null,
          extraEnv: {},
          notes: ['Skipped completely (leandock.ignore=true)'],
        });
        continue;
      }

      if (options.extractStatic && labels['leandock.static_dir']) {
        estimatedDefaultMb += 96;
        overrideServices[name] = { profiles: ['leandock-offloaded'] };
        services.push({
          name,
          role,
          dbKind: db,
          ignored: false,
          offloaded: true,
          memLimitMb: null,
          memReservationMb: null,
          logRotation: false,
          tunedCommand: null,
          extraEnv: {},
          notes: [`Offloaded ${labels['leandock.static_dir']} to LeanDock Rust static server`],
        });
        continue;
      }

      const w = weights[name] ?? 2;
      const rawShare = weightSum > 0 ? Math.floor((allocatableRamMb * w) / weightSum) : 256;
      const targetMb = Math.min(maxRam(role, totalRamMb), Math.max(minRam(role, db), rawShare));

      estimatedDefaultMb += estimateDefaultMb(role, db, totalRamMb);
      totalBudgetedMb += targetMb;

      const deploy = cfg.deploy as Record<string, Record<string, Record<string, unknown>>> | undefined;
      const hasExistingMem = Boolean(cfg.mem_limit || deploy?.resources?.limits?.memory);
      const hasLogging = Boolean(cfg.logging);

      const notes: string[] = [];
      const memLimitMb = hasExistingMem ? null : targetMb;
      const memReservationMb = hasExistingMem ? null : Math.max(32, Math.round(targetMb * 0.45));
      if (hasExistingMem) {
        notes.push('Preserved existing memory limit');
      }

      let tunedCommand: string[] | null = null;
      const extraEnv: Record<string, string> = {};

      if (db === 'Qdrant') {
        if (host.cores <= 2) {
          extraEnv.QDRANT__STORAGE__PERFORMANCE__MAX_OPTIMIZATION_THREADS = '1';
          notes.push('Qdrant: MAX_OPTIMIZATION_THREADS=1');
        }
      } else if (db) {
        if (isShellCommand(cfg.command)) {
          notes.push('Skipped DB flags (shell-wrapper command)');
          warnings.push(`Service "${name}" (${db}) uses a shell-wrapper command; skipped flag injection safely.`);
        } else {
          const existing = extractTokens(cfg.command);
          if (db === 'MongoDB') {
            if (!existing.some((a) => a === '--wiredTigerCacheSizeGB' || a.startsWith('--wiredTigerCacheSizeGB='))) {
              const cacheGb = Math.max(0.25, Math.round(((targetMb * 0.55) / 1024) * 100) / 100);
              tunedCommand = [...(existing.length ? existing : ['mongod']), '--wiredTigerCacheSizeGB', cacheGb.toFixed(2)];
              notes.push(`MongoDB: WiredTiger cache ${cacheGb.toFixed(2)} GB`);
            } else {
              notes.push('MongoDB: preserved user WiredTiger flag');
            }
          } else if (db === 'PostgreSQL') {
            const sb = Math.max(64, Math.round(targetMb * 0.25));
            const ec = Math.max(128, Math.round(targetMb * 0.6));
            const wm = Math.min(16, Math.max(2, Math.floor(targetMb / 128)));
            const mc = totalRamMb <= 1024 ? 30 : totalRamMb <= 2048 ? 50 : 100;
            const cmd = existing.length ? [...existing] : ['postgres'];
            const pairs: [string, string][] = [
              ['shared_buffers', `${sb}MB`],
              ['effective_cache_size', `${ec}MB`],
              ['work_mem', `${wm}MB`],
              ['max_connections', String(mc)],
            ];
            for (const [k, v] of pairs) {
              if (!cmd.some((a) => a.startsWith(`${k}=`) || a.startsWith(`-c${k}=`))) {
                cmd.push('-c', `${k}=${v}`);
              }
            }
            tunedCommand = cmd;
            notes.push(`PostgreSQL: shared_buffers=${sb}MB, max_connections=${mc}`);
          } else if (db === 'Redis/Valkey') {
            const mm = Math.max(32, Math.round(targetMb * 0.75));
            const ephemeral = options.ephemeralCache || labelTruthy(labels, 'leandock.redis.ephemeral');
            const cmd = existing.length ? [...existing] : ['redis-server'];
            if (!cmd.some((a) => a === '--maxmemory' || a.startsWith('--maxmemory='))) {
              cmd.push('--maxmemory', `${mm}mb`);
            }
            if (ephemeral) {
              cmd.push('--maxmemory-policy', 'allkeys-lru', '--save', '');
              notes.push(`Redis/Valkey: ephemeral cache (${mm}mb, allkeys-lru, --save "")`);
            } else {
              cmd.push('--maxmemory-policy', 'noeviction');
              notes.push(`Redis/Valkey: persistence-safe (${mm}mb, noeviction)`);
            }
            tunedCommand = cmd;
          } else if (db === 'MySQL/MariaDB') {
            const pool = Math.max(64, Math.round(targetMb * 0.5));
            const cmd = existing.length ? [...existing] : ['mysqld'];
            cmd.push(`--innodb-buffer-pool-size=${pool}M`);
            if (totalRamMb <= 2048) {
              cmd.push('--performance-schema=OFF');
            }
            tunedCommand = cmd;
            notes.push(`MySQL/MariaDB: buffer-pool=${pool}M${totalRamMb <= 2048 ? ', performance-schema=OFF' : ''}`);
          }
        }
      }

      const ovEntry: Record<string, unknown> = {};
      if (memLimitMb != null) ovEntry.mem_limit = `${memLimitMb}m`;
      if (memReservationMb != null) ovEntry.mem_reservation = `${memReservationMb}m`;
      if (!hasLogging) {
        ovEntry.logging = {
          driver: 'json-file',
          options: { 'max-size': '10m', 'max-file': '3' },
        };
      }
      if (tunedCommand) ovEntry.command = tunedCommand;
      if (Object.keys(extraEnv).length > 0) ovEntry.environment = extraEnv;
      if (Object.keys(ovEntry).length > 0) overrideServices[name] = ovEntry;

      services.push({
        name,
        role,
        dbKind: db,
        ignored: false,
        offloaded: false,
        memLimitMb,
        memReservationMb,
        logRotation: !hasLogging,
        tunedCommand,
        extraEnv,
        notes,
      });
    }

    if (totalBudgetedMb > allocatableRamMb) {
      const withSwap = totalRamMb + host.swapMb;
      if (totalBudgetedMb > withSwap) {
        warnings.push(
          `Ultra-low RAM warning: minimum safe stack footprint (~${totalBudgetedMb} MB) exceeds RAM + swap (${withSwap} MB). Enable zram or Wake-on-Request.`
        );
      } else {
        warnings.push(
          `Tight RAM budget: safe minimum floors total ~${totalBudgetedMb} MB vs ${allocatableRamMb} MB allocatable RAM (headroom covered by swap/zram).`
        );
      }
    }

    const header =
      '# Generated by LeanDock — DO NOT EDIT MANUALLY.\n# Original Compose file is untouched. Run `leandock revert` to remove.\n';
    const overrideYaml = header + yaml.dump({ services: overrideServices }, { lineWidth: 100 });

    return {
      error: null,
      effectiveRamMb: totalRamMb,
      osReserveMb,
      allocatableRamMb,
      totalBudgetedMb,
      estimatedDefaultMb,
      estimatedSavingsMb: Math.max(0, estimatedDefaultMb - totalBudgetedMb),
      services,
      warnings,
      overrideYaml,
    };
  } catch (err) {
    return {
      error: err instanceof Error ? err.message : 'Invalid YAML syntax',
      effectiveRamMb: host.effectiveRamMb,
      osReserveMb: 384,
      allocatableRamMb: 0,
      totalBudgetedMb: 0,
      estimatedDefaultMb: 0,
      estimatedSavingsMb: 0,
      services: [],
      warnings: [],
      overrideYaml: '',
    };
  }
}
