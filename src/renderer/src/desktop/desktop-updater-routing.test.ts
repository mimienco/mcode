import { readdirSync, readFileSync } from 'node:fs'
import { extname, join, relative } from 'node:path'
import { expect, it } from 'vitest'

const RENDERER_SOURCE_ROOT = join(process.cwd(), 'src', 'renderer', 'src')
const SOURCE_EXTENSIONS = new Set(['.ts', '.tsx', '.mts', '.cts', '.js', '.jsx', '.mjs', '.cjs'])
const TEST_FILE_PATTERN = /\.(?:bench|test|spec)\.[^.]+$/
const ELECTRON_COMPATIBILITY_CLIENTS = [
  'desktop/runtime-desktop-updater-client.ts',
  // The Tauri typed bridge mirrors window.api.updater verbatim in its
  // Electron-compatibility branch; every other production file must route
  // through a runtime client.
  'desktop/updater-client.ts'
]

function productionRendererSources(directory = RENDERER_SOURCE_ROOT): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) {
      return productionRendererSources(path)
    }
    if (
      !entry.isFile() ||
      !SOURCE_EXTENSIONS.has(extname(entry.name)) ||
      TEST_FILE_PATTERN.test(entry.name)
    ) {
      return []
    }
    return [path]
  })
}
it('routes every production renderer update operation through the runtime updater client', () => {
  const directUpdaterCallers = productionRendererSources()
    .filter(
      (path) =>
        !ELECTRON_COMPATIBILITY_CLIENTS.includes(
          relative(RENDERER_SOURCE_ROOT, path).replaceAll('\\', '/')
        )
    )
    .filter((path) =>
      /window\.api\.updater\.(?:getVersion|getStatus|check|download|quitAndInstall|onStatus)\s*\(/.test(
        readFileSync(path, 'utf8')
      )
    )
    .map((path) => relative(RENDERER_SOURCE_ROOT, path).replaceAll('\\', '/'))

  expect(directUpdaterCallers).toEqual([])
})
