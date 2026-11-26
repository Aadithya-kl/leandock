import { useMemo, useState } from 'react';
import { Check, Copy, FileCode2, Search } from 'lucide-react';
import { REPO_FILES, type RepoFile } from '../lib/repoFiles';

interface RepoExplorerProps {
  selectedPath: string;
  onSelectPath: (path: string) => void;
}

type CategoryFilter = 'all' | 'rust' | 'tests' | 'docs' | 'ci-scripts';

export function RepoExplorer({ selectedPath, onSelectPath }: RepoExplorerProps) {
  const [query, setQuery] = useState('');
  const [category, setCategory] = useState<CategoryFilter>('all');
  const [copied, setCopied] = useState(false);

  const filteredFiles = useMemo(() => {
    const q = query.trim().toLowerCase();
    return REPO_FILES.filter((f) => {
      if (category === 'rust' && !f.path.startsWith('src/') && f.path !== 'Cargo.toml') {
        return false;
      }
      if (category === 'tests' && !f.path.startsWith('tests/')) {
        return false;
      }
      if (
        category === 'docs' &&
        !f.path.startsWith('docs/') &&
        !['README.md', 'CHANGELOG.md', 'CONTRIBUTING.md', 'LICENSE'].includes(f.path)
      ) {
        return false;
      }
      if (
        category === 'ci-scripts' &&
        !f.path.startsWith('.github/') &&
        !f.path.startsWith('scripts/') &&
        !f.path.startsWith('examples/')
      ) {
        return false;
      }
      if (!q) return true;
      return f.path.toLowerCase().includes(q) || f.content.toLowerCase().includes(q);
    });
  }, [query, category]);

  const activeFile: RepoFile =
    REPO_FILES.find((f) => f.path === selectedPath) ?? filteredFiles[0] ?? REPO_FILES[0];

  const totalLines = useMemo(
    () => REPO_FILES.reduce((sum, f) => sum + f.lines, 0),
    []
  );

  const handleCopy = async () => {
    await navigator.clipboard.writeText(activeFile.content);
    setCopied(true);
    setTimeout(() => setCopied(false), 1800);
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-3 border-b border-slate-800 pb-5 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight text-slate-50">
            Complete Rust Repository
          </h1>
          <div className="mt-1.5 text-xs text-slate-400">
            <span className="font-mono tabular-nums text-slate-200">{REPO_FILES.length}</span> files
            <span className="mx-2 text-slate-600">·</span>
            <span className="font-mono tabular-nums text-slate-200">{totalLines.toLocaleString()}</span> lines of Rust, tests, CI workflows, scripts & docs
            <span className="mx-2 text-slate-600">·</span>
            Zero placeholders
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-1 rounded-md bg-slate-900 p-1">
          {(
            [
              ['all', 'All Files'],
              ['rust', 'Rust Src'],
              ['tests', 'Tests'],
              ['docs', 'Docs'],
              ['ci-scripts', 'CI & Examples'],
            ] as [CategoryFilter, string][]
          ).map(([id, label]) => (
            <button
              key={id}
              type="button"
              onClick={() => setCategory(id)}
              className={`whitespace-nowrap rounded px-2.5 py-1.5 text-xs font-medium transition-colors ${
                category === id
                  ? 'bg-slate-800 text-slate-50'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      <div className="grid grid-cols-1 gap-6 lg:grid-cols-12">
        {/* File List Sidebar */}
        <div className="flex flex-col rounded-lg border border-slate-800 bg-slate-900/40 lg:col-span-4">
          <div className="border-b border-slate-800 p-3">
            <div className="relative">
              <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-slate-500" />
              <input
                type="text"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Filter path or code..."
                className="w-full rounded-md border border-slate-800 bg-slate-950 py-2 pl-8 pr-3 text-xs text-slate-100 placeholder:text-slate-500 focus:border-sky-500 focus:outline-none"
              />
            </div>
          </div>

          <div className="max-h-[640px] divide-y divide-slate-800/50 overflow-y-auto">
            {filteredFiles.length === 0 ? (
              <div className="p-6 text-center text-xs text-slate-500">
                No repository files match "{query}".
              </div>
            ) : (
              filteredFiles.map((file) => {
                const isSelected = file.path === activeFile.path;
                return (
                  <button
                    key={file.path}
                    type="button"
                    onClick={() => onSelectPath(file.path)}
                    className={`flex w-full items-center justify-between gap-2 px-3.5 py-2.5 text-left text-xs transition-colors ${
                      isSelected
                        ? 'bg-sky-500/10 text-sky-300'
                        : 'text-slate-300 hover:bg-slate-800/50 hover:text-slate-100'
                    }`}
                  >
                    <span className="flex min-w-0 items-center gap-2">
                      <FileCode2 className="h-3.5 w-3.5 shrink-0 text-slate-500" />
                      <span className="truncate font-mono">{file.path}</span>
                    </span>
                    <span className="shrink-0 font-mono text-[11px] tabular-nums text-slate-500">
                      {file.lines}L
                    </span>
                  </button>
                );
              })
            )}
          </div>
        </div>

        {/* Code Viewer */}
        <div className="flex flex-col overflow-hidden rounded-lg border border-slate-800 bg-slate-900/40 lg:col-span-8">
          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-slate-800 px-4 py-3">
            <div className="min-w-0">
              <div className="truncate font-mono text-xs font-medium text-slate-100">
                leandock/{activeFile.path}
              </div>
              <div className="mt-0.5 text-[11px] text-slate-500">
                <span className="font-mono tabular-nums">{activeFile.lines}</span> lines
                <span className="mx-1.5">·</span>
                <span className="font-mono tabular-nums">
                  {(activeFile.bytes / 1024).toFixed(1)} KB
                </span>
              </div>
            </div>

            <button
              type="button"
              onClick={handleCopy}
              className="inline-flex shrink-0 items-center gap-1.5 rounded-md border border-slate-700 bg-slate-900 px-3 py-1.5 text-xs font-medium text-slate-200 transition-colors hover:bg-slate-800"
            >
              {copied ? (
                <Check className="h-3.5 w-3.5 text-emerald-400" />
              ) : (
                <Copy className="h-3.5 w-3.5" />
              )}
              {copied ? 'Copied' : 'Copy File'}
            </button>
          </div>

          <div className="max-h-[640px] overflow-auto bg-slate-950 p-4">
            <table className="w-full border-collapse font-mono text-xs leading-6">
              <tbody>
                {activeFile.content.split('\n').map((line, i) => (
                  <tr key={i} className="hover:bg-slate-900/60">
                    <td className="w-10 select-none pr-4 text-right align-top tabular-nums text-slate-600">
                      {i + 1}
                    </td>
                    <td className="whitespace-pre text-slate-200">{line || ' '}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>
  );
}
