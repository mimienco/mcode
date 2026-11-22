import { useCallback, useEffect, useSyncExternalStore } from 'react'

const maximizedGroupIdByWorktree = new Map<string, string>()
const listenersByWorktree = new Map<string, Set<() => void>>()

export function getMaximizedTabGroupId(worktreeId: string): string | null {
  return maximizedGroupIdByWorktree.get(worktreeId) ?? null
}

export function setMaximizedTabGroupId(worktreeId: string, groupId: string | null): void {
  const previous = getMaximizedTabGroupId(worktreeId)
  if (previous === groupId) {
    return
  }
  if (groupId === null) {
    maximizedGroupIdByWorktree.delete(worktreeId)
  } else {
    maximizedGroupIdByWorktree.set(worktreeId, groupId)
  }
  for (const listener of listenersByWorktree.get(worktreeId) ?? []) {
    listener()
  }
}

function subscribeToMaximizedTabGroup(worktreeId: string, listener: () => void): () => void {
  const listeners = listenersByWorktree.get(worktreeId) ?? new Set<() => void>()
  listeners.add(listener)
  listenersByWorktree.set(worktreeId, listeners)
  return () => {
    listeners.delete(listener)
    if (listeners.size === 0) {
      listenersByWorktree.delete(worktreeId)
    }
  }
}

export function useMaximizedTabGroupId(worktreeId: string): string | null {
  const subscribe = useCallback(
    (listener: () => void) => subscribeToMaximizedTabGroup(worktreeId, listener),
    [worktreeId]
  )
  const getSnapshot = useCallback(() => getMaximizedTabGroupId(worktreeId), [worktreeId])
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot)
}

export function useValidatedMaximizedTabGroupId(
  worktreeId: string,
  isValidGroupId: (groupId: string) => boolean
): string | null {
  const maximizedGroupId = useMaximizedTabGroupId(worktreeId)
  const isValid = maximizedGroupId !== null && isValidGroupId(maximizedGroupId)
  useEffect(() => {
    if (maximizedGroupId !== null && !isValid) {
      setMaximizedTabGroupId(worktreeId, null)
    }
  }, [isValid, maximizedGroupId, worktreeId])
  return isValid ? maximizedGroupId : null
}

export function resetMaximizedTabGroupsForTests(): void {
  maximizedGroupIdByWorktree.clear()
  listenersByWorktree.clear()
}
