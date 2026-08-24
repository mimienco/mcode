#!/usr/bin/env node
// Why: PERF-03 stage B — keep the Settings shell and its section chunks inside
// measured byte budgets and forbid heavy foreign surfaces in their graphs.
import { readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'

const MANIFEST_PATH = 'out/renderer/.vite/manifest.json'
const SHELL_BUDGET_BYTES = 250_000
const SECTION_BUDGET_BYTES = 200_000
const FORBIDDEN_REACHABLE = [/TerminalPane/, /NativeChat/, /StatsPane/, /xterm/i]

export function checkRendererChunkBudgets({
  manifestPath = MANIFEST_PATH,
  rootDir = 'out/renderer',
  budgetOverrides = {},
  manifest
} = {}) {
  return checkManifest({
    manifest: manifest ?? JSON.parse(readFileSync(manifestPath, 'utf8')),
    sizeOf: (file) => {
      try {
        return statSync(join(rootDir, file)).size
      } catch {
        return 0
      }
    },
    shellBudget: budgetOverrides.shellBytes ?? SHELL_BUDGET_BYTES,
    sectionBudget: budgetOverrides.sectionBytes ?? SECTION_BUDGET_BYTES,
    forbiddenReachable: FORBIDDEN_REACHABLE
  })
}

export function checkManifest({
  manifest,
  sizeOf,
  shellBudget,
  sectionBudget,
  forbiddenReachable
}) {
  const violations = []
  const byFile = new Map(Object.values(manifest).map((chunk) => [chunk.file, chunk]))
  const settingsEntry = Object.values(manifest).find(
    (chunk) => chunk.name && /(^|\/)Settings-[^/]*\.js$/.test(chunk.file)
  )
  if (!settingsEntry) {
    return { ok: false, violations: ['Settings entry chunk not found in manifest'] }
  }

  // Why: BFS over static imports only — dynamic imports are the point of splitting.
  const reachable = new Set()
  const queue = [settingsEntry.file]
  while (queue.length > 0) {
    const file = queue.shift()
    if (!file || reachable.has(file)) {
      continue
    }
    reachable.add(file)
    const node = byFile.get(file)
    for (const imported of node?.imports ?? []) {
      queue.push(imported)
    }
  }

  const shellBytes = [...reachable]
    .filter((file) => file.endsWith('.js'))
    .reduce((total, file) => total + sizeOf(file), 0)
  if (shellBytes > shellBudget) {
    violations.push(`Settings shell graph is ${shellBytes} bytes (budget ${shellBudget}).`)
  }

  for (const pattern of forbiddenReachable) {
    const offenders = [...reachable].filter((file) => pattern.test(file))
    if (offenders.length > 0) {
      violations.push(
        `Forbidden chunk(s) statically reachable from Settings shell: ${offenders.join(', ')}`
      )
    }
  }

  const sectionChunks = Object.values(manifest).filter(
    (chunk) =>
      chunk.isDynamicEntry &&
      /GeneralPane/.test(chunk.file) &&
      !/(^|\/)Settings-[^/]*\.js$/.test(chunk.file)
  )
  for (const chunk of sectionChunks) {
    const seen = new Set()
    let bytes = 0
    const stack = [chunk.file]
    while (stack.length > 0) {
      const file = stack.pop()
      if (!file || seen.has(file)) {
        continue
      }
      seen.add(file)
      bytes += sizeOf(file)
      const node = byFile.get(file)
      for (const imported of node?.imports ?? []) {
        stack.push(imported)
      }
    }
    if (bytes > sectionBudget) {
      violations.push(
        `Section chunk ${chunk.file} is ${bytes} bytes incl. imports (budget ${sectionBudget}).`
      )
    }
  }

  return { ok: violations.length === 0, violations, shellBytes }
}

if (process.argv[1] && process.argv[1].endsWith('check-renderer-chunk-budgets.mjs')) {
  const result = checkRendererChunkBudgets()
  for (const violation of result.violations) {
    console.error(`renderer chunk budget violation: ${violation}`)
  }
  if (!result.ok) {
    process.exit(1)
  }
  console.log(`renderer chunk budgets OK (Settings shell graph: ${result.shellBytes} bytes)`)
}
