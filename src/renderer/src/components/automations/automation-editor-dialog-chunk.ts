export const importAutomationEditorDialog = () =>
  import('./AutomationEditorDialog').then(({ AutomationEditorDialog }) => ({
    default: AutomationEditorDialog
  }))

export function createAutomationEditorDialogPrefetcher<T>(
  importer: () => Promise<T>
): () => Promise<T> {
  let request: Promise<T> | null = null

  return () => {
    if (request) {
      return request
    }

    request = importer().catch((error: unknown) => {
      // Why: updates can briefly race a chunk read. Let the next real intent retry
      // instead of permanently poisoning the session's editor-dialog prefetch.
      request = null
      throw error
    })
    return request
  }
}

export const prefetchAutomationEditorDialog = createAutomationEditorDialogPrefetcher(
  importAutomationEditorDialog
)
