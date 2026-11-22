import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ManagedPaneInternal } from './pane-manager-types'
import { resumePaneRendering, suspendPaneRendering } from './pane-rendering-control'
import {
  releaseHiddenWebglRetention,
  resetHiddenWebglRetentionForTest,
  retainedHiddenWebglOwnerCountForTest,
  tryRetainHiddenPanesWebgl
} from './terminal-webgl-hidden-retention'

function createPane(withAddon = true): ManagedPaneInternal {
  return {
    terminal: { blur: vi.fn() },
    webglAddon: withAddon
      ? ({ dispose: vi.fn() } as unknown as ManagedPaneInternal['webglAddon'])
      : null,
    webglAttachmentDeferred: false,
    webglDisabledAfterContextLoss: false,
    webglAttachFailedSinceRecovery: false,
    gpuRenderingEnabled: true,
    pendingWebglRefreshRafId: null
  } as unknown as ManagedPaneInternal
}

function retentionFor(owner: object, panes: ManagedPaneInternal[]) {
  return { owner, livePanes: () => panes }
}

describe('terminal-webgl-hidden-retention', () => {
  beforeEach(() => {
    resetHiddenWebglRetentionForTest()
  })

  it('suspend retains live WebGL addons and still defers attachment', () => {
    const owner = {}
    const panes = [createPane(), createPane()]
    suspendPaneRendering(panes, retentionFor(owner, panes))
    expect(panes.every((pane) => pane.webglAttachmentDeferred)).toBe(true)
    expect(panes.every((pane) => pane.webglAddon != null)).toBe(true)
    expect(panes.every((pane) => vi.mocked(pane.terminal.blur).mock.calls.length === 1)).toBe(true)
    expect(retainedHiddenWebglOwnerCountForTest()).toBe(1)
  })

  it('suspend without retention context disposes addons (window-level callers unchanged)', () => {
    const panes = [createPane()]
    const addon = panes[0].webglAddon
    suspendPaneRendering(panes)
    expect(addon?.dispose).toHaveBeenCalled()
    expect(panes[0].webglAddon).toBeNull()
    expect(panes[0].terminal.blur).not.toHaveBeenCalled()
  })

  it('evicts the least-recently-hidden owner over the context cap', () => {
    const ownerA = {}
    const panesA = Array.from({ length: 10 }, () => createPane())
    const addonsA = panesA.map((pane) => pane.webglAddon)
    suspendPaneRendering(panesA, retentionFor(ownerA, panesA))

    const ownerB = {}
    const panesB = Array.from({ length: 10 }, () => createPane())
    suspendPaneRendering(panesB, retentionFor(ownerB, panesB))

    // A(10) + B(10) fits the cap of 24; C(5) forces A out.
    const ownerC = {}
    const panesC = Array.from({ length: 5 }, () => createPane())
    suspendPaneRendering(panesC, retentionFor(ownerC, panesC))

    expect(panesA.every((pane) => pane.webglAddon === null)).toBe(true)
    for (const addon of addonsA) {
      expect(addon?.dispose).toHaveBeenCalled()
    }
    expect(panesB.every((pane) => pane.webglAddon != null)).toBe(true)
    expect(panesC.every((pane) => pane.webglAddon != null)).toBe(true)
    expect(retainedHiddenWebglOwnerCountForTest()).toBe(2)
  })

  it('re-suspending an owner refreshes its LRU position', () => {
    const ownerA = {}
    const panesA = Array.from({ length: 10 }, () => createPane())
    suspendPaneRendering(panesA, retentionFor(ownerA, panesA))
    const ownerB = {}
    const panesB = Array.from({ length: 10 }, () => createPane())
    suspendPaneRendering(panesB, retentionFor(ownerB, panesB))

    // Touch A again: B becomes the eviction candidate.
    suspendPaneRendering(panesA, retentionFor(ownerA, panesA))
    const ownerC = {}
    const panesC = Array.from({ length: 5 }, () => createPane())
    suspendPaneRendering(panesC, retentionFor(ownerC, panesC))

    expect(panesB.every((pane) => pane.webglAddon === null)).toBe(true)
    expect(panesA.every((pane) => pane.webglAddon != null)).toBe(true)
  })

  it('does not retain when the owner has no live addons', () => {
    const owner = {}
    const panes = [createPane(false)]
    suspendPaneRendering(panes, retentionFor(owner, panes))
    expect(retainedHiddenWebglOwnerCountForTest()).toBe(0)
  })

  it('retains a dense hidden workspace for an immediate switch-back', () => {
    const owner = {}
    const panes = Array.from({ length: 15 }, () => createPane())
    suspendPaneRendering(panes, retentionFor(owner, panes))
    expect(retainedHiddenWebglOwnerCountForTest()).toBe(1)
    expect(panes.every((pane) => pane.webglAddon != null)).toBe(true)
  })

  it('does not retain a single owner wider than the whole cap', () => {
    const owner = {}
    const panes = Array.from({ length: 25 }, () => createPane())
    const addons = panes.map((pane) => pane.webglAddon)
    suspendPaneRendering(panes, retentionFor(owner, panes))
    expect(retainedHiddenWebglOwnerCountForTest()).toBe(0)
    for (const addon of addons) {
      expect(addon?.dispose).toHaveBeenCalled()
    }
  })

  it('resume releases retention bookkeeping and keeps the live addon', () => {
    const owner = {}
    const panes = [createPane()]
    suspendPaneRendering(panes, retentionFor(owner, panes))
    expect(resumePaneRendering(panes, owner)).toBe(false)
    expect(retainedHiddenWebglOwnerCountForTest()).toBe(0)
    expect(panes[0].webglAddon).not.toBeNull()
    expect(panes[0].webglAttachmentDeferred).toBe(false)
    // Released owner no longer counts toward the cap: two more owners fit.
    const others = [{}, {}]
    for (const other of others) {
      const otherPanes = [createPane(), createPane(), createPane()]
      suspendPaneRendering(otherPanes, retentionFor(other, otherPanes))
      expect(otherPanes.every((pane) => pane.webglAddon != null)).toBe(true)
    }
  })

  it('releaseHiddenWebglRetention never disposes addons (destroy path owns that)', () => {
    const owner = {}
    const panes = [createPane()]
    expect(tryRetainHiddenPanesWebgl(owner, () => panes)).toBe(true)
    releaseHiddenWebglRetention(owner)
    expect(panes[0].webglAddon).not.toBeNull()
    expect(retainedHiddenWebglOwnerCountForTest()).toBe(0)
  })

  it('eviction accounting survives addons disposed after retention (GPU toggle off)', () => {
    const ownerA = {}
    const panesA = [createPane(), createPane(), createPane(), createPane()]
    suspendPaneRendering(panesA, retentionFor(ownerA, panesA))
    // GPU-off path disposes retained addons directly.
    for (const pane of panesA) {
      pane.webglAddon = null
    }
    const ownerB = {}
    const panesB = Array.from({ length: 6 }, () => createPane())
    suspendPaneRendering(panesB, retentionFor(ownerB, panesB))
    expect(panesB.every((pane) => pane.webglAddon != null)).toBe(true)
  })
})
