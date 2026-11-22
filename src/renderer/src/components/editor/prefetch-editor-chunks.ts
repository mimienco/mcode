// Why: EditorPanel and its Monaco/TipTap children are React.lazy chunks that
// were only fetched at click time, so the first file open showed
// "Loading editor…" through a three-stage waterfall (panel chunk → editor
// chunk → file read). Warming them at idle makes the first open feel native.
export const EDITOR_CHUNK_IMPORTERS: readonly (() => Promise<unknown>)[] = [
  () => import('./EditorPanel'),
  () => import('./MonacoEditor'),
  () => import('./RichMarkdownEditor'),
  () => import('./MarkdownPreview')
]

const warm = (): void => {
  for (const load of EDITOR_CHUNK_IMPORTERS) {
    void load()
  }
}

export function prefetchEditorChunks(): void {
  if (typeof window.requestIdleCallback === 'function') {
    window.requestIdleCallback(warm, { timeout: 3000 })
  } else {
    setTimeout(warm, 1500)
  }
}
