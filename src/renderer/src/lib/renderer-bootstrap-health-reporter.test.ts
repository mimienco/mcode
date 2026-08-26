// @vitest-environment happy-dom

import { expect, it, vi } from 'vitest'
import { installRendererBootstrapHealthReporter } from './renderer-bootstrap-health-reporter'

it('reports root health for the latest main-process probe across root mutations', async () => {
  const root = document.createElement('div')
  root.append(document.createElement('main'))
  const reportHealth = vi.fn()
  let onProbe = (_probeId: number): void => {
    throw new Error('health probe listener was not installed')
  }

  installRendererBootstrapHealthReporter({
    root,
    reportHealth,
    onHealthProbe: (listener) => {
      onProbe = listener
      return () => undefined
    }
  })

  expect(reportHealth).not.toHaveBeenCalled()
  onProbe(17)
  expect(reportHealth).toHaveBeenLastCalledWith({ healthy: true, probeId: 17 })
  root.replaceChildren()
  await Promise.resolve()
  expect(reportHealth).toHaveBeenLastCalledWith({ healthy: false, probeId: 17 })
  root.append(document.createElement('main'))
  await Promise.resolve()
  expect(reportHealth).toHaveBeenLastCalledWith({ healthy: true, probeId: 17 })
  onProbe(18)
  expect(reportHealth).toHaveBeenLastCalledWith({ healthy: true, probeId: 18 })
})
