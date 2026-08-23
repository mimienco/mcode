import type {
  CodexLiveSessionModel,
  CodexLiveSessionOptionsSnapshot,
  CodexLiveSessionOptionsUpdate,
  CodexLiveSessionOptionsUpdateResult
} from '../../shared/codex-live-session-options'
import { parseCodexLiveModels } from './codex-live-session-planning'
import { CodexLiveSessionRpc } from './codex-live-session-rpc'

const THREAD_TIMEOUT_MS = 8_000

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

export class CodexLiveSessionConnection {
  private threadId: string | null = null
  private current: { model: string; effort?: string } | null = null
  private subscribed = false
  private subscribeInFlight: Promise<void> | null = null
  private settingsUpdateSupported: boolean | null = null
  private capabilityProbeInFlight: Promise<boolean> | null = null
  private readonly settingsNotificationWaiters = new Set<
    (current: { model: string; effort?: string }) => void
  >()

  private constructor(
    private readonly rpc: CodexLiveSessionRpc,
    public readonly models: CodexLiveSessionModel[]
  ) {
    rpc.onNotification((method, params) => this.handleNotification(method, params))
  }

  static async start(args: {
    appServerArgv: string[]
    endpoint: string
    env?: Record<string, string>
    cwd?: string
    clientVersion: string
  }): Promise<CodexLiveSessionConnection> {
    const rpc = await CodexLiveSessionRpc.connect(args)
    try {
      await rpc.request('initialize', {
        clientInfo: { name: 'mcode', title: 'MCode', version: args.clientVersion },
        capabilities: { experimentalApi: true }
      })
      rpc.notify('initialized')
      const models: CodexLiveSessionModel[] = []
      let cursor: string | null = null
      do {
        const response = await rpc.request('model/list', {
          limit: 100,
          includeHidden: false,
          ...(cursor ? { cursor } : {})
        })
        models.push(...parseCodexLiveModels(response))
        cursor =
          response &&
          typeof response === 'object' &&
          typeof (response as { nextCursor?: unknown }).nextCursor === 'string'
            ? (response as { nextCursor: string }).nextCursor || null
            : null
      } while (cursor)
      if (models.length === 0) {
        throw new Error('Codex App Server returned no selectable models.')
      }
      return new CodexLiveSessionConnection(rpc, models)
    } catch (error) {
      rpc.dispose()
      throw error
    }
  }

  private handleNotification(method: string, value: unknown): void {
    if (method !== 'thread/settings/updated') {
      return
    }
    const params = value as
      | { threadId?: unknown; threadSettings?: { model?: unknown; effort?: unknown } }
      | undefined
    if (params?.threadId === this.threadId && typeof params.threadSettings?.model === 'string') {
      this.current = {
        model: params.threadSettings.model,
        ...(typeof params.threadSettings.effort === 'string'
          ? { effort: params.threadSettings.effort }
          : {})
      }
      for (const resolve of this.settingsNotificationWaiters) {
        resolve({ ...this.current })
      }
      this.settingsNotificationWaiters.clear()
    }
  }

  private async loadedThreadId(timeoutMs = THREAD_TIMEOUT_MS): Promise<string> {
    if (this.threadId) {
      return this.threadId
    }
    const deadline = Date.now() + timeoutMs
    while (Date.now() < deadline) {
      const response = await this.rpc.request('thread/loaded/list', { limit: 20 })
      const data =
        response &&
        typeof response === 'object' &&
        Array.isArray((response as { data?: unknown }).data)
          ? (response as { data: unknown[] }).data.filter(
              (id): id is string => typeof id === 'string'
            )
          : []
      if (data.length > 0) {
        // One dedicated App Server backs one root TUI. Capture its first loaded
        // thread before later subagent threads can appear.
        this.threadId = data[0]
        void this.trySubscribe()
        return this.threadId
      }
      await delay(100)
    }
    throw new Error('Codex thread did not become available.')
  }

  private trySubscribe(): Promise<void> {
    if (this.subscribed || !this.threadId) {
      return Promise.resolve()
    }
    if (this.subscribeInFlight) {
      return this.subscribeInFlight
    }
    const threadId = this.threadId
    const pending = this.rpc
      .request('thread/resume', {
        threadId,
        // Experimental but capability-gated with settings/update. Avoid
        // retransferring a long transcript merely to subscribe to settings.
        excludeTurns: true
      })
      .then((resumed) => {
        const record =
          resumed && typeof resumed === 'object' ? (resumed as Record<string, unknown>) : null
        if (record && typeof record.model === 'string') {
          this.current = {
            model: record.model,
            ...(typeof record.reasoningEffort === 'string'
              ? { effort: record.reasoningEffort }
              : {})
          }
        }
        this.subscribed = true
      })
      .catch(() => undefined)
      .finally(() => {
        if (this.subscribeInFlight === pending) {
          this.subscribeInFlight = null
        }
      })
    this.subscribeInFlight = pending
    return pending
  }

  private ensureSettingsUpdateSupported(): Promise<boolean> {
    if (this.settingsUpdateSupported !== null) {
      return Promise.resolve(this.settingsUpdateSupported)
    }
    if (this.capabilityProbeInFlight) {
      return this.capabilityProbeInFlight
    }
    const pending = this.loadedThreadId()
      .then((threadId) => this.rpc.request('thread/settings/update', { threadId }))
      .then(() => {
        this.settingsUpdateSupported = true
        return true
      })
      .catch(() => {
        this.settingsUpdateSupported = false
        return false
      })
      .finally(() => {
        if (this.capabilityProbeInFlight === pending) {
          this.capabilityProbeInFlight = null
        }
      })
    this.capabilityProbeInFlight = pending
    return pending
  }

  private waitForSettingsNotification(timeoutMs = 750): Promise<{
    model: string
    effort?: string
  } | null> {
    return new Promise((resolve) => {
      const settle = (current: { model: string; effort?: string }): void => {
        clearTimeout(timer)
        this.settingsNotificationWaiters.delete(settle)
        resolve(current)
      }
      const timer = setTimeout(() => {
        this.settingsNotificationWaiters.delete(settle)
        resolve(null)
      }, timeoutMs)
      this.settingsNotificationWaiters.add(settle)
    })
  }

  snapshot(): CodexLiveSessionOptionsSnapshot {
    return !this.rpc.available
      ? { available: false }
      : {
          available: true,
          models: [...this.models],
          ...(this.current ? { current: { ...this.current } } : {})
        }
  }

  async readySnapshot(): Promise<CodexLiveSessionOptionsSnapshot> {
    if (!this.rpc.available) {
      return { available: false }
    }
    try {
      await this.loadedThreadId(350)
      if (!(await this.ensureSettingsUpdateSupported())) {
        return { available: false }
      }
      void this.trySubscribe()
      return this.snapshot()
    } catch {
      return { available: false }
    }
  }

  async update(
    update: CodexLiveSessionOptionsUpdate
  ): Promise<CodexLiveSessionOptionsUpdateResult> {
    if (!this.rpc.available) {
      return { outcome: 'unavailable' }
    }
    if (!(await this.ensureSettingsUpdateSupported())) {
      return { outcome: 'unavailable' }
    }
    const selectedModel = update.model
      ? this.models.find((model) => model.id === update.model)
      : undefined
    if (update.model && !selectedModel) {
      return { outcome: 'unavailable' }
    }
    const effort = update.effort ?? selectedModel?.defaultEffort
    if (
      selectedModel &&
      effort &&
      selectedModel.efforts.length > 0 &&
      !selectedModel.efforts.some((entry) => entry.id === effort)
    ) {
      return { outcome: 'unavailable' }
    }
    try {
      const threadId = await this.loadedThreadId()
      const expectsNotification =
        this.subscribed &&
        ((update.model !== undefined && update.model !== this.current?.model) ||
          (effort !== undefined && effort !== this.current?.effort))
      const effectiveSettings = expectsNotification ? this.waitForSettingsNotification() : null
      await this.rpc.request('thread/settings/update', {
        threadId,
        ...(update.model ? { model: update.model } : {}),
        ...(effort ? { effort } : {})
      })
      const confirmed = await effectiveSettings
      if (confirmed) {
        return {
          outcome: 'applied',
          model: confirmed.model,
          ...(confirmed.effort ? { effort: confirmed.effort } : {})
        }
      }
      // A no-op emits no notification. When this alpha cannot subscribe until
      // the first rollout exists, the accepted queue response is the strongest
      // available confirmation and still guarantees next-turn settings.
      const currentModel = update.model ?? this.current?.model
      if (currentModel) {
        this.current = {
          model: currentModel,
          ...(effort ? { effort } : this.current?.effort ? { effort: this.current.effort } : {})
        }
      }
      return {
        outcome: 'applied',
        ...(update.model ? { model: update.model } : {}),
        ...(effort ? { effort } : {})
      }
    } catch {
      // Treat an accepted handshake/model catalog but rejected settings method
      // as a per-session capability failure. Subsequent polls expose the honest
      // Terminal picker instead of leaving stale clickable rows active.
      this.settingsUpdateSupported = false
      return { outcome: 'unavailable' }
    }
  }

  dispose(): void {
    this.rpc.dispose()
  }
}
