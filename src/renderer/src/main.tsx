// Why: must run before any component touches window.api — under Tauri there is no
// Electron preload, so the clipboard bridge is installed over a partial shim.
import { installTauriWindowApiShim } from './desktop/tauri-window-api-shim'

installTauriWindowApiShim()

import './assets/main.css'

import { StrictMode } from 'react'
import { useTranslation } from 'react-i18next'
import App from './App'
import { RecoverableRenderErrorBoundary } from './components/error-boundaries/RecoverableRenderErrorBoundary'
import {
  installRendererCrashDiagnostics,
  recordRendererCrashBreadcrumb
} from './lib/crash-diagnostics'
import { applyDocumentTheme } from './lib/document-theme'
import { installTypingLatencyDiagnostic } from './lib/typing-latency-diagnostic'
import { shouldEnableReactGrab } from './lib/react-grab-dev-gate'
import { I18nProvider } from './i18n/I18nProvider'
import { translate } from './i18n/i18n'
import { getOrCreateRendererRoot } from './lib/react-renderer-root'
import { SkillWarningPreviewLauncher } from './components/skills/SkillWarningPreviewLauncher'
import { installRendererBootstrapHealthReporter } from './lib/renderer-bootstrap-health-reporter'
import { subscribeToDesktopWindowCloseRequests } from './desktop/desktop-window-close-subscription'
import { desktopWindowClient } from './desktop/runtime-desktop-window-client'
import { dispatchWindowCloseRequest } from './components/window-close-request-coordinator'

recordRendererCrashBreadcrumb('renderer_bootstrap_started', { dev: import.meta.env.DEV })
installRendererCrashDiagnostics()
installTypingLatencyDiagnostic()

if (
  import.meta.env.DEV &&
  shouldEnableReactGrab({
    dev: import.meta.env.DEV,
    enableFlag: import.meta.env.VITE_ENABLE_REACT_GRAB
  })
) {
  void import('react-grab').then(({ init }) => init())
  void import('react-grab/styles.css')
}

applyDocumentTheme('system', { disableTransitions: false })

const rootElement = document.getElementById('root')
if (!rootElement) {
  recordRendererCrashBreadcrumb('renderer_root_missing')
  throw new Error('Renderer root element not found.')
}
const rendererRootElement = rootElement

type RendererHotData = { rendererHealthReporterInstalled?: boolean }
const rendererHotData = import.meta.hot?.data as RendererHotData | undefined
// Why: native quit must remain actionable even if a later Electron-only bootstrap
// service fails while the Tauri migration is incomplete. The coordinator gains
// its rich guards/finalizers as App mounts, but this listener exists first.
const stopDesktopWindowCloseSubscription = subscribeToDesktopWindowCloseRequests(
  desktopWindowClient,
  dispatchWindowCloseRequest
)
import.meta.hot?.dispose(stopDesktopWindowCloseSubscription)
// Why: document ownership lets the watchdog survive the Fast Refresh unmount that can empty #root.
if (import.meta.env.DEV && !rendererHotData?.rendererHealthReporterInstalled) {
  installRendererBootstrapHealthReporter({
    root: rendererRootElement,
    reportHealth: (report) => window.api?.app.reportRendererHealth?.(report),
    onHealthProbe: (listener) =>
      window.api?.app.onRendererHealthProbe?.(listener) ?? (() => undefined)
  })
  if (rendererHotData) {
    rendererHotData.rendererHealthReporterInstalled = true
  }
}

function RendererRoot(): React.JSX.Element {
  useTranslation()
  return (
    <RecoverableRenderErrorBoundary
      boundaryId="app.root"
      surface="app-root"
      title={translate('app.recoverableError.rootTitle', 'MCode hit a renderer error.')}
      description={translate(
        'app.recoverableError.rootDescription',
        'The app shell could not finish rendering. Retry to remount it, or relaunch MCode if the error persists.'
      )}
    >
      <App />
      <SkillWarningPreviewLauncher />
    </RecoverableRenderErrorBoundary>
  )
}

getOrCreateRendererRoot(rendererRootElement, import.meta.hot?.data).render(
  <StrictMode>
    <I18nProvider>
      <RendererRoot />
    </I18nProvider>
  </StrictMode>
)
recordRendererCrashBreadcrumb('renderer_bootstrap_rendered')
