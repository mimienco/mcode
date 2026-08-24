import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { AUTOMATION_EDITOR_SECTION_LABEL_CLASS, Field } from './automation-page-parts'
import type { AutomationDraft } from './AutomationEditorDialog'
import { translate } from '@/i18n/i18n'

type AutomationSafetyRailFieldsProps = {
  draft: AutomationDraft
  disabled: boolean
  onDraftChange: (updater: (current: AutomationDraft) => AutomationDraft) => void
}

export function AutomationSafetyRailFields({
  draft,
  disabled,
  onDraftChange
}: AutomationSafetyRailFieldsProps): React.JSX.Element {
  return (
    <>
      <Field
        labelClassName={AUTOMATION_EDITOR_SECTION_LABEL_CLASS}
        label={translate(
          'auto.components.automations.AutomationSafetyRailFields.costCap.label',
          'Cost cap per run'
        )}
      >
        <Input
          type="number"
          min="0"
          step="0.01"
          value={draft.maxCostUsdPerRun}
          disabled={disabled}
          placeholder={translate(
            'auto.components.automations.AutomationSafetyRailFields.costCap.placeholder',
            'Unlimited'
          )}
          onChange={(event) =>
            onDraftChange((current) => ({
              ...current,
              maxCostUsdPerRun: event.target.value
            }))
          }
        />
      </Field>
      <div className="flex items-center gap-2">
        <Checkbox
          id="automation-auto-push-enabled"
          checked={draft.autoPushEnabled}
          disabled={disabled}
          onCheckedChange={(checked) =>
            onDraftChange((current) => ({ ...current, autoPushEnabled: checked === true }))
          }
        />
        <Label
          htmlFor="automation-auto-push-enabled"
          className="text-xs font-normal text-muted-foreground"
        >
          {translate(
            'auto.components.automations.AutomationSafetyRailFields.autoPush.label',
            'Agent may push and open PRs'
          )}
        </Label>
      </div>
    </>
  )
}
