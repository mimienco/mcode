import { spawn } from 'node:child_process'
import { mkdtemp, mkdir, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { resolve, join } from 'node:path'

const repo = resolve(import.meta.dirname, '../..')
const fixture = await mkdtemp(join(tmpdir(), 'mcode-tauri-agent-status-'))
const marker = join(fixture, 'verdict.txt')
const settings = join(fixture, 'mcode-data.json')
const appData = join(fixture, 'appdata')
await mkdir(appData)
await writeFile(
  settings,
  JSON.stringify({ settings: { workspaceDir: fixture }, workspaceSession: {} })
)
const response = await fetch('http://127.0.0.1:1420/lifecycle-smoke.html')
if (!response.ok) {
  throw new Error(`Tauri renderer is unavailable: HTTP ${response.status}`)
}
const executable = resolve(
  repo,
  'target/debug',
  process.platform === 'win32' ? 'mcode-tauri.exe' : 'mcode-tauri'
)
const child = spawn(executable, [], {
  cwd: repo,
  windowsHide: true,
  env: {
    ...process.env,
    APPDATA: appData,
    LOCALAPPDATA: appData,
    XDG_CONFIG_HOME: appData,
    XDG_DATA_HOME: appData,
    MCODE_TAURI_SETTINGS_PATH: settings,
    MCODE_TAURI_SMOKE_TEST: 'agent_status',
    MCODE_TAURI_SMOKE_AGENT_STATUS_MARKER: marker
  },
  stdio: ['ignore', 'pipe', 'pipe']
})
let output = ''
for (const stream of [child.stdout, child.stderr]) {
  stream.on('data', (chunk) => {
    output = (output + chunk.toString()).slice(-65536)
  })
}
let timedOut = false
const timeout = setTimeout(() => {
  timedOut = true
  child.kill()
}, 100_000)
const exit = await new Promise((resolveExit, reject) => {
  child.once('error', reject)
  child.once('close', (code, signal) => resolveExit({ code, signal }))
}).finally(() => clearTimeout(timeout))
await writeFile(join(fixture, 'app.log'), output)
const verdict = await readFile(marker, 'utf8').catch(() => 'missing')
const passed = !timedOut && exit.code === 0 && verdict.trim() === 'passed'
console.log(JSON.stringify({ passed, verdict: verdict.trim(), exit, timedOut, fixture }, null, 2))
if (!passed) {
  console.error(output)
}
process.exitCode = passed ? 0 : 1
