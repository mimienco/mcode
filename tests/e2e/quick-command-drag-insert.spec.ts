import { expect, test } from './helpers/mcode-app'
import { ensureTerminalVisible, waitForSessionReady } from './helpers/store'
import {
  getTerminalContent,
  sendToTerminal,
  waitForActivePanePtyId,
  waitForActiveTerminalManager
} from './helpers/terminal'
import { waitForPtyShellEcho } from './terminal-pty-readiness'

const COMMAND_LABEL = 'E2E drag command'
const COMMAND_TEXT = 'echo QUICK_DROP_EXECUTED'

test('drags a saved quick command into the active terminal without executing it', async ({
  mcodePage
}) => {
  await waitForSessionReady(mcodePage)
  await ensureTerminalVisible(mcodePage)
  await waitForActiveTerminalManager(mcodePage, 30_000)
  const ptyId = await waitForActivePanePtyId(mcodePage)
  await waitForPtyShellEcho(mcodePage, ptyId, 15_000)

  await mcodePage.evaluate(
    ({ label, text }) => {
      const store = window.__store
      const settings = store?.getState().settings
      if (!store || !settings) {
        throw new Error('settings store unavailable')
      }
      store.setState({
        settings: {
          ...settings,
          terminalQuickCommands: [
            {
              id: 'e2e-quick-command-drag',
              label,
              action: 'terminal-command',
              command: text,
              appendEnter: true,
              scope: { type: 'global' }
            }
          ]
        }
      })
    },
    { label: COMMAND_LABEL, text: COMMAND_TEXT }
  )

  await mcodePage.getByRole('button', { name: 'More quick commands' }).click()
  const savedCommand = mcodePage.getByRole('option', { name: `Insert ${COMMAND_LABEL}` })
  await expect(savedCommand).toBeVisible()
  const terminalTabId = await mcodePage.evaluate(() => window.__store?.getState().activeTabId)
  if (!terminalTabId) {
    throw new Error('active terminal tab unavailable')
  }
  const terminalRoot = mcodePage.locator(
    `[data-terminal-tab-id="${terminalTabId}"][data-terminal-layout-leaf-ids]`
  )
  await savedCommand.dragTo(terminalRoot.locator('.xterm-screen'))
  await expect(savedCommand).toBeHidden()
  await expect
    .poll(async () => countOccurrences(await getTerminalContent(mcodePage), 'QUICK_DROP_EXECUTED'))
    .toBe(1)

  await sendToTerminal(mcodePage, ptyId, '\r')
  await expect
    .poll(async () => countOccurrences(await getTerminalContent(mcodePage), 'QUICK_DROP_EXECUTED'))
    .toBeGreaterThanOrEqual(2)
})

function countOccurrences(value: string, needle: string): number {
  return value.split(needle).length - 1
}
