import { build } from 'esbuild'
import { describe, expect, it } from 'vitest'

describe('shell quoting browser boundary', () => {
  it('bundles for renderer consumers without Node built-ins', async () => {
    await expect(
      build({
        bundle: true,
        format: 'esm',
        logLevel: 'silent',
        platform: 'browser',
        stdin: {
          contents: "import './src/shared/shell-quoting.ts'",
          resolveDir: process.cwd()
        },
        write: false
      })
    ).resolves.toBeDefined()
  })
})
