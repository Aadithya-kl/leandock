import JSZip from 'jszip';

export interface RepoFile {
  path: string;
  name: string;
  dir: string;
  ext: string;
  content: string;
  lines: number;
  bytes: number;
}

// Eagerly bundle every real file in /leandock/ (including hidden .github/ and .gitignore files)
const standardFiles = import.meta.glob('../../leandock/**/*', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const hiddenWorkflowFiles = import.meta.glob('../../leandock/.github/**/*', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const gitignoreFiles = import.meta.glob('../../leandock/.gitignore', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

function normalizePath(rawKey: string): string {
  return rawKey.replace(/^\.\.\/\.\.\/leandock\//, '');
}

const mergedRaw: Record<string, string> = {
  ...standardFiles,
  ...hiddenWorkflowFiles,
  ...gitignoreFiles,
};

export const REPO_FILES: RepoFile[] = Object.entries(mergedRaw)
  .map(([key, content]) => {
    const path = normalizePath(key);
    const segments = path.split('/');
    const name = segments[segments.length - 1] ?? path;
    const dir = segments.length > 1 ? segments.slice(0, -1).join('/') : '';
    const ext = name.includes('.') ? name.split('.').pop() ?? '' : '';
    return {
      path,
      name,
      dir,
      ext,
      content,
      lines: content.split('\n').length,
      bytes: new TextEncoder().encode(content).length,
    };
  })
  .sort((a, b) => a.path.localeCompare(b.path));

export function getRepoFile(path: string): RepoFile | undefined {
  return REPO_FILES.find((f) => f.path === path);
}

export async function downloadRepositoryZip(): Promise<void> {
  const zip = new JSZip();
  const root = zip.folder('leandock');
  if (!root) return;

  for (const file of REPO_FILES) {
    root.file(file.path, file.content);
  }

  const blob = await zip.generateAsync({ type: 'blob' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = 'leandock-v0.2.0.zip';
  document.body.appendChild(anchor);
  anchor.click();
  document.body.removeChild(anchor);
  URL.revokeObjectURL(url);
}
