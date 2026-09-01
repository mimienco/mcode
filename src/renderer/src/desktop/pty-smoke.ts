import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { createTauriFsClient } from './fs-client'
import { createDesktopInvoke } from './desktop-client'
import { createTauriPtyClient, type PtyDataPayload, type PtyExitPayload } from './pty-client'

const smokeParameters = new URLSearchParams(window.location.search)
const echo = smokeParameters.get('echo')
const marker = smokeParameters.get('marker')

const failStatus = (message: string): void => {
  document.getElementById('status')?.replaceChildren(message)
  // Why: the smoke page runs in an invisible window; mirroring progress into
  // the title lets the driver script observe milestones from the outside.
  document.title = message
}

if (!echo || !marker) {
  failStatus('Pty smoke misconfigured: echo/marker missing')
  throw new Error('pty smoke requires echo and marker query parameters')
}

const fs = createTauriFsClient(
  (command, args) => invoke(command, args),
  (event, listener) => listen(event, listener)
)

// Why: the smoke driver's readiness handshake — the Rust side waits for the
// tray-revision to advance past 0 (NativeWindowState::close_to_tray_revision)
// before polling the verdict marker, mirroring the Filesystem smoke arm. It
// lives inside the try block: a failure here must land in the verdict marker,
// not die at module scope where nothing would be reported.
const desktopInvoke = createDesktopInvoke((command, args) => invoke(command, args))

const report = (verdict: 'passed' | 'renderer-failed', detail?: string): Promise<void> =>
  fs.writeFileAtomic({ filePath: marker, content: detail ? `${verdict}: ${detail}` : verdict })

const settle = <T>(promise: Promise<T>, timeoutMs: number, label: string): Promise<T> =>
  new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`${label} timed out`)), timeoutMs)
    void promise.then(
      (value) => {
        clearTimeout(timer)
        resolve(value)
      },
      (error: unknown) => {
        clearTimeout(timer)
        reject(error)
      }
    )
  })

try {
  const client = createTauriPtyClient(
    (command, args) => invoke(command, args),
    (event, listener) => listen(event, listener)
  )

  // BISECT: prove the fs write path works before anything else.
  try {
    await fs.writeFileAtomic({ filePath: `${marker}.probe`, content: 'probe' })
  } catch (probeError) {
    await fs
      .writeFileAtomic({ filePath: `${marker}.probe2`, content: String(probeError) })
      .catch(() => undefined)
  }
  await desktopInvoke('window_set_minimize_to_tray_on_close', { enabled: true })

  document.title = 'ready-handshake-done'
  // Subscriptions are installed before the spawn: `cmd /c echo` can exit
  // immediately, and the forwarder owns both the final flush and the exit
  // announcement. Events are buffered by id, then replayed below.
  let resolveEcho: () => void = () => undefined
  const echoPromise = new Promise<void>((resolve) => {
    resolveEcho = resolve
  })
  let resolveExit: () => void = () => undefined
  let rejectExit: (error: unknown) => void = () => undefined
  const exitPromise = new Promise<void>((resolve, reject) => {
    resolveExit = resolve
    rejectExit = reject
  })

  let sawEcho = false
  const sawExit: PtyExitPayload[] = []
  const unsubscribeData = await client.onData((payload: PtyDataPayload) => {
    if (sawEcho) {
      return
    }
    if (payload.chunks.some((chunk) => chunk.data.includes(echo))) {
      sawEcho = true
      resolveEcho()
    }
  })
  const unsubscribeExit = await client.onExit((payload: PtyExitPayload) => {
    sawExit.push(payload)
    resolveExit()
  })
  document.title = 'subscriptions-installed'

  const spawned = await client.spawn({
    program: 'cmd',
    argVec: ['/c', 'echo', echo],
    cols: 80,
    rows: 24
  })

  // 60s: ConPTY cold start on a loaded dev box can take tens of seconds
  // before the first output bytes flow.
  await settle(echoPromise, 60_000, 'pty echo')

  // Optional capability probes: ackData/getSize may not exist yet while the
  // sibling migration lands them. Their absence is not a smoke failure.
  await client.ackData?.(spawned.id, echo.length)
  const size = await client.getSize?.(spawned.id)
  if (size !== null && size !== undefined && (!(size.cols > 0) || !(size.rows > 0))) {
    rejectExit(new Error(`pty getSize returned invalid size ${JSON.stringify(size)}`))
    throw new Error(`pty getSize returned invalid size ${JSON.stringify(size)}`)
  }

  await settle(exitPromise, 10_000, 'pty exit')
  const exit = sawExit.find((payload) => payload.id === spawned.id)
  if (!exit) {
    throw new Error(`pty exit event never matched spawned id ${spawned.id}`)
  }
  if (typeof exit.code !== 'number') {
    throw new Error('pty exit event carried no exit code')
  }

  await client.kill(spawned.id)

  unsubscribeData()
  unsubscribeExit()

  await report('passed')
  document.getElementById('status')?.replaceChildren('Pty smoke passed')
} catch (error) {
  console.error('[smoke] pty smoke failed', error)
  failStatus(`Pty smoke failed: ${error instanceof Error ? error.message : String(error)}`)
  try {
    await report('renderer-failed', error instanceof Error ? error.message : String(error))
  } catch (reportError) {
    console.error('[smoke] failed to report pty smoke failure', reportError)
    document
      .getElementById('status')
      ?.replaceChildren(`Pty smoke failed AND marker write failed: ${String(reportError)}`)
  }
}
