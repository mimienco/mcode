import assert from 'node:assert/strict'
import test from 'node:test'
import { createDesktopClient } from './desktop-client.ts'

test('launch context uses the narrow Tauri command and preserves its typed result', async () => {
  const calls: { command: string; args?: Record<string, unknown> }[] = []
  const expected = {
    version: '1.4.178',
    buildLane: 'development' as const,
    headless: false,
    safeMode: false
  }
  const client = createDesktopClient(async (command, args) => {
    calls.push({ command, args })
    return expected
  })

  const context = await client.app.getLaunchContext()

  assert.deepEqual(calls, [{ command: 'app_get_launch_context', args: undefined }])
  assert.deepEqual(context, expected)
})
