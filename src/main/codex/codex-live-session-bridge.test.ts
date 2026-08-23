import { describe, expect, it } from 'vitest'
import * as nodePty from 'node-pty'
import {
  codexLiveSessionBridgeRegistry,
  parseCodexLiveModels,
  planCodexRemoteLaunch
} from './codex-live-session-bridge'

describe('Codex live session bridge planning', () => {
  it('adds --remote to the TUI while reusing the exact executable for app-server', () => {
    expect(
      planCodexRemoteLaunch({
        command: "'C:\\Program Files\\Codex\\codex.exe' --no-alt-screen",
        shell: 'powershell',
        endpoint: 'ws://127.0.0.1:45123'
      })
    ).toEqual({
      appServerArgv: [
        'C:\\Program Files\\Codex\\codex.exe',
        'app-server',
        '--listen',
        'ws://127.0.0.1:45123'
      ],
      tuiCommand:
        "& 'C:\\Program Files\\Codex\\codex.exe' '--remote' 'ws://127.0.0.1:45123' '--no-alt-screen'"
    })
  })

  it('supports the npm node entrypoint shape and rejects shell expressions', () => {
    expect(
      planCodexRemoteLaunch({
        command: "node 'C:\\tools\\codex.js' --model gpt-5.5",
        shell: 'powershell',
        endpoint: 'ws://127.0.0.1:45123'
      })?.appServerArgv
    ).toEqual(['node', 'C:\\tools\\codex.js', 'app-server', '--listen', 'ws://127.0.0.1:45123'])
    expect(
      planCodexRemoteLaunch({
        command: 'codex; Write-Host nope',
        shell: 'powershell',
        endpoint: 'ws://127.0.0.1:45123'
      })
    ).toBeNull()
  })

  it('shares config globals with app-server but keeps TUI-only flags on the TUI', () => {
    expect(
      planCodexRemoteLaunch({
        command:
          "codex -c 'features.apps=true' --profile work --strict-config --model gpt-5.6-sol --no-alt-screen",
        shell: 'powershell',
        endpoint: 'ws://127.0.0.1:45123'
      })
    ).toEqual({
      appServerArgv: [
        'codex',
        '-c',
        'features.apps=true',
        '--profile',
        'work',
        '--strict-config',
        'app-server',
        '--listen',
        'ws://127.0.0.1:45123'
      ],
      tuiCommand:
        "& 'codex' '--remote' 'ws://127.0.0.1:45123' '-c' 'features.apps=true' '--profile' 'work' '--strict-config' '--model' 'gpt-5.6-sol' '--no-alt-screen'"
    })
  })

  it('parses only visible account models and preserves effort order', () => {
    expect(
      parseCodexLiveModels({
        data: [
          {
            model: 'gpt-5.6-sol',
            displayName: 'GPT-5.6 Sol',
            description: 'Frontier',
            hidden: false,
            isDefault: true,
            defaultReasoningEffort: 'medium',
            supportedReasoningEfforts: [
              { reasoningEffort: 'low', description: 'Fast' },
              { reasoningEffort: 'medium', description: 'Balanced' },
              { reasoningEffort: 'high', description: 'Deep' }
            ]
          },
          {
            model: 'hidden-model',
            displayName: 'Hidden',
            hidden: true,
            defaultReasoningEffort: 'medium',
            supportedReasoningEfforts: []
          }
        ]
      })
    ).toEqual([
      {
        id: 'gpt-5.6-sol',
        label: 'GPT-5.6 Sol',
        description: 'Frontier',
        efforts: [
          { id: 'low', label: 'Low', description: 'Fast' },
          { id: 'medium', label: 'Medium', description: 'Balanced' },
          { id: 'high', label: 'High', description: 'Deep' }
        ],
        defaultEffort: 'medium',
        isDefault: true
      }
    ])
  })

  it.skipIf(process.env.MCODE_LIVE_CODEX_BRIDGE_TEST !== '1')(
    'smoke-tests the installed Codex App Server and account catalog',
    async () => {
      const ptyId = 'live-codex-bridge-smoke'
      let terminal: nodePty.IPty | null = null
      let terminalOutput = ''
      try {
        const prepared = await codexLiveSessionBridgeRegistry.prepare({
          ptyId,
          command: 'codex',
          shell: process.platform === 'win32' ? 'powershell' : 'posix',
          cwd: process.cwd(),
          clientVersion: 'test'
        })
        expect(prepared?.command).toContain('--remote')
        const snapshot = codexLiveSessionBridgeRegistry.snapshot(ptyId)
        expect(snapshot.available && snapshot.models.length).toBeGreaterThan(0)
        if (!prepared || !snapshot.available) {
          return
        }
        terminal = nodePty.spawn(
          process.platform === 'win32' ? 'powershell.exe' : '/bin/sh',
          process.platform === 'win32'
            ? ['-NoLogo', '-NoProfile', '-Command', prepared.command]
            : ['-lc', prepared.command],
          {
            cwd: process.cwd(),
            env: { ...(process.env as Record<string, string>), TERM: 'xterm-256color' },
            cols: 100,
            rows: 30
          }
        )
        terminal.onData((data) => {
          terminalOutput = `${terminalOutput}${data}`.slice(-16_384)
        })
        const startupDeadline = Date.now() + 4_000
        while (Date.now() < startupDeadline && !terminalOutput.includes('Hooks need review')) {
          await new Promise((resolve) => setTimeout(resolve, 50))
        }
        if (terminalOutput.includes('Hooks need review')) {
          // Live smoke only: do not mutate trust state; continue with hooks disabled.
          terminal.write('3\r')
        }
        const model = snapshot.models[0]
        const update = await codexLiveSessionBridgeRegistry.update(ptyId, {
          model: model.id,
          effort: model.defaultEffort
        })
        expect(update, terminalOutput).toMatchObject({ outcome: 'applied', model: model.id })
        expect(codexLiveSessionBridgeRegistry.snapshot(ptyId)).toMatchObject({
          available: true,
          current: { model: model.id, effort: model.defaultEffort }
        })
      } finally {
        terminal?.kill()
        codexLiveSessionBridgeRegistry.dispose(ptyId)
      }
    },
    15_000
  )
})
