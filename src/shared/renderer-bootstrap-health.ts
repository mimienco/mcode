export const RENDERER_HEALTH_CHANNEL = 'app:renderer-health'
export const RENDERER_HEALTH_PROBE_CHANNEL = 'app:renderer-health-probe'

export type RendererHealthReport = Readonly<{
  healthy: boolean
  probeId: number
}>
