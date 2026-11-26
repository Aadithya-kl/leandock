import { useState } from 'react';
import {
  Download,
  FolderGit2,
  Layers,
  SlidersHorizontal,
} from 'lucide-react';
import { ArchitectureView } from './components/ArchitectureView';
import { ComposeAnalyzer } from './components/ComposeAnalyzer';
import { RepoExplorer } from './components/RepoExplorer';
import { downloadRepositoryZip, REPO_FILES } from './lib/repoFiles';

type ActiveView = 'analyzer' | 'repository' | 'architecture';

export default function App() {
  const [activeView, setActiveView] = useState<ActiveView>('analyzer');
  const [selectedFilePath, setSelectedFilePath] = useState<string>('README.md');
  const [downloading, setDownloading] = useState<boolean>(false);

  const handleDownloadZip = async () => {
    setDownloading(true);
    try {
      await downloadRepositoryZip();
    } finally {
      setDownloading(false);
    }
  };

  const handleOpenRepoFile = (path: string) => {
    setSelectedFilePath(path);
    setActiveView('repository');
  };

  const breadcrumbLabel =
    activeView === 'analyzer'
      ? 'Interactive Compose Analyzer'
      : activeView === 'repository'
      ? `Repository / ${selectedFilePath}`
      : 'Architecture & Safeguards';

  return (
    <div className="flex min-h-screen bg-slate-950 text-slate-100">
      {/* Left Navigation Sidebar */}
      <aside className="hidden w-64 shrink-0 flex-col border-r border-slate-800 bg-slate-950 lg:flex">
        <div className="flex h-14 items-center border-b border-slate-800 px-5">
          <span className="text-base font-bold tracking-tight text-slate-50 whitespace-nowrap">
            LeanDock
          </span>
        </div>

        <nav className="flex-1 space-y-1 p-3">
          <button
            type="button"
            onClick={() => setActiveView('analyzer')}
            className={`flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-left text-sm font-medium transition-colors ${
              activeView === 'analyzer'
                ? 'bg-slate-900 text-slate-50'
                : 'text-slate-400 hover:bg-slate-900/50 hover:text-slate-200'
            }`}
          >
            <SlidersHorizontal className="h-4 w-4 shrink-0" />
            <span className="whitespace-nowrap">Compose Analyzer</span>
          </button>

          <button
            type="button"
            onClick={() => setActiveView('repository')}
            className={`flex w-full items-center justify-between gap-2 rounded-md px-3 py-2 text-left text-sm font-medium transition-colors ${
              activeView === 'repository'
                ? 'bg-slate-900 text-slate-50'
                : 'text-slate-400 hover:bg-slate-900/50 hover:text-slate-200'
            }`}
          >
            <span className="flex items-center gap-2.5">
              <FolderGit2 className="h-4 w-4 shrink-0" />
              <span className="whitespace-nowrap">Rust Repository</span>
            </span>
            <span className="font-mono text-xs tabular-nums text-slate-500">
              {REPO_FILES.length}
            </span>
          </button>

          <button
            type="button"
            onClick={() => setActiveView('architecture')}
            className={`flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-left text-sm font-medium transition-colors ${
              activeView === 'architecture'
                ? 'bg-slate-900 text-slate-50'
                : 'text-slate-400 hover:bg-slate-900/50 hover:text-slate-200'
            }`}
          >
            <Layers className="h-4 w-4 shrink-0" />
            <span className="whitespace-nowrap">Architecture & Tests</span>
          </button>
        </nav>

        <div className="border-t border-slate-800 p-4 text-xs text-slate-500">
          <div className="font-medium text-slate-300">Quickstart (CLI)</div>
          <div className="mt-2 space-y-1 font-mono text-[11px] text-slate-400">
            <div>leandock analyze</div>
            <div>leandock up</div>
            <div>leandock proxy</div>
            <div>leandock revert</div>
          </div>
        </div>
      </aside>

      {/* Main Workspace Column */}
      <div className="flex min-w-0 flex-1 flex-col">
        {/* Top Bar Contract: Breadcrumb on left, 1 primary action on right */}
        <header className="sticky top-0 z-20 flex h-14 items-center justify-between gap-6 border-b border-slate-800 bg-slate-950/95 px-6 backdrop-blur">
          <div className="flex min-w-0 items-center gap-2 text-sm">
            <span className="font-semibold text-slate-100 lg:hidden whitespace-nowrap">
              LeanDock
            </span>
            <span className="text-slate-600 lg:hidden">/</span>
            <span className="hidden text-slate-500 lg:inline whitespace-nowrap">
              leandock
            </span>
            <span className="hidden text-slate-600 lg:inline">/</span>
            <span className="truncate font-medium text-slate-200">
              {breadcrumbLabel}
            </span>
          </div>

          <div className="flex items-center gap-3 shrink-0">
            <button
              type="button"
              onClick={handleDownloadZip}
              disabled={downloading}
              className="inline-flex items-center gap-2 whitespace-nowrap rounded-md bg-sky-500 px-4 py-2 text-xs font-semibold text-slate-950 transition-colors hover:bg-sky-400 disabled:opacity-60 shrink-0"
            >
              <Download className="h-3.5 w-3.5" />
              {downloading ? 'Packaging...' : 'Download Repository'}
            </button>
          </div>
        </header>

        {/* Mobile / Tablet Navigation Bar */}
        <div className="flex items-center gap-1 border-b border-slate-800 bg-slate-950 px-4 py-2 lg:hidden">
          {(
            [
              ['analyzer', 'Analyzer'],
              ['repository', `Repository (${REPO_FILES.length})`],
              ['architecture', 'Architecture'],
            ] as [ActiveView, string][]
          ).map(([id, label]) => (
            <button
              key={id}
              type="button"
              onClick={() => setActiveView(id)}
              className={`whitespace-nowrap rounded px-3 py-1.5 text-xs font-medium ${
                activeView === id
                  ? 'bg-slate-800 text-slate-50'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              {label}
            </button>
          ))}
        </div>

        {/* Main Viewport */}
        <main className="mx-auto w-full max-w-7xl flex-1 px-6 py-8">
          {activeView === 'analyzer' && <ComposeAnalyzer />}
          {activeView === 'repository' && (
            <RepoExplorer
              selectedPath={selectedFilePath}
              onSelectPath={setSelectedFilePath}
            />
          )}
          {activeView === 'architecture' && (
            <ArchitectureView onOpenFile={handleOpenRepoFile} />
          )}
        </main>
      </div>
    </div>
  );
}
