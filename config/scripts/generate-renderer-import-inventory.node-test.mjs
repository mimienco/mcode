import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'
import { generateRendererImportInventory } from './generate-renderer-import-inventory.mjs'

function createFixture() {
  const root = mkdtempSync(join(tmpdir(), 'mcode-renderer-import-inventory-'))
  mkdirSync(join(root, 'src', 'renderer', 'src'), { recursive: true })
  writeFileSync(
    join(root, 'src', 'renderer', 'src', 'main.ts'),
    ["import './reachable'", "import 'react'"].join('\n')
  )
  writeFileSync(
    join(root, 'src', 'renderer', 'src', 'reachable.ts'),
    "import { readFileSync } from 'node:fs'\nvoid readFileSync\n"
  )
  return root
}

test('rejects a Node builtin reachable from a production renderer entry', () => {
  const root = createFixture()
  try {
    assert.throws(
      () =>
        generateRendererImportInventory(root, {
          entryPoints: ['src/renderer/src/main.ts'],
          allowlist: { schemaVersion: 1, entries: [] }
        }),
      /reachable production renderer imports node:fs/
    )
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('accepts an exact allowlist entry for unreachable test support', () => {
  const root = createFixture()
  try {
    writeFileSync(join(root, 'src', 'renderer', 'src', 'reachable.ts'), 'export {}\n')
    writeFileSync(
      join(root, 'src', 'renderer', 'src', 'test-support.ts'),
      "import { join } from 'node:path'\nvoid join\n"
    )
    const report = generateRendererImportInventory(root, {
      entryPoints: ['src/renderer/src/main.ts'],
      allowlist: {
        schemaVersion: 1,
        entries: [
          {
            file: 'src/renderer/src/test-support.ts',
            specifier: 'node:path',
            importKind: 'value',
            rationale: 'Test-only support module; unreachable from every production entry.'
          }
        ]
      }
    })

    assert.deepEqual(report.allowlistedImports, [
      {
        file: 'src/renderer/src/test-support.ts',
        line: 1,
        column: 1,
        specifier: 'node:path',
        importKind: 'value',
        rationale: 'Test-only support module; unreachable from every production entry.'
      }
    ])
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('rejects stale allowlist entries', () => {
  const root = createFixture()
  try {
    writeFileSync(join(root, 'src', 'renderer', 'src', 'reachable.ts'), 'export {}\n')
    assert.throws(
      () =>
        generateRendererImportInventory(root, {
          entryPoints: ['src/renderer/src/main.ts'],
          allowlist: {
            schemaVersion: 1,
            entries: [
              {
                file: 'src/renderer/src/deleted-support.ts',
                specifier: 'node:path',
                importKind: 'value',
                rationale: 'No longer present.'
              }
            ]
          }
        }),
      /Stale renderer import allowlist entry/
    )
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('follows literal dynamic imports into lazy production modules', () => {
  const root = createFixture()
  try {
    writeFileSync(join(root, 'src', 'renderer', 'src', 'reachable.ts'), 'export {}\n')
    writeFileSync(join(root, 'src', 'renderer', 'src', 'main.ts'), "void import('./lazy')\n")
    writeFileSync(
      join(root, 'src', 'renderer', 'src', 'lazy.ts'),
      "import { readFileSync } from 'node:fs'\nvoid readFileSync\n"
    )
    assert.throws(
      () =>
        generateRendererImportInventory(root, {
          entryPoints: ['src/renderer/src/main.ts'],
          allowlist: { schemaVersion: 1, entries: [] }
        }),
      /lazy\.ts:1 reachable production renderer imports node:fs/
    )
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('inventories exact external packages used by the reachable graph', () => {
  const root = createFixture()
  try {
    writeFileSync(join(root, 'src', 'renderer', 'src', 'reachable.ts'), 'export {}\n')
    const report = generateRendererImportInventory(root, {
      entryPoints: ['src/renderer/src/main.ts'],
      allowlist: { schemaVersion: 1, entries: [] }
    })

    assert.deepEqual(report.externalPackages, [
      {
        package: 'react',
        specifiers: ['react'],
        importingFiles: ['src/renderer/src/main.ts']
      }
    ])
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('records local assets without parsing them as source modules', () => {
  const root = createFixture()
  try {
    writeFileSync(join(root, 'src', 'renderer', 'src', 'reachable.ts'), 'export {}\n')
    writeFileSync(join(root, 'src', 'renderer', 'src', 'main.ts'), "import './theme.css'\n")
    writeFileSync(join(root, 'src', 'renderer', 'src', 'theme.css'), ':root { color: red; }\n')

    const report = generateRendererImportInventory(root, {
      entryPoints: ['src/renderer/src/main.ts'],
      allowlist: { schemaVersion: 1, entries: [] }
    })
    assert.deepEqual(report.reachableFiles, ['src/renderer/src/main.ts'])
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('rejects unresolved local source imports', () => {
  const root = createFixture()
  try {
    writeFileSync(join(root, 'src', 'renderer', 'src', 'main.ts'), "import './missing-module'\n")
    writeFileSync(join(root, 'src', 'renderer', 'src', 'reachable.ts'), 'export {}\n')

    assert.throws(
      () =>
        generateRendererImportInventory(root, {
          entryPoints: ['src/renderer/src/main.ts'],
          allowlist: { schemaVersion: 1, entries: [] }
        }),
      /Unresolved local renderer import.*missing-module/
    )
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('rejects non-literal dynamic imports that could hide a Node dependency', () => {
  const root = createFixture()
  try {
    writeFileSync(
      join(root, 'src', 'renderer', 'src', 'main.ts'),
      "const moduleName = './lazy'\nvoid import(moduleName)\n"
    )
    writeFileSync(join(root, 'src', 'renderer', 'src', 'reachable.ts'), 'export {}\n')

    assert.throws(
      () =>
        generateRendererImportInventory(root, {
          entryPoints: ['src/renderer/src/main.ts'],
          allowlist: { schemaVersion: 1, entries: [] }
        }),
      /Unresolved dynamic renderer import.*import\(moduleName\)/
    )
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('does not make type-only local imports runtime reachable', () => {
  const root = createFixture()
  try {
    writeFileSync(
      join(root, 'src', 'renderer', 'src', 'main.ts'),
      "import { type Hidden } from './type-only'\nexport type Visible = Hidden\n"
    )
    writeFileSync(
      join(root, 'src', 'renderer', 'src', 'type-only.ts'),
      "import { readFileSync } from 'node:fs'\nexport type Hidden = typeof readFileSync\n"
    )
    writeFileSync(join(root, 'src', 'renderer', 'src', 'reachable.ts'), 'export {}\n')

    const report = generateRendererImportInventory(root, {
      entryPoints: ['src/renderer/src/main.ts'],
      allowlist: {
        schemaVersion: 1,
        entries: [
          {
            file: 'src/renderer/src/type-only.ts',
            specifier: 'node:fs',
            importKind: 'value',
            rationale: 'Type-only fixture dependency; unreachable at runtime.'
          }
        ]
      }
    })
    assert.deepEqual(report.reachableFiles, ['src/renderer/src/main.ts'])
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('rejects Electron subpath imports reachable from production', () => {
  const root = createFixture()
  try {
    writeFileSync(join(root, 'src', 'renderer', 'src', 'main.ts'), "import 'electron/renderer'\n")
    writeFileSync(join(root, 'src', 'renderer', 'src', 'reachable.ts'), 'export {}\n')

    assert.throws(
      () =>
        generateRendererImportInventory(root, {
          entryPoints: ['src/renderer/src/main.ts'],
          allowlist: { schemaVersion: 1, entries: [] }
        }),
      /reachable production renderer imports electron\/renderer/
    )
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('rejects parser recovery errors instead of returning partial evidence', () => {
  const root = createFixture()
  try {
    writeFileSync(join(root, 'src', 'renderer', 'src', 'main.ts'), 'let duplicate\nlet duplicate\n')
    writeFileSync(join(root, 'src', 'renderer', 'src', 'reachable.ts'), 'export {}\n')

    assert.throws(
      () =>
        generateRendererImportInventory(root, {
          entryPoints: ['src/renderer/src/main.ts'],
          allowlist: { schemaVersion: 1, entries: [] }
        }),
      /Renderer source parse recovery error.*main\.ts/
    )
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('rejects TypeScript import-equals references to Node builtins', () => {
  const root = createFixture()
  try {
    writeFileSync(
      join(root, 'src', 'renderer', 'src', 'main.ts'),
      "import filesystem = require('node:fs')\nvoid filesystem\n"
    )
    writeFileSync(join(root, 'src', 'renderer', 'src', 'reachable.ts'), 'export {}\n')

    assert.throws(
      () =>
        generateRendererImportInventory(root, {
          entryPoints: ['src/renderer/src/main.ts'],
          allowlist: { schemaVersion: 1, entries: [] }
        }),
      /reachable production renderer imports node:fs/
    )
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('follows static template-literal dynamic imports', () => {
  const root = createFixture()
  try {
    writeFileSync(join(root, 'src', 'renderer', 'src', 'main.ts'), 'void import(`./lazy`)\n')
    writeFileSync(
      join(root, 'src', 'renderer', 'src', 'lazy.ts'),
      "import { readFileSync } from 'node:fs'\nvoid readFileSync\n"
    )
    writeFileSync(join(root, 'src', 'renderer', 'src', 'reachable.ts'), 'export {}\n')

    assert.throws(
      () =>
        generateRendererImportInventory(root, {
          entryPoints: ['src/renderer/src/main.ts'],
          allowlist: { schemaVersion: 1, entries: [] }
        }),
      /lazy\.ts:1 reachable production renderer imports node:fs/
    )
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('rejects non-literal require calls that could hide a Node dependency', () => {
  const root = createFixture()
  try {
    writeFileSync(
      join(root, 'src', 'renderer', 'src', 'main.ts'),
      "const dependency = 'node:fs'\nvoid require(dependency)\n"
    )
    writeFileSync(join(root, 'src', 'renderer', 'src', 'reachable.ts'), 'export {}\n')

    assert.throws(
      () =>
        generateRendererImportInventory(root, {
          entryPoints: ['src/renderer/src/main.ts'],
          allowlist: { schemaVersion: 1, entries: [] }
        }),
      /Unresolved dynamic renderer import.*require\(dependency\)/
    )
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('follows static template-literal require calls', () => {
  const root = createFixture()
  try {
    writeFileSync(join(root, 'src', 'renderer', 'src', 'main.ts'), 'void require(`node:fs`)\n')
    writeFileSync(join(root, 'src', 'renderer', 'src', 'reachable.ts'), 'export {}\n')

    assert.throws(
      () =>
        generateRendererImportInventory(root, {
          entryPoints: ['src/renderer/src/main.ts'],
          allowlist: { schemaVersion: 1, entries: [] }
        }),
      /reachable production renderer imports node:fs/
    )
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})
