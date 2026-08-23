import { useEffect, useMemo, useState } from 'react'
import { RotateCcw } from 'lucide-react'
import {
  findKeybindingConflictsForDefinitions,
  formatKeybindingList,
  getEffectiveKeybindingsForAction,
  getKeybindingDefinition,
  keybindingFromInputForAction,
  normalizeKeybindingListForAction,
  type KeybindingInput,
  type KeybindingOverrides
} from '../../../../shared/keybindings'
import { getShortcutPlatform } from '@/lib/shortcut-platform'
import { useAppStore } from '@/store'
import { useMountedRef } from '@/hooks/useMountedRef'
import { Button } from '../ui/button'
import { Label } from '../ui/label'
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/tooltip'
import {
  hasCommonBindingOverride,
  removeBindingOverride,
  sameBindings
} from './keybinding-override-edits'
import { ShortcutRecorderButton } from './ShortcutRecorderButton'
import { translate } from '@/i18n/i18n'
import { buildPluginCommandKeybindingDefinitions } from '@/lib/plugin-command-keybindings'
import { useEditablePluginCommands } from '@/store/plugin-panels'
import {
  disabledAgentTabActionIds,
  EMPTY_DISABLED_TUI_AGENTS,
  groupDefinitions
} from './shortcut-groups'

const ACTION_ID = 'voice.dictation'
const CONTROL_ID = 'voice-dictation-shortcut'
const ERROR_ID = 'voice-dictation-shortcut-error'
const platform = getShortcutPlatform()
const definition = getKeybindingDefinition(ACTION_ID)
if (!definition) {
  throw new Error('Missing voice dictation keybinding definition.')
}
const actionTitle = definition.title

function replacePrimaryBinding(current: readonly string[], binding: string): string[] {
  return current.length === 0 ? [binding] : [binding, ...current.slice(1)]
}

export function VoiceDictationShortcutSetting(): React.JSX.Element {
  const keybindings = useAppStore((state) => state.keybindings)
  const keybindingSnapshot = useAppStore((state) => state.keybindingSnapshot)
  const setKeybindingOverride = useAppStore((state) => state.setKeybindingOverride)
  const resetKeybindingOverride = useAppStore((state) => state.resetKeybindingOverride)
  const disabledTuiAgents = useAppStore(
    (state) => state.settings?.disabledTuiAgents ?? EMPTY_DISABLED_TUI_AGENTS
  )
  const pluginCommands = useEditablePluginCommands()
  const [recording, setRecording] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const mountedRef = useMountedRef()
  const effective = getEffectiveKeybindingsForAction(ACTION_ID, platform, keybindings)
  const defaults = getEffectiveKeybindingsForAction(ACTION_ID, platform, {})
  const modified = Object.hasOwn(keybindings, ACTION_ID)
  const { definitions, definitionsByAction, ignoredConflictActionIds, relevantConflictActionIds } =
    useMemo(() => {
      const pluginDefinitions = buildPluginCommandKeybindingDefinitions(pluginCommands)
      const nextDefinitions = groupDefinitions(disabledTuiAgents, pluginDefinitions).flatMap(
        (group) => group.items
      )
      return {
        definitions: nextDefinitions,
        definitionsByAction: new Map(
          nextDefinitions.map((candidateDefinition) => [
            candidateDefinition.id,
            candidateDefinition
          ])
        ),
        ignoredConflictActionIds: disabledAgentTabActionIds(disabledTuiAgents),
        relevantConflictActionIds: pluginDefinitions.map((pluginDefinition) => pluginDefinition.id)
      }
    }, [disabledTuiAgents, pluginCommands])

  useEffect(() => {
    window.api.ui.setShortcutRecorderFocused(recording)
    return () => window.api.ui.setShortcutRecorderFocused(false)
  }, [recording])

  const saveBindings = async (bindings: string[]): Promise<boolean> => {
    const normalized = normalizeKeybindingListForAction(ACTION_ID, bindings.join(', '))
    if (!Array.isArray(normalized)) {
      setError(normalized.ok ? 'Unable to parse shortcut.' : normalized.error)
      return false
    }

    const matchesDefault = sameBindings(normalized, defaults)
    const nextOverrides: KeybindingOverrides = matchesDefault
      ? removeBindingOverride(keybindings, ACTION_ID)
      : { ...keybindings, [ACTION_ID]: normalized }
    const conflict = findKeybindingConflictsForDefinitions(definitions, platform, nextOverrides, {
      ignoredActionIds: ignoredConflictActionIds,
      relevantActionIds: relevantConflictActionIds
    }).find((candidate) => candidate.actionIds.includes(ACTION_ID))
    if (conflict) {
      const labels = conflict.actionIds
        .filter((actionId) => actionId !== ACTION_ID)
        .map((actionId) => definitionsByAction.get(actionId)?.title ?? actionId)
        .join(', ')
      setError(`${formatKeybindingList([conflict.binding], platform)} conflicts with ${labels}.`)
      return false
    }

    setError(null)
    try {
      await (matchesDefault && !hasCommonBindingOverride(keybindingSnapshot, ACTION_ID)
        ? resetKeybindingOverride(ACTION_ID)
        : setKeybindingOverride(ACTION_ID, normalized))
      return true
    } catch (saveError) {
      if (mountedRef.current) {
        setError(saveError instanceof Error ? saveError.message : 'Failed to save shortcut.')
      }
      return false
    }
  }

  const capture = async (input: KeybindingInput): Promise<void> => {
    const captured = keybindingFromInputForAction(ACTION_ID, input, platform)
    if (!captured.ok) {
      setError(captured.error)
      return
    }
    if (
      (await saveBindings(replacePrimaryBinding(effective, captured.value))) &&
      mountedRef.current
    ) {
      setRecording(false)
    }
  }

  return (
    <div className="flex items-start justify-between gap-4 py-2">
      <div className="min-w-0 space-y-0.5">
        <Label htmlFor={CONTROL_ID}>
          {translate('auto.components.settings.VoicePane.dictationShortcut', 'Dictation Shortcut')}
        </Label>
        <p className="text-xs text-muted-foreground">
          {translate(
            'auto.components.settings.VoicePane.dictationShortcutDescription',
            'Select the current shortcut, then press a new key combination.'
          )}
        </p>
        {error ? (
          <p id={ERROR_ID} className="text-[11px] leading-4 text-destructive" aria-live="polite">
            {error}
          </p>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center gap-1">
        {modified ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                className="text-muted-foreground hover:text-foreground"
                aria-label={translate(
                  'auto.components.settings.VoicePane.resetDictationShortcut',
                  'Reset dictation shortcut to default'
                )}
                onClick={() => {
                  setError(null)
                  setRecording(false)
                  void (
                    hasCommonBindingOverride(keybindingSnapshot, ACTION_ID)
                      ? setKeybindingOverride(ACTION_ID, defaults)
                      : resetKeybindingOverride(ACTION_ID)
                  ).catch((resetError: unknown) => {
                    if (mountedRef.current) {
                      setError(
                        resetError instanceof Error
                          ? resetError.message
                          : 'Failed to reset shortcut.'
                      )
                    }
                  })
                }}
              >
                <RotateCcw className="size-3" />
              </Button>
            </TooltipTrigger>
            <TooltipContent side="top" sideOffset={4}>
              {translate(
                'auto.components.settings.ShortcutCommandBlock.9b02917027',
                'Reset to default'
              )}
            </TooltipContent>
          </Tooltip>
        ) : null}
        <ShortcutRecorderButton
          id={CONTROL_ID}
          actionId={ACTION_ID}
          title={actionTitle}
          platform={platform}
          isDigitIndex={false}
          binding={effective[0] ?? null}
          bindingIndex={0}
          bindingCount={Math.max(effective.length, 1)}
          recording={recording}
          ariaInvalid={error !== null}
          ariaDescribedBy={error ? ERROR_ID : undefined}
          onStartRecording={() => {
            setError(null)
            setRecording(true)
          }}
          onCancelRecording={() => setRecording(false)}
          onCapture={(_actionId, input) => void capture(input)}
          onClearError={() => setError(null)}
        />
      </div>
    </div>
  )
}
