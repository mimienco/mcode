import { readFileSync, writeFileSync } from 'node:fs'
import { win32 as pathWin32 } from 'node:path'
import type { HydrationResult } from './hydrate-shell-path'

// Why: the login-shell probe (PowerShell with full user profile) can take seconds on
// every launch, and agent/chat session creation awaits it — so the last good result is
// persisted to userData and served instantly on the next launch, with a live re-probe
// in the background so CLIs installed since then still get picked up.
// Why no electron import here: hydrate-shell-path is also bundled into plain-Node
// processes (managed-agent-hook-controls) where require('electron') throws; index.ts
// wires the userData path explicitly via configureShellPathCacheFile.

type PersistedShellPathHydration = { v: 1; shell: string; segments: string[] }

let overrideCacheFile: string | null = null

/** Set the persistence location (main process startup; tests use it too). */
export function configureShellPathCacheFile(path: string | null): void {
  overrideCacheFile = path
}

export function resetShellPathPersistenceForTests(): void {
  overrideCacheFile = null
}

function resolveCacheFile(): string | null {
  return overrideCacheFile
}

/** Last launch's PATH segments for this shell, or null when absent/foreign/unreadable. */
export function readPersistedHydration(shell: string | null): HydrationResult | null {
  const file = resolveCacheFile()
  if (!file || !shell) {
    return null
  }
  try {
    const parsed = JSON.parse(readFileSync(file, 'utf-8')) as Partial<PersistedShellPathHydration>
    // A different configured shell exports a different PATH — don't serve it.
    if (parsed?.v !== 1 || parsed.shell !== pathWin32.basename(shell).toLowerCase()) {
      return null
    }
    if (!Array.isArray(parsed.segments) || parsed.segments.length === 0) {
      return null
    }
    return { ok: true, segments: parsed.segments, failureReason: 'none' }
  } catch {
    return null
  }
}

export function persistHydration(shell: string, segments: string[]): void {
  const file = resolveCacheFile()
  if (!file) {
    return
  }
  try {
    const payload: PersistedShellPathHydration = {
      v: 1,
      shell: pathWin32.basename(shell).toLowerCase(),
      segments
    }
    writeFileSync(file, JSON.stringify(payload), 'utf-8')
  } catch {
    // Best-effort cache; a failed write just means the next launch probes live.
  }
}
