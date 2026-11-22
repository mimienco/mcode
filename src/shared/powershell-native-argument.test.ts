import { describe, expect, it } from 'vitest'
import { quotePowerShellNativeArgument } from './powershell-native-argument'
import { quotePowerShellString } from './shell-quoting'

describe('PowerShell native argument quoting', () => {
  it('escapes literals for PowerShell source parsing', () => {
    expect(quotePowerShellString("WSL 'Preview'")).toBe("'WSL ''Preview'''")
  })

  it('pre-escapes embedded quotes for Windows native argv parsing', () => {
    expect(quotePowerShellNativeArgument('eval "decoded"')).toBe(String.raw`'eval \"decoded\"'`)
    expect(quotePowerShellNativeArgument(String.raw`before\"after`)).toBe(
      String.raw`'before\\\"after'`
    )
  })
})
