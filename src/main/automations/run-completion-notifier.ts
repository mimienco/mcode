import type { Store } from '../persistence'
import { createNotificationDispatchHandler } from '../ipc/notifications'
import type { MCodeRuntimeService } from '../runtime/mcode-runtime'
import type { AutomationRun } from '../../shared/automations-types'

// Why: single factory so main/index.ts stays a two-line wiring change and so
// burst cooldowns apply across back-to-back automation finalizations. Desktop
// notification + paired-phone fanout ride the shared dispatch handler gates.
export function createAutomationRunCompletionNotifier(
  store: Store,
  runtime: MCodeRuntimeService
): (run: AutomationRun) => void {
  const dispatchNotification = createNotificationDispatchHandler(store, runtime)
  return (run) => {
    const automationName =
      store.listAutomations().find((entry) => entry.id === run.automationId)?.name ?? 'Automation'
    void dispatchNotification({
      source: 'automation-run-complete',
      automationName,
      automationOutcome:
        run.status === 'completed'
          ? 'completed'
          : run.status === 'dispatch_failed' || run.status === 'stopped_cost_cap'
            ? 'failed'
            : 'skipped',
    })
  }
}
