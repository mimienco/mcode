import { spawn, execFileSync } from 'node:child_process'
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { createServer } from 'node:net'
import { chromium } from 'playwright'

const repo = resolve(import.meta.dirname, '../..')
const fixture = await mkdtemp(join(tmpdir(), 'mcode-tauri-workspace-'))
const folder = join(fixture, 'Native workspace fixture')
const gitFolder = join(fixture, 'Native Git fixture')
const appData = join(fixture, 'appdata')
await Promise.all([folder, gitFolder, appData].map((path) => mkdir(path)))
execFileSync('git', ['init', gitFolder], { windowsHide: true, stdio: 'ignore' })
const settings = join(fixture, 'mcode-data.json')
await writeFile(
  settings,
  JSON.stringify({ settings: { workspaceDir: fixture }, workspaceSession: {} })
)
const portServer = createServer()
await new Promise((resolveReady, reject) => {
  portServer.once('error', reject)
  portServer.listen(0, '127.0.0.1', resolveReady)
})
const port = portServer.address().port
await new Promise((resolveClosed) => portServer.close(resolveClosed))
const child = spawn(
  resolve(repo, 'target/debug', process.platform === 'win32' ? 'mcode-tauri.exe' : 'mcode-tauri'),
  [],
  {
    cwd: repo,
    windowsHide: true,
    stdio: ['ignore', 'pipe', 'pipe'],
    env: {
      ...process.env,
      APPDATA: appData,
      LOCALAPPDATA: appData,
      XDG_CONFIG_HOME: appData,
      XDG_DATA_HOME: appData,
      MCODE_TAURI_SETTINGS_PATH: settings,
      MCODE_TAURI_WEBVIEW_DATA_DIR: join(fixture, 'webview'),
      MCODE_TAURI_WEBVIEW_REMOTE_DEBUGGING_PORT: String(port)
    }
  }
)
let appLog = ''
for (const stream of [child.stdout, child.stderr]) {
  stream.on('data', (chunk) => {
    appLog = (appLog + chunk.toString()).slice(-65536)
  })
}
let spawnError
child.once('error', (error) => {
  spawnError = error
})
const errors = []
const deadlineTimer = setTimeout(() => {
  errors.push({ kind: 'smoke', message: 'Native workspace smoke exceeded 180 seconds' })
  child.kill()
}, 180_000)
let browser
let passed = false
try {
  const deadline = Date.now() + 60_000
  while (true) {
    if (spawnError) {
      throw spawnError
    }
    if (child.exitCode !== null) {
      throw new Error(`Native app exited early: ${child.exitCode}`)
    }
    if (Date.now() > deadline) {
      throw new Error('Native WebView debugging endpoint did not start')
    }
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/version`)
      if (response.ok) {
        break
      }
    } catch {}
    await new Promise((resolveDelay) => setTimeout(resolveDelay, 200))
  }
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`)
  const context = browser.contexts()[0]
  const page = context.pages()[0] ?? (await context.waitForEvent('page', { timeout: 30_000 }))
  page.on('pageerror', (error) => errors.push({ kind: 'exception', message: error.message }))
  page.on('console', (message) => {
    if (message.type() === 'error') {
      errors.push({ kind: 'console', message: message.text() })
    }
  })
  await page.reload({ waitUntil: 'domcontentloaded', timeout: 60_000 })
  await page.waitForFunction(() => Boolean(window.api?.repos?.add), undefined, { timeout: 30_000 })
  const created = await page.evaluate(
    async ({ folder, gitFolder }) => {
      const folderResult = await window.api.repos.add({ path: folder, kind: 'folder' })
      const gitResult = await window.api.repos.add({ path: gitFolder })
      for (const result of [folderResult, gitResult]) {
        const rows = await window.api.worktrees.list({ repoId: result.repo.id })
        if (!rows.some((row) => row.isMainWorktree)) {
          throw new Error('No actual main workspace was discovered')
        }
      }
      const state = await window.api.session.get()
      await window.api.session.set({ ...state, activeRepoId: folderResult.repo.id })
      await window.api.session.flush()
      return { folderId: folderResult.repo.id, gitId: gitResult.repo.id }
    },
    { folder, gitFolder }
  )
  await page.reload({ waitUntil: 'domcontentloaded', timeout: 60_000 })
  await page.waitForFunction(() => Boolean(window.api?.session?.get), undefined, {
    timeout: 30_000
  })
  const restored = await page.evaluate(async () => ({
    repos: await window.api.repos.list(),
    session: await window.api.session.get()
  }))
  if (
    ![created.folderId, created.gitId].every((id) =>
      restored.repos.some((entry) => entry.id === id)
    )
  ) {
    throw new Error('Project catalog did not survive reload')
  }
  // A persisted active repo must survive the shared renderer hydration path.
  if (restored.session.activeRepoId !== created.folderId) {
    throw new Error('Session active repo was lost')
  }
  await page
    .getByText('Native workspace fixture', { exact: true })
    .first()
    .waitFor({ timeout: 30_000 })
  await writeFile(join(fixture, 'rendered-text.txt'), await page.locator('body').innerText())
  await page.screenshot({ path: join(fixture, 'workspace.png') })
  if (errors.some((error) => error.kind === 'exception')) {
    throw new Error('Renderer raised an uncaught exception')
  }
  passed = true
} catch (error) {
  errors.push({ kind: 'smoke', message: String(error) })
} finally {
  clearTimeout(deadlineTimer)
  await browser?.close().catch(() => {})
  if (child.exitCode === null) {
    child.kill()
  }
  await Promise.all([
    writeFile(join(fixture, 'app.log'), appLog),
    writeFile(join(fixture, 'errors.json'), JSON.stringify(errors, null, 2))
  ])
}
console.log(JSON.stringify({ passed, fixture, errors }, null, 2))
process.exitCode = passed ? 0 : 1
