import type { CatalogModel } from '../../../../shared/agent-session-option-catalog'
import { stripAnsiEscapeSequences } from '../../../../shared/ansi-escape-sequences'
import type { SessionOptionValue } from '../../../../shared/native-chat-session-options'

const OMP_STATUS_LINE = /\[(off|minimal|low|medium|high|xhigh|max|auto)\]\s+(.+?)(?:\s+>\s+|$)/i

function normalizedModelName(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, '')
}

/** Recover the model and thinking level already shown by OMP's status line so
 * Native Chat reflects the live session instead of guessing from launch defaults. */
export function readOmpSessionOptionsFromTerminalScreen(
  screen: string | null | undefined,
  models: readonly CatalogModel[]
): Record<string, SessionOptionValue> | null {
  if (!screen) {
    return null
  }
  const lines = stripAnsiEscapeSequences(screen).split(/\r?\n/)
  for (let index = lines.length - 1; index >= 0; index -= 1) {
    const match = OMP_STATUS_LINE.exec(lines[index] ?? '')
    if (!match) {
      continue
    }
    const effort = match[1]!.toLowerCase()
    const displayedModel = normalizedModelName(match[2]!.trim())
    const model = models.find((candidate) => {
      const basename = candidate.id.split('/').at(-1) ?? candidate.id
      return (
        normalizedModelName(candidate.label) === displayedModel ||
        normalizedModelName(basename) === displayedModel ||
        normalizedModelName(candidate.id) === displayedModel
      )
    })
    return { ...(model ? { model: model.id } : {}), effort }
  }
  return null
}
