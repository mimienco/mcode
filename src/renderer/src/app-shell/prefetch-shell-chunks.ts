import { scheduleAfterInputQuiet } from '../lib/input-quiet-scheduler'
import { EDITOR_CHUNK_IMPORTERS } from '../components/editor/prefetch-editor-chunks'
import { APP_PAGE_IMPORTERS } from './app-page-chunks'

type ChunkImporter = () => Promise<unknown>
type QuietScheduler = typeof scheduleAfterInputQuiet

const SHELL_CHUNK_IMPORTERS: readonly ChunkImporter[] = [
  // Settings is the highest-frequency whole-page destination and must not sit
  // behind secondary tool panels in the idle queue.
  APP_PAGE_IMPORTERS.settings,
  () => import('../components/QuickOpen'),
  ...EDITOR_CHUNK_IMPORTERS,
  () => import('../components/right-sidebar/FileExplorer'),
  () => import('../components/WorktreeJumpPalette'),
  () => import('../components/right-sidebar/AiVaultPanel'),
  () => import('../components/right-sidebar/WorkspaceProblemsPanel'),
  () => import('../components/right-sidebar/PortsPanel'),
  () => import('../components/right-sidebar/ChecksPanel'),
  () => import('../components/right-sidebar/FolderWorkspaceWorktreesPanel'),
  () => import('../components/right-sidebar/FolderWorkspacePrChecksPanel'),
  APP_PAGE_IMPORTERS.artifacts,
  APP_PAGE_IMPORTERS.skills
  // Why: Source Control and Tasks are 610–720 KiB entry chunks, while
  // Automations has its own targeted route/editor intent warming. Evaluating
  // them speculatively can create a long task after startup.
]

export function scheduleShellChunkWarmQueue(
  importers: readonly ChunkImporter[],
  schedule: QuietScheduler = scheduleAfterInputQuiet
): () => void {
  let cancelled = false
  let cancelScheduled: (() => void) | null = null

  const scheduleNext = (index: number): void => {
    if (cancelled || index >= importers.length) {
      return
    }
    cancelScheduled = schedule(
      () => {
        cancelScheduled = null
        if (cancelled) {
          return
        }
        void importers[index]()
          .catch(() => {})
          .finally(() => scheduleNext(index + 1))
      },
      {
        delayMs: index === 0 ? 1000 : 250,
        quietMs: 750,
        idleTimeoutMs: 4000
      }
    )
  }

  scheduleNext(0)
  return () => {
    cancelled = true
    cancelScheduled?.()
    cancelScheduled = null
  }
}

export function prefetchShellChunks(): () => void {
  return scheduleShellChunkWarmQueue(SHELL_CHUNK_IMPORTERS)
}
