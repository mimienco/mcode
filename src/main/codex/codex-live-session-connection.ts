import type {
  CodexLiveSessionModel,
  CodexLiveSessionOptionsSnapshot,
  CodexLiveSessionOptionsUpdate,
  CodexLiveSessionOptionsUpdateResult
} from '../../shared/codex-live-session-options'
import type { AgentInteractionMode, AgentPermissionPreset } from '../../shared/agent-run-profile'
import { parseCodexThreadRunProfile } from './codex-live-session-controls'
import {
  codexRunProfileControls,
  loadCodexLiveSessionCatalog,
  prepareCodexLiveSessionUpdate,
  type CodexLiveSessionCatalog
} from './codex-live-session-catalog'
import { CodexLiveSessionRpc } from './codex-live-session-rpc'

const THREAD_TIMEOUT_MS = 8_000

type CodexLiveCurrentSettings = {
  model: string
  effort?: string
  permission?: AgentPermissionPreset
  interactionMode?: AgentInteractionMode
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

export class CodexLiveSessionConnection {
  private threadId: string | null = null
  private current: CodexLiveCurrentSettings | null = null
  private subscribed = false
  private subscribeInFlight: Promise<void> | null = null
  private settingsUpdateSupported: boolean | null = null
  private capabilityProbeInFlight: Promise<boolean> | null = null
  private readonly settingsNotificationWaiters = new Set<
    (current: CodexLiveCurrentSettings) => void
  >()

  private constructor(
    private readonly rpc: CodexLiveSessionRpc,
    private readonly catalog: CodexLiveSessionCatalog
  ) {
    rpc.onNotification((method, params) => this.handleNotification(method, params))
  }

  get models(): CodexLiveSessionModel[] {
    return this.catalog.models
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
      const catalog = await loadCodexLiveSessionCatalog(rpc, args.cwd)
      return new CodexLiveSessionConnection(rpc, catalog)
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
      | { threadId?: unknown; threadSettings?: Record<string, unknown> }
      | undefined
    if (params?.threadId === this.threadId && typeof params.threadSettings?.model === 'string') {
      const runProfile = parseCodexThreadRunProfile(params.threadSettings)
      this.current = {
        model: params.threadSettings.model,
        ...(typeof params.threadSettings.effort === 'string'
          ? { effort: params.threadSettings.effort }
          : {}),
        ...runProfile
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
          const runProfile = parseCodexThreadRunProfile(record.threadSettings ?? record)
          this.current = {
            model: record.model,
            ...(typeof record.reasoningEffort === 'string'
              ? { effort: record.reasoningEffort }
              : {}),
            ...runProfile
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
    permission?: AgentPermissionPreset
    interactionMode?: AgentInteractionMode
  } | null> {
    return new Promise((resolve) => {
      const settle = (current: CodexLiveCurrentSettings): void => {
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

  // Why: before the first turn the Codex TUI already prints a model+effort
  // banner — the catalog's flagged default with its default reasoning effort.
  // Surfacing the same values keeps the chat composer's pills honest with the
  // terminal banner instead of showing bare "Model"/"Effort" until a thread
  // exists for the settings subscription.
  private catalogCurrent(): CodexLiveCurrentSettings | null {
    const model = this.catalog.models.find((entry) => entry.isDefault) ?? this.catalog.models[0]
    return model ? { model: model.id, effort: model.defaultEffort } : null
  }

  snapshot(): CodexLiveSessionOptionsSnapshot {
    if (!this.rpc.available) {
      return { available: false }
    }
    const current = this.current ?? this.catalogCurrent()
    return {
      available: true,
      models: [...this.models],
      controls: codexRunProfileControls(this.catalog),
      ...(current ? { current: { ...current } } : {})
    }
  }

  async readySnapshot(): Promise<CodexLiveSessionOptionsSnapshot> {
    if (!this.rpc.available) {
      return { available: false }
    }
    // Why: a pre-thread poll (fresh "Start a chat" pane) must not fail the whole
    // snapshot — the catalog already knows the default model/effort the TUI
    // banner prints. Probe settings support only once a thread exists: the
    // probe itself needs a thread, and a premature false result is cached
    // permanently, demoting the session to command fallbacks forever.
    const threadReady = await this.loadedThreadId(350).then(
      () => true,
      () => false
    )
    if (!threadReady) {
      return this.snapshot()
    }
    if (!(await this.ensureSettingsUpdateSupported())) {
      return { available: false }
    }
    void this.trySubscribe()
    return this.snapshot()
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
    // Why: loadedThreadId starts this subscription without awaiting it. A fast
    // picker change could otherwise be accepted and echoed by Codex before the
    // confirmation waiter exists, making the UI falsely demote the session to
    // the Terminal-only fallback.
    await this.trySubscribe()
    const prepared = prepareCodexLiveSessionUpdate({
      catalog: this.catalog,
      update,
      current: this.current
    })
    if (!prepared) {
      return { outcome: 'unavailable' }
    }
    const { currentModel, effort, runProfileUpdate, accepted } = prepared
    try {
      const threadId = await this.loadedThreadId()
      const changesEffectiveSettings =
        (update.model !== undefined && update.model !== this.current?.model) ||
        (effort !== undefined && effort !== this.current?.effort) ||
        (update.permission !== undefined && update.permission !== this.current?.permission) ||
        (update.interactionMode !== undefined &&
          update.interactionMode !== this.current?.interactionMode)
      const effectiveSettings =
        changesEffectiveSettings && this.subscribed ? this.waitForSettingsNotification() : null
      await this.rpc.request('thread/settings/update', {
        threadId,
        ...(update.interactionMode ? {} : update.model ? { model: update.model } : {}),
        ...(update.interactionMode ? {} : effort ? { effort } : {}),
        ...runProfileUpdate
      })
      const confirmed = await effectiveSettings
      if (confirmed) {
        return {
          outcome: 'applied',
          model: confirmed.model,
          ...(confirmed.effort ? { effort: confirmed.effort } : {}),
          ...(confirmed.permission ? { permission: confirmed.permission } : {}),
          ...(confirmed.interactionMode ? { interactionMode: confirmed.interactionMode } : {})
        }
      }
      if (changesEffectiveSettings && !this.subscribed) {
        // Before the first turn Codex has a loaded in-memory thread but no
        // rollout file, so thread/resume cannot subscribe and no settings echo
        // is available. A fulfilled settings/update is the provider's only
        // confirmation in that state; the TUI has already applied the change.
        this.current = { ...accepted }
        return { outcome: 'applied', ...accepted }
      }
      // An accepted RPC only proves the update was queued. Changed settings
      // require the provider's effective-state notification before the UI can
      // claim success when a subscription exists; a genuine no-op does not
      // emit one.
      if (changesEffectiveSettings) {
        return { outcome: 'unavailable' }
      }
      return {
        outcome: 'applied',
        ...(this.current?.model ? { model: this.current.model } : { model: currentModel }),
        ...(this.current?.effort ? { effort: this.current.effort } : {}),
        ...(this.current?.permission ? { permission: this.current.permission } : {}),
        ...(this.current?.interactionMode ? { interactionMode: this.current.interactionMode } : {})
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
