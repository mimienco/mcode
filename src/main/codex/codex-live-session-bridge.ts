import { createServer } from 'node:net'
import type { AgentStartupShell } from '../../shared/tui-agent-startup-shell'
import type {
  CodexLiveSessionOptionsSnapshot,
  CodexLiveSessionOptionsUpdate,
  CodexLiveSessionOptionsUpdateResult
} from '../../shared/codex-live-session-options'
import { CodexLiveSessionConnection } from './codex-live-session-connection'
import { planCodexRemoteLaunch } from './codex-live-session-planning'

export {
  parseCodexLiveModels,
  planCodexRemoteLaunch,
  type CodexRemoteLaunchPlan
} from './codex-live-session-planning'

function reserveLoopbackPort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = createServer()
    server.unref()
    server.once('error', reject)
    server.listen(0, '127.0.0.1', () => {
      const address = server.address()
      const port = address && typeof address === 'object' ? address.port : 0
      server.close((error) => (error ? reject(error) : resolve(port)))
    })
  })
}

class CodexLiveSessionBridgeRegistry {
  private readonly sessions = new Map<string, CodexLiveSessionConnection>()

  async prepare(args: {
    ptyId: string
    command: string
    shell: AgentStartupShell
    env?: Record<string, string>
    cwd?: string
    clientVersion: string
  }): Promise<{ command: string } | null> {
    const port = await reserveLoopbackPort()
    const endpoint = `ws://127.0.0.1:${port}`
    const plan = planCodexRemoteLaunch({ command: args.command, shell: args.shell, endpoint })
    if (!plan) {
      return null
    }
    try {
      const connection = await CodexLiveSessionConnection.start({
        appServerArgv: plan.appServerArgv,
        endpoint,
        env: args.env,
        cwd: args.cwd,
        clientVersion: args.clientVersion
      })
      this.dispose(args.ptyId)
      this.sessions.set(args.ptyId, connection)
      return { command: plan.tuiCommand }
    } catch {
      return null
    }
  }

  adopt(fromPtyId: string, toPtyId: string): void {
    if (fromPtyId === toPtyId) {
      return
    }
    const connection = this.sessions.get(fromPtyId)
    if (!connection) {
      return
    }
    this.sessions.delete(fromPtyId)
    this.dispose(toPtyId)
    this.sessions.set(toPtyId, connection)
  }

  snapshot(ptyId: string): CodexLiveSessionOptionsSnapshot {
    return this.sessions.get(ptyId)?.snapshot() ?? { available: false }
  }

  readySnapshot(ptyId: string): Promise<CodexLiveSessionOptionsSnapshot> {
    return this.sessions.get(ptyId)?.readySnapshot() ?? Promise.resolve({ available: false })
  }

  update(
    ptyId: string,
    update: CodexLiveSessionOptionsUpdate
  ): Promise<CodexLiveSessionOptionsUpdateResult> {
    return this.sessions.get(ptyId)?.update(update) ?? Promise.resolve({ outcome: 'unavailable' })
  }

  dispose(ptyId: string): void {
    this.sessions.get(ptyId)?.dispose()
    this.sessions.delete(ptyId)
  }
}

export const codexLiveSessionBridgeRegistry = new CodexLiveSessionBridgeRegistry()
