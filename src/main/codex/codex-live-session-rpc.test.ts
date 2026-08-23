/**
 * Regression test for BH-RUNTIME-001: a Codex App Server endpoint that accepts
 * TCP but never answers the WebSocket upgrade fires neither 'open' nor 'error',
 * which previously hung connect() past the startup deadline and prevented the
 * PTY spawn fallback from ever being reached.
 */
import { createServer, type Server } from 'node:http'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { CodexLiveSessionRpc } from './codex-live-session-rpc'

describe('CodexLiveSessionRpc.connectSocket upgrade deadline', () => {
  let server: Server | null = null
  const hungSockets: { destroy: () => void }[] = []

  afterEach(async () => {
    vi.restoreAllMocks()
    for (const s of hungSockets.splice(0)) {
      s.destroy()
    }
    if (server) {
      await new Promise<void>((resolve) => server!.close(() => resolve()))
      server = null
    }
  })

  it('rejects with a timeout when the peer accepts TCP but never upgrades', async () => {
    // Accept the connection and swallow the upgrade request without replying.
    server = createServer()
    server.on('upgrade', (_req, socket) => {
      hungSockets.push(socket)
      // Intentionally never write a 101 response.
    })
    await new Promise<void>((resolve) => server!.listen(0, '127.0.0.1', () => resolve()))
    const address = server.address()
    if (!address || typeof address === 'string') {
      throw new Error('expected an address object')
    }
    const endpoint = `ws://127.0.0.1:${address.port}`

    // The fake app-server child must look alive so the retry loop runs.
    const child = {
      exitCode: null,
      stdout: { resume: () => undefined },
      stderr: { resume: () => undefined },
      kill: vi.fn()
    }

    const start = Date.now()
    await expect(
      CodexLiveSessionRpc.connect({
        appServerArgv: ['echo'],
        endpoint,
        startTimeoutMs: 150
      })
    ).rejects.toThrow(/timed out|did not become ready/)
    const elapsed = Date.now() - start

    // Why: the old code hung forever here; bound the wait to prove the
    // deadline is honored (150ms budget + one 75ms retry delay).
    expect(elapsed).toBeLessThan(2_000)
    expect(hungSockets.length).toBeGreaterThan(0)
    void child
  })
})
