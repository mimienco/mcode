import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'
import { generateElectronIpcInventory } from './generate-electron-ipc-inventory.mjs'

function createFixture() {
  const root = mkdtempSync(join(tmpdir(), 'mcode-electron-ipc-inventory-'))
  mkdirSync(join(root, 'src', 'main'), { recursive: true })
  mkdirSync(join(root, 'src', 'preload'), { recursive: true })
  mkdirSync(join(root, 'src', 'shared'), { recursive: true })
  writeFileSync(
    join(root, 'src', 'shared', 'channels.ts'),
    [
      "const PREFIX = 'app'",
      'export const GET_CHANNEL = `${PREFIX}:get`',
      "export const CHANGED_CHANNEL = 'app:changed'"
    ].join('\n')
  )
  writeFileSync(
    join(root, 'src', 'main', 'ipc.ts'),
    [
      "import { ipcMain } from 'electron'",
      "import { GET_CHANNEL } from '../shared/channels'",
      'ipcMain.handle(GET_CHANNEL, () => undefined)',
      'ipcMain.on(process.env.MCODE_DYNAMIC_CHANNEL, () => undefined)',
      "renderer?.send('app:changed')"
    ].join('\n')
  )
  writeFileSync(
    join(root, 'src', 'preload', 'index.ts'),
    [
      "import { ipcRenderer } from 'electron'",
      "import { CHANGED_CHANNEL, GET_CHANNEL } from '../shared/channels'",
      'const ipc = ipcRenderer',
      'ipc.invoke(GET_CHANNEL)',
      'ipc.on(CHANGED_CHANNEL, () => undefined)'
    ].join('\n')
  )
  writeFileSync(
    join(root, 'src', 'main', 'ignored.test.ts'),
    "ipcMain.handle('test:only', () => undefined)\n"
  )
  return root
}

test('resolves imported and templated channels and indexes both directions', () => {
  const root = createFixture()
  try {
    const report = generateElectronIpcInventory(root)

    assert.equal(report.summary.entries, 5)
    assert.equal(report.summary.resolvedEntries, 4)
    assert.equal(report.summary.dynamicEntries, 1)
    assert.deepEqual(report.summary.byRole, {
      registration: 2,
      caller: 1,
      emitter: 1,
      subscription: 1
    })

    const getChannel = report.channels.find((entry) => entry.channel === 'app:get')
    assert.ok(getChannel)
    assert.equal(getChannel.registrations.length, 1)
    assert.equal(getChannel.callers.length, 1)

    const changedChannel = report.channels.find((entry) => entry.channel === 'app:changed')
    assert.ok(changedChannel)
    assert.equal(changedChannel.emitters.length, 1)
    assert.equal(changedChannel.subscriptions.length, 1)

    assert.equal(report.dynamicEntries[0].expression, 'process.env.MCODE_DYNAMIC_CHANNEL')
    assert.equal(
      report.entries.some((entry) => entry.channels.includes('test:only')),
      false
    )
    assert.deepEqual(report.correlation, {
      registrationsWithoutCallers: [],
      callersWithoutRegistrations: [],
      emittersWithoutSubscriptions: [],
      subscriptionsWithoutEmitters: []
    })
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('keeps ordering and identifiers independent of the absolute source directory', () => {
  const leftRoot = createFixture()
  const rightRoot = createFixture()
  try {
    const left = generateElectronIpcInventory(leftRoot)
    const right = generateElectronIpcInventory(rightRoot)

    assert.deepEqual(left.entries, right.entries)
    assert.deepEqual(left.channels, right.channels)
    assert.deepEqual(left.summary, right.summary)
  } finally {
    rmSync(leftRoot, { recursive: true, force: true })
    rmSync(rightRoot, { recursive: true, force: true })
  }
})

test('requires explicit evidence to reconcile a dynamic factory channel', () => {
  const root = createFixture()
  try {
    const report = generateElectronIpcInventory(root, {
      schemaVersion: 1,
      resolutions: [
        {
          id: 'src/main/ipc.ts:4:1',
          channels: ['app:dynamic'],
          rationale: 'Fixture call-site reconciliation.',
          evidence: ['src/main/ipc.ts:4']
        }
      ]
    })

    assert.equal(report.summary.dynamicEntries, 0)
    assert.equal(report.summary.reconciledEntries, 1)
    assert.equal(report.dynamicEntries.length, 0)
    assert.equal(report.reconciledEntries[0].channels[0], 'app:dynamic')
    assert.equal(
      report.channels.find((entry) => entry.channel === 'app:dynamic')?.registrations.length,
      1
    )
    assert.deepEqual(report.correlation.registrationsWithoutCallers, ['app:dynamic'])
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})
