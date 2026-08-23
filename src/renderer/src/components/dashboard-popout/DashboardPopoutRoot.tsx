import { useEffect, useState } from 'react'
import { AgentKanbanBoard, type AgentDashboardView } from './AgentKanbanBoard'
import { useDashboardSnapshot } from './useDashboardSnapshot'
import { isDashboardView } from '../../../../shared/dashboard-snapshot'

type DashboardPopoutRootProps = {
  /** The layout requested via popout.html?view=<name>. */
  view: string | null
}

/**
 * Root of the pop-out dashboard window. Subscribes to the live snapshot relayed
 * from the main window and renders the requested layout.
 */
export function DashboardPopoutRoot(_props: DashboardPopoutRootProps): React.JSX.Element {
  const snapshot = useDashboardSnapshot()
  const [view, setView] = useState<AgentDashboardView>(() => {
    if (_props.view === 'map' || _props.view === 'rings') {
      return 'map'
    }
    if (isDashboardView(_props.view)) {
      return _props.view
    }
    return 'mission'
  })
  useEffect(() => window.api.dashboard.onViewRequested(setView), [])
  return <AgentKanbanBoard key={view} snapshot={snapshot} initialView={view} />
}
