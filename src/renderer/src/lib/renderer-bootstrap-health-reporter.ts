import type { RendererHealthReport } from '../../../shared/renderer-bootstrap-health'

type RendererBootstrapHealthReporterOptions = {
  root: HTMLElement
  reportHealth: (report: RendererHealthReport) => void
  onHealthProbe: (listener: (probeId: number) => void) => () => void
}

export function installRendererBootstrapHealthReporter({
  root,
  reportHealth,
  onHealthProbe
}: RendererBootstrapHealthReporterOptions): () => void {
  let activeProbeId: number | null = null
  const reportCurrentHealth = (): void => {
    if (activeProbeId !== null) {
      reportHealth({ healthy: root.childElementCount > 0, probeId: activeProbeId })
    }
  }
  const observer = new MutationObserver(reportCurrentHealth)
  observer.observe(root, { childList: true })

  const removeHealthProbeListener = onHealthProbe((probeId) => {
    activeProbeId = probeId
    reportCurrentHealth()
  })
  return () => {
    observer.disconnect()
    removeHealthProbeListener()
  }
}
