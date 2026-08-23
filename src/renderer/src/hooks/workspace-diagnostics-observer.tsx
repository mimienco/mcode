import { useEffect } from 'react'

import {
  MCODE_TERMINAL_COMMAND_FINISHED_EVENT,
  type TerminalCommandFinishedEventDetail
} from '@/hooks/terminal-command-finished-event'
import { subscribeTerminalOutputObserver } from '@/lib/terminal-output-observers'
import { getExecutionHostIdForWorktree } from '@/lib/worktree-runtime-owner'
import { useAppStore } from '@/store'
import {
  captureWorkspaceTerminalOutput,
  completeWorkspaceDiagnosticsCommand,
  pruneWorkspaceDiagnostics,
  workspaceDiagnosticsIdentity
} from '@/store/workspace-diagnostics'

type TerminalOwnershipState = {
  tabsByWorktree: Record<string, readonly { id: string; ptyId?: string | null }[]>
  ptyIdsByTabId: Record<string, readonly string[] | undefined>
  terminalLayoutsByTabId: Record<
    string,
    { ptyIdsByLeafId?: Record<string, string | null | undefined> } | undefined
  >
}

type TerminalOwnershipCache = {
  ptyIdsByTabId: TerminalOwnershipState['ptyIdsByTabId']
  tabsByWorktree: TerminalOwnershipState['tabsByWorktree']
  terminalLayoutsByTabId: TerminalOwnershipState['terminalLayoutsByTabId']
  worktreeIdByPtyId: Map<string, string>
}

let ownershipCache: TerminalOwnershipCache | null = null

function buildTerminalOwnershipIndex(state: TerminalOwnershipState): Map<string, string> {
  const worktreeIdByPtyId = new Map<string, string>()
  for (const [worktreeId, tabs] of Object.entries(state.tabsByWorktree)) {
    for (const tab of tabs) {
      if (tab.ptyId) {
        worktreeIdByPtyId.set(tab.ptyId, worktreeId)
      }
      for (const ptyId of state.ptyIdsByTabId[tab.id] ?? []) {
        worktreeIdByPtyId.set(ptyId, worktreeId)
      }
      for (const ptyId of Object.values(
        state.terminalLayoutsByTabId[tab.id]?.ptyIdsByLeafId ?? {}
      )) {
        if (ptyId) {
          worktreeIdByPtyId.set(ptyId, worktreeId)
        }
      }
    }
  }
  return worktreeIdByPtyId
}

function findCachedWorktreeIdForPty(state: TerminalOwnershipState, ptyId: string): string | null {
  if (
    !ownershipCache ||
    ownershipCache.ptyIdsByTabId !== state.ptyIdsByTabId ||
    ownershipCache.tabsByWorktree !== state.tabsByWorktree ||
    ownershipCache.terminalLayoutsByTabId !== state.terminalLayoutsByTabId
  ) {
    ownershipCache = {
      ptyIdsByTabId: state.ptyIdsByTabId,
      tabsByWorktree: state.tabsByWorktree,
      terminalLayoutsByTabId: state.terminalLayoutsByTabId,
      worktreeIdByPtyId: buildTerminalOwnershipIndex(state)
    }
  }
  return ownershipCache.worktreeIdByPtyId.get(ptyId) ?? null
}

export function findWorktreeIdForPty(state: TerminalOwnershipState, ptyId: string): string | null {
  return buildTerminalOwnershipIndex(state).get(ptyId) ?? null
}

export function WorkspaceDiagnosticsObserver(): null {
  useEffect(() => {
    const unsubscribeOutput = subscribeTerminalOutputObserver(({ ptyId, data }) => {
      const worktreeId = findCachedWorktreeIdForPty(useAppStore.getState(), ptyId)
      if (worktreeId) {
        const state = useAppStore.getState()
        captureWorkspaceTerminalOutput({
          ptyId,
          worktreeId,
          hostId: getExecutionHostIdForWorktree(state, worktreeId),
          data
        })
      }
    })
    const handleCommandFinished = (event: Event): void => {
      const detail = (event as CustomEvent<TerminalCommandFinishedEventDetail>).detail
      if (detail?.worktreeId && detail.ptyId) {
        const state = useAppStore.getState()
        completeWorkspaceDiagnosticsCommand({
          ptyId: detail.ptyId,
          worktreeId: detail.worktreeId,
          hostId: getExecutionHostIdForWorktree(state, detail.worktreeId),
          ...(detail.cwd ? { cwd: detail.cwd } : {}),
          exitCode: detail.exitCode
        })
      }
    }
    let priorWorktreesByRepo = useAppStore.getState().worktreesByRepo
    const unsubscribeWorktrees = useAppStore.subscribe((state) => {
      if (state.worktreesByRepo === priorWorktreesByRepo) {
        return
      }
      priorWorktreesByRepo = state.worktreesByRepo
      const validIdentities = new Set(
        state
          .allWorktrees()
          .map((worktree) =>
            workspaceDiagnosticsIdentity(
              worktree.hostId ?? getExecutionHostIdForWorktree(state, worktree.id),
              worktree.id
            )
          )
      )
      pruneWorkspaceDiagnostics(validIdentities)
    })
    window.addEventListener(MCODE_TERMINAL_COMMAND_FINISHED_EVENT, handleCommandFinished)
    return () => {
      unsubscribeOutput()
      unsubscribeWorktrees()
      window.removeEventListener(MCODE_TERMINAL_COMMAND_FINISHED_EVENT, handleCommandFinished)
    }
  }, [])

  return null
}
