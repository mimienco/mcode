import { parse } from '@babel/parser'
import traverseModule from '@babel/traverse'
import { format } from 'oxfmt'
import { execFileSync } from 'node:child_process'
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { extname, relative, resolve, sep } from 'node:path'
import {
  EXCLUDED_DIRECTORIES,
  IPC_MAIN_METHODS,
  IPC_RENDERER_METHODS,
  SOURCE_DIRECTORIES,
  SOURCE_EXTENSIONS,
  TEST_FILE_PATTERN
} from './electron-ipc-inventory-config.mjs'

const traverse = traverseModule.default ?? traverseModule
const SCHEMA_VERSION = 1

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
    if (argument === '--source' || argument === '--output' || argument === '--resolutions') {
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
  if (!options.source || !options.output) {
    throw new Error(
      'Usage: generate-electron-ipc-inventory.mjs --source <path> --output <path> [--resolutions <path>] [--check]'
    )
  }
  return options
}

function isProductionSource(path) {
  const normalized = normalizePath(path)
  const segments = normalized.split('/')
  const basename = segments.at(-1) ?? ''
  return (
    SOURCE_EXTENSIONS.has(extname(basename).toLowerCase()) &&
    !segments.some((segment) => EXCLUDED_DIRECTORIES.has(segment)) &&
    !TEST_FILE_PATTERN.test(basename)
  )
}

function collectSourceFiles(sourceRoot) {
  const files = []
  const visit = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      if (entry.isDirectory() && EXCLUDED_DIRECTORIES.has(entry.name)) {
        continue
      }
      const path = resolve(directory, entry.name)
      if (entry.isDirectory()) {
        visit(path)
      } else if (entry.isFile() && isProductionSource(path)) {
        files.push(path)
      }
    }
  }
  for (const sourceDirectory of SOURCE_DIRECTORIES) {
    const absolute = resolve(sourceRoot, sourceDirectory)
    if (existsSync(absolute) && statSync(absolute).isDirectory()) {
      visit(absolute)
    }
  }
  return files.sort((left, right) => normalizePath(left).localeCompare(normalizePath(right)))
}

function sourceArea(sourceRoot, file) {
  const path = normalizePath(relative(sourceRoot, file))
  if (path.startsWith('src/main/')) {
    return 'main'
  }
  if (path.startsWith('src/preload/')) {
    return 'preload'
  }
  if (path.startsWith('src/renderer/')) {
    return 'renderer'
  }
  return 'shared'
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

function parseSources(sourceRoot, files) {
  const records = new Map()
  const failures = []
  for (const file of files) {
    const text = readFileSync(file, 'utf8')
    try {
      const ast = parse(text, {
        sourceType: 'unambiguous',
        errorRecovery: true,
        plugins: parserPlugins(file)
      })
      const record = {
        file,
        relativeFile: normalizePath(relative(sourceRoot, file)),
        area: sourceArea(sourceRoot, file),
        text,
        ast,
        exports: new Map(),
        programPath: undefined
      }
      traverse(ast, {
        Program(path) {
          record.programPath = path
          path.stop()
        }
      })
      for (const statement of ast.program.body) {
        if (statement.type !== 'ExportNamedDeclaration') {
          continue
        }
        if (statement.declaration?.type === 'VariableDeclaration') {
          for (const declaration of statement.declaration.declarations) {
            if (declaration.id.type === 'Identifier') {
              record.exports.set(declaration.id.name, declaration.id.name)
            }
          }
        }
        for (const specifier of statement.specifiers) {
          if (specifier.type === 'ExportSpecifier') {
            const exported =
              specifier.exported.type === 'Identifier'
                ? specifier.exported.name
                : specifier.exported.value
            const local =
              specifier.local.type === 'Identifier' ? specifier.local.name : specifier.local.value
            record.exports.set(exported, local)
          }
        }
      }
      records.set(file, record)
    } catch (error) {
      failures.push({
        file: normalizePath(relative(sourceRoot, file)),
        message: error instanceof Error ? error.message : String(error)
      })
    }
  }
  return { records, failures }
}

function unwrapPath(path) {
  let current = path
  while (
    current?.isTSAsExpression?.() ||
    current?.isTSSatisfiesExpression?.() ||
    current?.isTSNonNullExpression?.() ||
    current?.isTypeCastExpression?.()
  ) {
    current = current.get('expression')
  }
  return current
}

function resolveModule(records, fromFile, request) {
  if (!request.startsWith('.')) {
    return undefined
  }
  const base = resolve(fromFile, '..', request)
  const candidates = [
    base,
    ...[...SOURCE_EXTENSIONS].map((extension) => `${base}${extension}`),
    ...[...SOURCE_EXTENSIONS].map((extension) => resolve(base, `index${extension}`))
  ]
  return candidates.map((candidate) => records.get(candidate)).find(Boolean)
}

function bindingIdentity(binding) {
  return `${binding.path.hub?.file?.opts?.filename ?? ''}:${binding.identifier.start ?? ''}`
}

function resolveImportedBinding(binding, context, seen) {
  const path = binding.path
  const declaration = path.parentPath
  if (!declaration?.isImportDeclaration()) {
    return undefined
  }
  const target = resolveModule(context.records, context.record.file, declaration.node.source.value)
  if (!target?.programPath) {
    return undefined
  }
  let imported
  if (path.isImportSpecifier()) {
    imported =
      path.node.imported.type === 'Identifier' ? path.node.imported.name : path.node.imported.value
  } else if (path.isImportDefaultSpecifier()) {
    imported = 'default'
  } else {
    return undefined
  }
  const local = target.exports.get(imported) ?? imported
  const targetBinding = target.programPath.scope.getBinding(local)
  return targetBinding
    ? resolveBinding(targetBinding, { ...context, record: target }, seen)
    : undefined
}

function resolveBinding(binding, context, seen) {
  const identity = bindingIdentity(binding)
  if (seen.has(identity)) {
    return undefined
  }
  const nextSeen = new Set(seen).add(identity)
  if (binding.path.isVariableDeclarator()) {
    const initializer = binding.path.get('init')
    return initializer?.node ? resolveChannelValues(initializer, context, nextSeen) : undefined
  }
  if (binding.path.isImportSpecifier() || binding.path.isImportDefaultSpecifier()) {
    return resolveImportedBinding(binding, context, nextSeen)
  }
  return undefined
}

function combineValues(left, right) {
  if (!left || !right || left.length * right.length > 64) {
    return undefined
  }
  const values = new Set()
  for (const leftValue of left) {
    for (const rightValue of right) {
      values.add(`${leftValue}${rightValue}`)
    }
  }
  return [...values]
}

function memberPropertyName(node) {
  if (!node.computed && node.property.type === 'Identifier') {
    return node.property.name
  }
  if (node.property.type === 'StringLiteral') {
    return node.property.value
  }
  return undefined
}

function resolveObjectMember(path, context, seen) {
  const propertyName = memberPropertyName(path.node)
  if (!propertyName) {
    return undefined
  }
  const objectPath = unwrapPath(path.get('object'))
  if (objectPath.isIdentifier()) {
    const binding = objectPath.scope.getBinding(objectPath.node.name)
    if (binding?.path.isImportNamespaceSpecifier()) {
      const declaration = binding.path.parentPath
      const target = declaration?.isImportDeclaration()
        ? resolveModule(context.records, context.record.file, declaration.node.source.value)
        : undefined
      const local = target?.exports.get(propertyName) ?? propertyName
      const targetBinding = target?.programPath?.scope.getBinding(local)
      return targetBinding
        ? resolveBinding(targetBinding, { ...context, record: target }, seen)
        : undefined
    }
    if (binding?.path.isVariableDeclarator()) {
      const initializer = unwrapPath(binding.path.get('init'))
      if (initializer?.isObjectExpression()) {
        const property = initializer
          .get('properties')
          .find(
            (candidate) =>
              candidate.isObjectProperty() &&
              ((candidate.node.key.type === 'Identifier' &&
                candidate.node.key.name === propertyName) ||
                (candidate.node.key.type === 'StringLiteral' &&
                  candidate.node.key.value === propertyName))
          )
        if (property?.isObjectProperty()) {
          return resolveChannelValues(property.get('value'), context, seen)
        }
      }
    }
  }
  return undefined
}

function resolveChannelValues(rawPath, context, seen = new Set()) {
  const path = unwrapPath(rawPath)
  if (!path?.node) {
    return undefined
  }
  if (path.isStringLiteral() || path.isNumericLiteral()) {
    return [String(path.node.value)]
  }
  if (path.isTemplateLiteral()) {
    let values = [path.node.quasis[0]?.value.cooked ?? '']
    const expressionPaths = path.get('expressions')
    for (let index = 0; index < expressionPaths.length; index += 1) {
      values = combineValues(values, resolveChannelValues(expressionPaths[index], context, seen))
      if (!values) {
        return undefined
      }
      const suffix = path.node.quasis[index + 1]?.value.cooked ?? ''
      values = values.map((value) => `${value}${suffix}`)
    }
    return values
  }
  if (path.isBinaryExpression({ operator: '+' })) {
    return combineValues(
      resolveChannelValues(path.get('left'), context, seen),
      resolveChannelValues(path.get('right'), context, seen)
    )
  }
  if (path.isConditionalExpression()) {
    const values = new Set([
      ...(resolveChannelValues(path.get('consequent'), context, seen) ?? []),
      ...(resolveChannelValues(path.get('alternate'), context, seen) ?? [])
    ])
    return values.size > 0 ? [...values] : undefined
  }
  if (path.isIdentifier()) {
    const binding = path.scope.getBinding(path.node.name)
    return binding ? resolveBinding(binding, context, seen) : undefined
  }
  if (path.isMemberExpression() || path.isOptionalMemberExpression()) {
    return resolveObjectMember(path, context, seen)
  }
  const evaluated = path.evaluate?.()
  return evaluated?.confident && ['string', 'number'].includes(typeof evaluated.value)
    ? [String(evaluated.value)]
    : undefined
}

function receiverText(path, record) {
  return record.text.slice(path.node.start, path.node.end)
}

function receiverMatches(receiver, kind) {
  return new RegExp(`(?:^|\\.)${kind}$`).test(receiver)
}

function isWebContentsSender(receiver) {
  return /(?:webContents|mainWebContents|contentWebContents|contents|renderer|sender|owner|listener|^wc)$/.test(
    receiver
  )
}

function classifyCall(area, receiver, method) {
  if (receiverMatches(receiver, 'ipcMain')) {
    return IPC_MAIN_METHODS.get(method)
  }
  if (receiverMatches(receiver, 'ipcRenderer')) {
    return IPC_RENDERER_METHODS.get(method)
  }
  if (area === 'preload' && receiver === 'ipc') {
    return IPC_RENDERER_METHODS.get(method)
  }
  if (area === 'main' && method === 'send' && isWebContentsSender(receiver)) {
    return { role: 'emitter', direction: 'main_to_renderer', transport: 'event' }
  }
  return undefined
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

function buildChannelIndex(entries) {
  const index = new Map()
  for (const entry of entries) {
    for (const channel of entry.channels) {
      const record = index.get(channel) ?? {
        channel,
        registrations: [],
        callers: [],
        emitters: [],
        subscriptions: []
      }
      record[`${entry.role}s`].push(entry.id)
      index.set(channel, record)
    }
  }
  return [...index.values()].sort((left, right) => left.channel.localeCompare(right.channel))
}

function summarize(entries, channels, parseFailures) {
  const byRole = Object.fromEntries(
    ['registration', 'caller', 'emitter', 'subscription'].map((role) => [
      role,
      entries.filter((entry) => entry.role === role).length
    ])
  )
  const byTransport = {}
  for (const entry of entries) {
    byTransport[entry.transport] = (byTransport[entry.transport] ?? 0) + 1
  }
  return {
    filesWithIpc: new Set(entries.map((entry) => entry.file)).size,
    entries: entries.length,
    resolvedEntries: entries.filter((entry) => entry.resolution !== 'dynamic').length,
    reconciledEntries: entries.filter((entry) => entry.resolution === 'reconciled').length,
    dynamicEntries: entries.filter((entry) => entry.resolution === 'dynamic').length,
    parseFailures,
    distinctChannels: channels.length,
    byRole,
    byTransport: Object.fromEntries(
      Object.entries(byTransport).sort(([left], [right]) => left.localeCompare(right))
    )
  }
}

function correlateChannels(channels) {
  const names = (predicate) => channels.filter(predicate).map((entry) => entry.channel)
  return {
    registrationsWithoutCallers: names(
      (entry) => entry.registrations.length > 0 && entry.callers.length === 0
    ),
    callersWithoutRegistrations: names(
      (entry) => entry.callers.length > 0 && entry.registrations.length === 0
    ),
    emittersWithoutSubscriptions: names(
      (entry) => entry.emitters.length > 0 && entry.subscriptions.length === 0
    ),
    subscriptionsWithoutEmitters: names(
      (entry) => entry.subscriptions.length > 0 && entry.emitters.length === 0
    )
  }
}

function applyDynamicResolutions(entries, resolutionDocument) {
  if (!resolutionDocument) {
    return entries
  }
  if (resolutionDocument.schemaVersion !== 1 || !Array.isArray(resolutionDocument.resolutions)) {
    throw new Error('Dynamic resolution document must use schemaVersion 1 with a resolutions array')
  }
  const entriesById = new Map(entries.map((entry) => [entry.id, entry]))
  const seen = new Set()
  for (const resolution of resolutionDocument.resolutions) {
    if (!resolution || typeof resolution.id !== 'string' || !Array.isArray(resolution.channels)) {
      throw new Error('Every dynamic resolution requires an id and channels array')
    }
    if (seen.has(resolution.id)) {
      throw new Error(`Duplicate dynamic resolution: ${resolution.id}`)
    }
    seen.add(resolution.id)
    const entry = entriesById.get(resolution.id)
    if (!entry) {
      throw new Error(`Dynamic resolution does not match an inventory entry: ${resolution.id}`)
    }
    if (entry.resolution !== 'dynamic') {
      throw new Error(`Dynamic resolution targets an already resolved entry: ${resolution.id}`)
    }
    const channels = [...new Set(resolution.channels)]
    if (
      channels.length === 0 ||
      channels.some((channel) => typeof channel !== 'string' || !channel)
    ) {
      throw new Error(`Dynamic resolution requires non-empty string channels: ${resolution.id}`)
    }
    entry.resolution = 'reconciled'
    entry.channels = channels.sort((left, right) => left.localeCompare(right))
    entry.reconciliation = {
      rationale: resolution.rationale,
      evidence: resolution.evidence
    }
  }
  return entries
}

function collectIpcEntry(path, record, records, entries) {
  const callee = unwrapPath(path.get('callee'))
  if (!callee?.isMemberExpression() && !callee?.isOptionalMemberExpression()) {
    return
  }
  const method = memberPropertyName(callee.node)
  if (!method) {
    return
  }
  const receiverPath = callee.get('object')
  const receiver = receiverText(receiverPath, record)
  const classification = classifyCall(record.area, receiver, method)
  const channelPath = path.get('arguments')[0]
  if (!classification || !channelPath?.node || channelPath.isSpreadElement()) {
    return
  }
  const channels =
    resolveChannelValues(channelPath, { records, record })?.sort((left, right) =>
      left.localeCompare(right)
    ) ?? []
  const location = path.node.loc?.start
  if (!location) {
    return
  }
  entries.push({
    id: `${record.relativeFile}:${location.line}:${location.column + 1}`,
    area: record.area,
    role: classification.role,
    direction: classification.direction,
    transport: classification.transport,
    receiver,
    method,
    file: record.relativeFile,
    line: location.line,
    column: location.column + 1,
    resolution: channels.length > 0 ? 'resolved' : 'dynamic',
    channels,
    expression: record.text.slice(channelPath.node.start, channelPath.node.end)
  })
}

export function generateElectronIpcInventory(sourceRoot, resolutionDocument) {
  const files = collectSourceFiles(sourceRoot)
  const { records, failures } = parseSources(sourceRoot, files)
  const entries = []

  for (const record of records.values()) {
    traverse(record.ast, {
      CallExpression(path) {
        collectIpcEntry(path, record, records, entries)
      },
      OptionalCallExpression(path) {
        collectIpcEntry(path, record, records, entries)
      }
    })
  }

  entries.sort(
    (left, right) =>
      left.file.localeCompare(right.file) || left.line - right.line || left.column - right.column
  )
  applyDynamicResolutions(entries, resolutionDocument)
  const channels = buildChannelIndex(entries)
  return {
    schemaVersion: SCHEMA_VERSION,
    source: readGitIdentity(sourceRoot),
    scan: {
      directories: SOURCE_DIRECTORIES,
      productionFiles: files.length,
      excludedFilePattern: TEST_FILE_PATTERN.source,
      parseFailures: failures
    },
    summary: summarize(entries, channels, failures.length),
    correlation: correlateChannels(channels),
    channels,
    dynamicEntries: entries.filter((entry) => entry.resolution === 'dynamic'),
    reconciledEntries: entries.filter((entry) => entry.resolution === 'reconciled'),
    entries
  }
}

async function serialize(report) {
  const unformatted = `${JSON.stringify(report, null, 2)}\n`
  const formatterConfig = JSON.parse(
    readFileSync(resolve(import.meta.dirname, '..', '..', '.oxfmtrc.json'), 'utf8')
  )
  const formatted = await format('electron-ipc-inventory.json', unformatted, formatterConfig)
  if (formatted.errors.length > 0) {
    throw new Error(
      `Failed to format Electron IPC inventory: ${formatted.errors.map((error) => error.message).join('; ')}`
    )
  }
  return formatted.code
}

async function main() {
  try {
    const options = parseArgs(process.argv.slice(2))
    const resolutions = options.resolutions
      ? JSON.parse(readFileSync(options.resolutions, 'utf8'))
      : undefined
    const actual = await serialize(generateElectronIpcInventory(options.source, resolutions))
    if (options.check) {
      const expected = existsSync(options.output) ? readFileSync(options.output, 'utf8') : ''
      if (expected !== actual) {
        process.stderr.write(`Electron IPC inventory is stale: ${options.output}\n`)
        process.exitCode = 1
      } else {
        process.stdout.write(`Electron IPC inventory is current: ${options.output}\n`)
      }
      return
    }
    writeFileSync(options.output, actual)
    process.stdout.write(`Wrote Electron IPC inventory: ${options.output}\n`)
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`)
    process.exitCode = 2
  }
}

if (process.argv[1] && resolve(import.meta.filename) === resolve(process.argv[1])) {
  void main()
}
