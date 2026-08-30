import { parse } from '@babel/parser'
import traverseModule from '@babel/traverse'
import { format } from 'oxfmt'
import { execFileSync } from 'node:child_process'
import { isBuiltin } from 'node:module'
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, extname, relative, resolve, sep } from 'node:path'

const traverse = traverseModule.default ?? traverseModule
const SOURCE_EXTENSIONS = ['.ts', '.tsx', '.mts', '.cts', '.js', '.jsx', '.mjs', '.cjs']
const EXCLUDED_DIRECTORIES = new Set(['__fixtures__', '__mocks__', '__snapshots__', '__tests__'])
const TEST_FILE_PATTERN = /\.(?:bench|spec|test)\.[^.]+$/i
const DEFAULT_ENTRY_POINTS = [
  'src/renderer/src/main.tsx',
  'src/renderer/src/popout.tsx',
  'src/renderer/src/web/main.tsx'
]

function normalizePath(path) {
  return path.split(sep).join('/')
}

function parseArgs(argv) {
  const options = { check: false }
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index]
    if (argument === '--check') {
      options.check = true
      continue
    }
    if (argument === '--source' || argument === '--output' || argument === '--allowlist') {
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
  if (!options.source || !options.output || !options.allowlist) {
    throw new Error(
      'Usage: generate-renderer-import-inventory.mjs --source <path> --output <path> --allowlist <path> [--check]'
    )
  }
  return options
}

function parserPlugins(path) {
  const extension = extname(path).toLowerCase()
  const plugins = ['decorators-legacy', 'explicitResourceManagement', 'importAttributes']
  if (['.ts', '.tsx', '.mts', '.cts'].includes(extension)) {
    plugins.push('typescript')
  }
  if (['.tsx', '.jsx'].includes(extension)) {
    plugins.push('jsx')
  }
  return plugins
}

function resolveLocalModule(sourceRoot, fromFile, specifier) {
  let base
  if (specifier.startsWith('@/')) {
    base = resolve(sourceRoot, 'src/renderer/src', specifier.slice(2))
  } else if (specifier.startsWith('@renderer/')) {
    base = resolve(sourceRoot, 'src/renderer/src', specifier.slice('@renderer/'.length))
  } else if (specifier.startsWith('.')) {
    base = resolve(dirname(fromFile), specifier)
  } else {
    return undefined
  }
  const candidates = [
    ...(SOURCE_EXTENSIONS.includes(extname(base).toLowerCase()) ? [base] : []),
    ...SOURCE_EXTENSIONS.map((extension) => `${base}${extension}`),
    ...SOURCE_EXTENSIONS.map((extension) => resolve(base, `index${extension}`))
  ]
  return candidates.find((candidate) => existsSync(candidate))
}

function isLocalSpecifier(specifier) {
  return (
    specifier.startsWith('.') || specifier.startsWith('@/') || specifier.startsWith('@renderer/')
  )
}

function isLocalSourceSpecifier(specifier) {
  const extension = extname(specifier.split(/[?#]/, 1)[0]).toLowerCase()
  return extension === '' || SOURCE_EXTENSIONS.includes(extension)
}

function packageName(specifier) {
  const segments = specifier.split('/')
  return specifier.startsWith('@') ? segments.slice(0, 2).join('/') : segments[0]
}

function summarizeExternalPackages(imports) {
  const packages = new Map()
  for (const record of imports) {
    if (isLocalSpecifier(record.specifier) || isForbiddenRendererImport(record.specifier)) {
      continue
    }
    const name = packageName(record.specifier)
    const entry = packages.get(name) ?? {
      package: name,
      specifiers: new Set(),
      importingFiles: new Set()
    }
    entry.specifiers.add(record.specifier)
    entry.importingFiles.add(record.file)
    packages.set(name, entry)
  }
  return [...packages.values()]
    .sort((left, right) => left.package.localeCompare(right.package))
    .map((entry) => ({
      package: entry.package,
      specifiers: [...entry.specifiers].sort(),
      importingFiles: [...entry.importingFiles].sort()
    }))
}

function importRecords(sourceRoot, file) {
  const text = readFileSync(file, 'utf8')
  const ast = parse(text, {
    sourceType: 'unambiguous',
    errorRecovery: true,
    plugins: parserPlugins(file)
  })
  if (ast.errors.length > 0) {
    const messages = ast.errors.map((error) => error.message).join('; ')
    throw new Error(
      `Renderer source parse recovery error in ${normalizePath(relative(sourceRoot, file))}: ${messages}`
    )
  }
  const records = []
  const addImportRecord = (path, specifier, importKind = 'value') => {
    const location = path.node.loc?.start
    records.push({
      file: normalizePath(relative(sourceRoot, file)),
      line: location?.line ?? 0,
      column: (location?.column ?? 0) + 1,
      specifier,
      importKind
    })
  }
  const addUnresolvedDynamic = (path) => {
    const location = path.node.loc?.start
    records.push({
      file: normalizePath(relative(sourceRoot, file)),
      line: location?.line ?? 0,
      column: (location?.column ?? 0) + 1,
      specifier: '<dynamic>',
      importKind: 'value',
      unresolvedDynamic: true,
      expression: text.slice(path.node.start, path.node.end)
    })
  }
  traverse(ast, {
    ImportDeclaration(path) {
      const typeOnly =
        path.node.importKind === 'type' ||
        (path.node.specifiers.length > 0 &&
          path.node.specifiers.every((specifier) => specifier.importKind === 'type'))
      addImportRecord(path, path.node.source.value, typeOnly ? 'type' : 'value')
    },
    ExportNamedDeclaration(path) {
      if (path.node.source) {
        addImportRecord(
          path,
          path.node.source.value,
          path.node.exportKind === 'type' ? 'type' : 'value'
        )
      }
    },
    ExportAllDeclaration(path) {
      addImportRecord(
        path,
        path.node.source.value,
        path.node.exportKind === 'type' ? 'type' : 'value'
      )
    },
    TSImportEqualsDeclaration(path) {
      const reference = path.node.moduleReference
      if (
        reference.type === 'TSExternalModuleReference' &&
        reference.expression.type === 'StringLiteral'
      ) {
        addImportRecord(
          path,
          reference.expression.value,
          path.node.importKind === 'type' ? 'type' : 'value'
        )
      }
    },
    CallExpression(path) {
      const argument = path.node.arguments[0]
      const moduleLoader =
        path.node.callee.type === 'Import' ||
        (path.node.callee.type === 'Identifier' && path.node.callee.name === 'require')
      if (moduleLoader) {
        const staticSpecifier =
          argument?.type === 'StringLiteral'
            ? argument.value
            : argument?.type === 'TemplateLiteral' && argument.expressions.length === 0
              ? argument.quasis[0]?.value.cooked
              : undefined
        if (staticSpecifier === undefined) {
          addUnresolvedDynamic(path)
          return
        }
        addImportRecord(path, staticSpecifier)
      }
    }
  })
  return records
}

function builtinName(specifier) {
  if (!isBuiltin(specifier)) {
    return undefined
  }
  return specifier.startsWith('node:') ? specifier : `node:${specifier}`
}

function isForbiddenRendererImport(specifier) {
  return (
    Boolean(builtinName(specifier)) || specifier === 'electron' || specifier.startsWith('electron/')
  )
}

function collectRendererSourceFiles(sourceRoot) {
  const files = []
  const visit = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      if (entry.isDirectory() && EXCLUDED_DIRECTORIES.has(entry.name)) {
        continue
      }
      const path = resolve(directory, entry.name)
      if (entry.isDirectory()) {
        visit(path)
      } else if (
        entry.isFile() &&
        SOURCE_EXTENSIONS.includes(extname(entry.name).toLowerCase()) &&
        !TEST_FILE_PATTERN.test(entry.name)
      ) {
        files.push(path)
      }
    }
  }
  visit(resolve(sourceRoot, 'src/renderer/src'))
  return files.sort((left, right) => normalizePath(left).localeCompare(normalizePath(right)))
}

function allowlistKey(entry) {
  return JSON.stringify([entry.file, entry.specifier, entry.importKind])
}

function validateAllowlist(allowlist) {
  if (allowlist?.schemaVersion !== 1 || !Array.isArray(allowlist.entries)) {
    throw new Error('Renderer import allowlist must use schemaVersion 1 with an entries array')
  }
  const entries = new Map()
  for (const entry of allowlist.entries) {
    if (
      typeof entry?.file !== 'string' ||
      typeof entry.specifier !== 'string' ||
      !['type', 'value'].includes(entry.importKind) ||
      typeof entry.rationale !== 'string' ||
      entry.rationale.trim().length === 0
    ) {
      throw new Error(
        'Every renderer import allowlist entry requires file, specifier, importKind, and rationale'
      )
    }
    const key = allowlistKey(entry)
    if (entries.has(key)) {
      throw new Error(
        `Duplicate renderer import allowlist entry: ${entry.file} -> ${entry.specifier}`
      )
    }
    entries.set(key, entry)
  }
  return entries
}

function readGitIdentity(sourceRoot) {
  const git = (args) =>
    execFileSync('git', args, {
      cwd: sourceRoot,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore']
    }).trim()
  try {
    return { head: git(['rev-parse', 'HEAD']), branch: git(['branch', '--show-current']) }
  } catch {
    return { head: null, branch: null }
  }
}

export function generateRendererImportInventory(sourceRoot, options) {
  const root = resolve(sourceRoot)
  const allowlist = validateAllowlist(options.allowlist)
  const entryPoints = options.entryPoints ?? DEFAULT_ENTRY_POINTS
  const pending = entryPoints.map((entry) => resolve(root, entry))
  const visited = new Set()
  const imports = []

  while (pending.length > 0) {
    const file = pending.pop()
    if (!file || visited.has(file)) {
      continue
    }
    if (!existsSync(file)) {
      throw new Error(
        `Renderer entry or dependency does not exist: ${normalizePath(relative(root, file))}`
      )
    }
    visited.add(file)
    for (const record of importRecords(root, file)) {
      imports.push(record)
      if (record.unresolvedDynamic) {
        throw new Error(
          `Unresolved dynamic renderer import at ${record.file}:${record.line}: ${record.expression}`
        )
      }
      if (isLocalSpecifier(record.specifier)) {
        const dependency = resolveLocalModule(root, file, record.specifier)
        if (!dependency && isLocalSourceSpecifier(record.specifier)) {
          throw new Error(
            `Unresolved local renderer import at ${record.file}:${record.line}: ${record.specifier}`
          )
        }
        if (dependency && record.importKind === 'value') {
          pending.push(dependency)
        }
      }
    }
  }

  const forbidden = imports.find((record) => isForbiddenRendererImport(record.specifier))
  if (forbidden) {
    const specifier = builtinName(forbidden.specifier) ?? forbidden.specifier
    throw new Error(
      `${forbidden.file}:${forbidden.line} reachable production renderer imports ${specifier}`
    )
  }

  const allowlistedImports = []
  const unallowlistedImports = []
  const matchedAllowlistKeys = new Set()
  const rendererSourceFiles = collectRendererSourceFiles(root)
  for (const file of rendererSourceFiles) {
    if (visited.has(file)) {
      continue
    }
    for (const record of importRecords(root, file).filter((entry) =>
      isForbiddenRendererImport(entry.specifier)
    )) {
      const key = allowlistKey(record)
      const allowed = allowlist.get(key)
      if (!allowed) {
        unallowlistedImports.push(record)
        continue
      }
      matchedAllowlistKeys.add(key)
      allowlistedImports.push({ ...record, rationale: allowed.rationale })
    }
  }
  if (unallowlistedImports.length > 0) {
    const details = unallowlistedImports
      .map(
        (entry) =>
          `${entry.file}:${entry.line} unallowlisted renderer source imports ${entry.specifier}`
      )
      .join('\n')
    throw new Error(`Unallowlisted renderer imports (${unallowlistedImports.length}):\n${details}`)
  }
  const stale = [...allowlist.entries()].find(([key]) => !matchedAllowlistKeys.has(key))?.[1]
  if (stale) {
    throw new Error(`Stale renderer import allowlist entry: ${stale.file} -> ${stale.specifier}`)
  }

  imports.sort(
    (left, right) =>
      left.file.localeCompare(right.file) ||
      left.line - right.line ||
      left.column - right.column ||
      left.specifier.localeCompare(right.specifier)
  )
  allowlistedImports.sort(
    (left, right) =>
      left.file.localeCompare(right.file) || left.line - right.line || left.column - right.column
  )
  const externalPackages = summarizeExternalPackages(imports)

  return {
    schemaVersion: 1,
    source: readGitIdentity(root),
    scan: {
      entryPoints: [...entryPoints],
      rendererSourceDirectory: 'src/renderer/src',
      excludedFilePattern: TEST_FILE_PATTERN.source
    },
    summary: {
      rendererSourceFiles: rendererSourceFiles.length,
      reachableSourceFiles: visited.size,
      reachableImportSites: imports.length,
      externalPackages: externalPackages.length,
      allowlistedUnreachableImports: allowlistedImports.length,
      forbiddenReachableImports: 0,
      unresolvedDynamicImports: 0
    },
    entryPoints: [...entryPoints],
    reachableFiles: [...visited].map((file) => normalizePath(relative(root, file))).sort(),
    allowlistedImports,
    externalPackages
  }
}

async function serialize(report) {
  const unformatted = `${JSON.stringify(report, null, 2)}\n`
  const formatterConfig = JSON.parse(
    readFileSync(resolve(import.meta.dirname, '..', '..', '.oxfmtrc.json'), 'utf8')
  )
  const formatted = await format('renderer-import-inventory.json', unformatted, formatterConfig)
  if (formatted.errors.length > 0) {
    throw new Error(
      `Failed to format renderer import inventory: ${formatted.errors.map((error) => error.message).join('; ')}`
    )
  }
  return formatted.code
}

async function main() {
  try {
    const options = parseArgs(process.argv.slice(2))
    const allowlist = JSON.parse(readFileSync(options.allowlist, 'utf8'))
    const actual = await serialize(generateRendererImportInventory(options.source, { allowlist }))
    if (options.check) {
      const expected = existsSync(options.output) ? readFileSync(options.output, 'utf8') : ''
      if (expected !== actual) {
        process.stderr.write(`Renderer import inventory is stale: ${options.output}\n`)
        process.exitCode = 1
      } else {
        process.stdout.write(`Renderer import inventory is current: ${options.output}\n`)
      }
      return
    }
    writeFileSync(options.output, actual)
    process.stdout.write(`Wrote renderer import inventory: ${options.output}\n`)
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`)
    process.exitCode = 2
  }
}

if (process.argv[1] && resolve(import.meta.filename) === resolve(process.argv[1])) {
  void main()
}
