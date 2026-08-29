import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, readFileSync } from 'node:fs'
import { resolve } from 'node:path'

const checks = [
  { name: 'HEAD', snapshot: 'HEAD.txt', gitArgs: ['rev-parse', 'HEAD'] },
  { name: 'branch', snapshot: 'branch.txt', gitArgs: ['branch', '--show-current'] },
  {
    name: 'unstaged diff',
    snapshot: 'diff-binary.patch',
    gitArgs: ['diff', '--binary'],
    fallbackGitArgs: ['diff']
  },
  {
    name: 'staged diff',
    snapshot: 'diff-cached-binary.patch',
    gitArgs: ['diff', '--cached', '--binary']
  },
  {
    name: 'working tree status',
    snapshot: 'status-full.txt',
    gitArgs: ['status', '--porcelain=v1', '-uall']
  }
]

function parseArgs(argv) {
  const options = { json: false }
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index]
    if (argument === '--json') {
      options.json = true
      continue
    }
    if (argument === '--source' || argument === '--snapshot') {
      const value = argv[index + 1]
      if (!value) {
        throw new Error(`${argument} requires a path`)
      }
      options[argument.slice(2)] = resolve(value)
      index += 1
      continue
    }
    throw new Error(`Unknown argument: ${argument}`)
  }
  if (!options.source || !options.snapshot) {
    throw new Error(
      'Usage: verify-original-integrity.mjs --source <path> --snapshot <path> [--json]'
    )
  }
  return options
}

function normalizeGitText(value) {
  return value
    .toString('utf8')
    .replace(/^\uFEFF/, '')
    .replace(/\r\n/g, '\n')
    .replace(/\n+$/, '')
}

function runGit(source, args) {
  const result = spawnSync('git', args, {
    cwd: source,
    encoding: null,
    maxBuffer: 64 * 1024 * 1024
  })
  if (result.status !== 0) {
    const detail = result.stderr?.toString('utf8').trim()
    throw new Error(`git ${args.join(' ')} failed${detail ? `: ${detail}` : ''}`)
  }
  return result.stdout
}

function verifyManifest(source, snapshot) {
  const entries = normalizeGitText(readFileSync(resolve(snapshot, 'manifest.sha256')))
    .split('\n')
    .filter(Boolean)
    .map((line) => {
      const match = /^([0-9a-f]{64}) ([* ])(.+)$/i.exec(line)
      if (!match) {
        throw new Error(`Invalid source manifest line: ${line}`)
      }
      return {
        expectedHash: match[1].toLowerCase(),
        expectedPresent: match[2] === '*',
        path: match[3]
      }
    })
  let checked = 0
  let expectedMissing = 0
  const unexpectedMissing = []
  const unexpectedPresent = []
  const hashMismatches = []
  for (const entry of entries) {
    const file = resolve(source, entry.path)
    const present = existsSync(file)
    if (!entry.expectedPresent) {
      if (present) {
        unexpectedPresent.push(entry.path)
      } else {
        expectedMissing += 1
      }
      continue
    }
    if (!present) {
      unexpectedMissing.push(entry.path)
      continue
    }
    const actualHash = createHash('sha256').update(readFileSync(file)).digest('hex')
    checked += 1
    if (actualHash !== entry.expectedHash) {
      hashMismatches.push(entry.path)
    }
  }
  return {
    name: 'source manifest',
    unchanged:
      unexpectedMissing.length === 0 &&
      unexpectedPresent.length === 0 &&
      hashMismatches.length === 0,
    snapshotFile: 'manifest.sha256',
    entries: entries.length,
    checked,
    expectedMissing,
    unexpectedMissing,
    unexpectedPresent,
    hashMismatches
  }
}

export function verifyOriginalIntegrity({ source, snapshot }) {
  const results = checks.map((check) => {
    const expected = normalizeGitText(readFileSync(resolve(snapshot, check.snapshot)))
    const actual = normalizeGitText(runGit(source, check.gitArgs))
    const fallbackMatched =
      actual !== expected &&
      check.fallbackGitArgs &&
      normalizeGitText(runGit(source, check.fallbackGitArgs)) === expected
    return {
      name: check.name,
      unchanged: actual === expected || Boolean(fallbackMatched),
      snapshotFile: check.snapshot,
      ...(fallbackMatched ? { comparison: `git ${check.fallbackGitArgs.join(' ')}` } : {})
    }
  })
  results.push(verifyManifest(source, snapshot))
  return { source, snapshot, unchanged: results.every((check) => check.unchanged), checks: results }
}

function main() {
  try {
    const options = parseArgs(process.argv.slice(2))
    const report = verifyOriginalIntegrity(options)
    if (options.json) {
      process.stdout.write(`${JSON.stringify(report, null, 2)}\n`)
    } else {
      for (const check of report.checks) {
        process.stdout.write(`${check.unchanged ? 'PASS' : 'FAIL'} ${check.name}\n`)
      }
      process.stdout.write(`Original source is ${report.unchanged ? 'unchanged' : 'different'}\n`)
    }
    process.exitCode = report.unchanged ? 0 : 1
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`)
    process.exitCode = 2
  }
}

if (process.argv[1] && resolve(import.meta.filename) === resolve(process.argv[1])) {
  main()
}
