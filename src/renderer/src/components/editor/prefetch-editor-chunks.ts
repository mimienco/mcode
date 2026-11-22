// Why: EditorPanel and its Monaco/TipTap children are React.lazy chunks that
// were only fetched at click time. The post-hydration warm queue consumes
// these one at a time so first file-open is fast without a concurrent parse
// burst that stalls unrelated input.
export const EDITOR_CHUNK_IMPORTERS: readonly (() => Promise<unknown>)[] = [
  () => import('./EditorPanel'),
  () => import('./MonacoEditor'),
  () => import('./RichMarkdownEditor'),
  () => import('./MarkdownPreview')
]
