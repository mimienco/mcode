import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process'
import WebSocket from 'ws'
import { getSpawnArgsForWindows, resolveWindowsCommand } from '../win32-utils'
import { killCodexAppServerProcessTree } from './codex-app-server-session'

const START_TIMEOUT_MS = 5_000
const REQUEST_TIMEOUT_MS = 5_000

type JsonRpcResponse = {
  id?: number
  result?: unknown
  error?: { message?: string }
  method?: string
  params?: unknown
}

type NotificationListener = (method: string, params: unknown) => void

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

export class CodexLiveSessionRpc {
  private nextId = 1
  private readonly pending = new Map<
    number,
    { resolve: (value: unknown) => void; reject: (error: Error) => void; timer: NodeJS.Timeout }
  >()
  private readonly notificationListeners = new Set<NotificationListener>()
  private closed = false

  private constructor(
    private readonly child: ChildProcessWithoutNullStreams,
    private readonly socket: WebSocket
  ) {
    socket.on('message', (raw) => this.handleMessage(raw.toString()))
    socket.on('close', () => this.closeWithError(new Error('Codex App Server disconnected.')))
    socket.on('error', () => this.closeWithError(new Error('Codex App Server connection failed.')))
    child.on('exit', () => this.closeWithError(new Error('Codex App Server exited.')))
  }

  static async connect(args: {
    appServerArgv: string[]
    endpoint: string
    env?: Record<string, string>
    cwd?: string
    /** Injectable for tests; bounds each connection attempt, not just total wait. */
    startTimeoutMs?: number
  }): Promise<CodexLiveSessionRpc> {
    const childEnv = { ...process.env, ...args.env }
    const executable = resolveWindowsCommand(args.appServerArgv[0], childEnv)
    const commandArgs = args.appServerArgv.slice(1)
    const spawnPlan =
      process.platform === 'win32'
        ? getSpawnArgsForWindows(executable, commandArgs)
        : { spawnCmd: executable, spawnArgs: commandArgs }
    const startTimeoutMs = args.startTimeoutMs ?? START_TIMEOUT_MS
    const child = spawn(spawnPlan.spawnCmd, spawnPlan.spawnArgs, {
      env: childEnv,
      cwd: args.cwd,
      stdio: ['pipe', 'pipe', 'pipe'],
      windowsHide: true
    }) as ChildProcessWithoutNullStreams
    // App Server writes diagnostics to stderr. Drain both streams so a verbose
    // failure cannot back-pressure the hidden child; never log protocol/auth data.
    child.stdout.resume()
    child.stderr.resume()
    try {
      const socket = await CodexLiveSessionRpc.connectSocket(args.endpoint, child, startTimeoutMs)
      return new CodexLiveSessionRpc(child, socket)
    } catch (error) {
      killCodexAppServerProcessTree(child)
      throw error
    }
  }

  private static async connectSocket(
    endpoint: string,
    child: ChildProcessWithoutNullStreams,
    startTimeoutMs: number
  ): Promise<WebSocket> {
    const deadline = Date.now() + startTimeoutMs
    let lastError: Error = new Error('Codex App Server did not become ready.')
    while (Date.now() < deadline && child.exitCode === null) {
      const remainingMs = Math.max(deadline - Date.now(), 1)
      try {
        return await new Promise<WebSocket>((resolve, reject) => {
          // Why: a peer that accepts TCP but never answers the HTTP upgrade
          // fires neither 'open' nor 'error', so without this timer the await
          // hangs past the startup deadline and the PTY spawn fallback is
          // never reached (BH-RUNTIME-001).
          const socket = new WebSocket(endpoint)
          let settled = false
          const timer = setTimeout(() => {
            if (settled) {
              return
            }
            settled = true
            // Why: terminate() while CONNECTING emits a late 'error'; swallow
            // it or it surfaces as an uncaught exception.
            socket.on('error', () => undefined)
            socket.terminate()
            reject(
              new Error(`Codex App Server WebSocket upgrade timed out after ${remainingMs}ms.`)
            )
          }, remainingMs)
          const cleanup = (): void => {
            clearTimeout(timer)
            socket.off('open', onOpen)
            socket.off('error', onError)
          }
          const onOpen = (): void => {
            if (settled) {
              return
            }
            settled = true
            cleanup()
            resolve(socket)
          }
          const onError = (error: Error): void => {
            if (settled) {
              return
            }
            settled = true
            cleanup()
            socket.close()
            reject(error)
          }
          socket.once('open', onOpen)
          socket.once('error', onError)
        })
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error))
        await delay(75)
      }
    }
    throw lastError
  }

  get available(): boolean {
    return !this.closed && this.socket.readyState === WebSocket.OPEN
  }

  onNotification(listener: NotificationListener): () => void {
    this.notificationListeners.add(listener)
    return () => this.notificationListeners.delete(listener)
  }

  private handleMessage(raw: string): void {
    let response: JsonRpcResponse
    try {
      response = JSON.parse(raw) as JsonRpcResponse
    } catch {
      return
    }
    if (typeof response.method === 'string') {
      for (const listener of this.notificationListeners) {
        listener(response.method, response.params)
      }
      return
    }
    if (typeof response.id !== 'number') {
      return
    }
    const waiter = this.pending.get(response.id)
    if (!waiter) {
      return
    }
    this.pending.delete(response.id)
    clearTimeout(waiter.timer)
    if (response.error) {
      waiter.reject(new Error(response.error.message ?? 'Codex App Server request failed.'))
    } else {
      waiter.resolve(response.result)
    }
  }

  request(method: string, params?: Record<string, unknown>): Promise<unknown> {
    if (!this.available) {
      return Promise.reject(new Error('Codex App Server is unavailable.'))
    }
    const id = this.nextId++
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id)
        reject(new Error(`Codex App Server ${method} timed out.`))
      }, REQUEST_TIMEOUT_MS)
      this.pending.set(id, { resolve, reject, timer })
      this.socket.send(JSON.stringify({ method, id, ...(params ? { params } : {}) }))
    })
  }

  notify(method: string, params?: Record<string, unknown>): void {
    if (this.available) {
      this.socket.send(JSON.stringify({ method, ...(params ? { params } : {}) }))
    }
  }

  private closeWithError(error: Error): void {
    if (this.closed) {
      return
    }
    this.closed = true
    for (const waiter of this.pending.values()) {
      clearTimeout(waiter.timer)
      waiter.reject(error)
    }
    this.pending.clear()
  }

  dispose(): void {
    this.closeWithError(new Error('Codex live session bridge closed.'))
    try {
      this.socket.close()
    } catch {
      // Best effort; process-tree termination below is authoritative.
    }
    killCodexAppServerProcessTree(this.child)
  }
}
