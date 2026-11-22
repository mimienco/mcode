import { homedir } from 'node:os'
import { isAbsolute, join, resolve } from 'node:path'
import { isPathInsideOrEqual } from '../../shared/cross-platform-path'
import type { AiVaultAgent } from '../../shared/ai-vault-types'
import {
  getAiVaultAdditionalCodexSessionsDirs,
  getAiVaultWslHomeDirs
} from './cached-session-list'
import {
  resolveOpenCodeDataDirectory,
  resolveOpenCodeStorageDirectory
} from '../opencode/opencode-data-directory'
import { AI_VAULT_AGENT_SOURCES } from './session-scanner-agent-sources'
import { splitOpenCodeSqliteCandidate } from './session-scanner-opencode-sqlite-paths'

// Why: renderer-supplied transcript paths reach filesystem reads through several
// IPC entries — including OpenCode's SQLite open — so every one gates against
// the same WSL-aware roots its agent's scanner discovers under. Root derivation
// here must stay aligned with the scanners (agent-sources table, opencode data
// dirs + OPENCODE_DB override, antigravity brain dirs).
export async function isTranscriptPathInsideAgentRoots(args: {
  agent: AiVaultAgent
  filePath: string
}): Promise<boolean> {
  const wslHomeDirs = await getAiVaultWslHomeDirs()
  return isPathInsideAgentRoots({
    agent: args.agent,
    filePath: args.filePath,
    wslHomeDirs
  })
}

export function isPathInsideAgentRoots(args: {
  agent: AiVaultAgent
  filePath: string
  wslHomeDirs: readonly string[]
}): boolean {
  // Why: an OpenCode SQLite candidate (`<dbPath>#<sessionId>`) opens dbPath, so
  // containment is judged against the database path, not the synthetic string.
  const sqlite = args.agent === 'opencode' ? splitOpenCodeSqliteCandidate(args.filePath) : null
  const target = (sqlite ? sqlite.dbPath : args.filePath).trim()
  if (!target) {
    return false
  }
  // resolve() collapses `..` segments first — isPathInsideOrEqual compares
  // textually and would otherwise pass `<root>/../../etc/x.jsonl`.
  const resolvedTarget = resolve(target)
  const roots = agentTranscriptRootDirs(args.agent, args.wslHomeDirs)
  return roots.some((root) => isPathInsideOrEqual(root, resolvedTarget))
}

// The root set whose descendants a path-read for `agent` may touch. Agents with
// no known roots (neither table nor shape-specific scanner) allow nothing.
export function agentTranscriptRootDirs(
  agent: AiVaultAgent,
  wslHomeDirs: readonly string[]
): string[] {
  const source = AI_VAULT_AGENT_SOURCES[agent]
  if (source) {
    return source.rootDirs({ additionalCodexSessionsDirs: getAiVaultAdditionalCodexSessionsDirs() }, wslHomeDirs)
      .filter((rootDir) => rootDir.trim().length > 0)
      .map((rootDir) => resolve(rootDir))
  }
  if (agent === 'opencode') {
    return opencodeTranscriptRootDirs(wslHomeDirs)
  }
  if (agent === 'antigravity') {
    return antigravityTranscriptRootDirs(wslHomeDirs)
  }
  return []
}

function opencodeTranscriptRootDirs(wslHomeDirs: readonly string[]): string[] {
  const dataDir = resolveOpenCodeDataDirectory()
  const roots = [
    dataDir,
    resolveOpenCodeStorageDirectory(),
    ...wslHomeDirs.flatMap((homeDir) => [
      join(homeDir, '.local', 'share', 'opencode'),
      join(homeDir, '.local', 'share', 'opencode', 'storage')
    ])
  ]
  const override = process.env.OPENCODE_DB?.trim()
  // Why: mirror the scanner's OPENCODE_DB resolution so an exact out-of-root
  // database stays readable instead of silently dropping its sessions.
  if (override) {
    roots.push(isAbsolute(override) ? override : join(dataDir, override))
  }
  return roots.map((rootDir) => resolve(rootDir))
}

function antigravityTranscriptRootDirs(wslHomeDirs: readonly string[]): string[] {
  return [
    join(homedir(), '.gemini', 'antigravity-cli', 'brain'),
    ...wslHomeDirs.map((homeDir) => join(homeDir, '.gemini', 'antigravity-cli', 'brain'))
  ].map((rootDir) => resolve(rootDir))
}
