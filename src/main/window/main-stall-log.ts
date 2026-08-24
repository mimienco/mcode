import { appendFileSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { getLogsDirectory } from '../observability/logs-directory'

// Why a plain file: the NDJSON trace sink only flushes on durable breadcrumbs,
// and the developer's terminal stdout is not machine-readable after the fact.
// Stalls are rare, so a tiny append-only log next to the trace file gives any
// later diagnosis (human or agent) the exact durations without anyone pasting
// console output.

let ensuredDirectory = false

export function appendStallRecord(
  source: string,
  fields: Record<string, unknown>,
  options?: { filePath?: string }
): void {
  try {
    const filePath = options?.filePath ?? join(getLogsDirectory(), 'main-stall.log')
    if (!options?.filePath && !ensuredDirectory) {
      mkdirSync(getLogsDirectory(), { recursive: true })
      ensuredDirectory = true
    }
    appendFileSync(filePath, `${new Date().toISOString()} ${source} ${JSON.stringify(fields)}\n`)
  } catch {
    // Never let diagnostics I/O take the app down.
  }
}
