import { useSyncExternalStore } from 'react'
import { ShieldQuestion } from 'lucide-react'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from '@/components/ui/dialog'

// ─── Module-level pending host-key decision ─────────────────────────────────

export type SshPresentedHostKey = {
  connectAttemptId: number
  fingerprint: string
  algorithm: string
  keyBase64: string
}

type PendingDecision = {
  presented: SshPresentedHostKey
  resolve: (accept: boolean) => void
}

let pendingDecision: PendingDecision | null = null
const listeners: (() => void)[] = []

function subscribe(listener: () => void): () => void {
  listeners.push(listener)
  return () => {
    const index = listeners.indexOf(listener)
    if (index !== -1) {
      listeners.splice(index, 1)
    }
  }
}

function getPendingDecision(): PendingDecision | null {
  return pendingDecision
}

function notifyListeners(): void {
  for (const listener of listeners) {
    listener()
  }
}

function setPendingDecision(next: PendingDecision | null): void {
  pendingDecision = next
  notifyListeners()
}

/**
 * TOFU surface used by ssh connect callers: presents the dialog for the
 * given host key and resolves to the user's decision. Only one decision can
 * be pending at a time — a second concurrent call supersedes the first by
 * rejecting it.
 */
export function confirmSshHostKeyDecision(presented: SshPresentedHostKey): Promise<boolean> {
  const previous = pendingDecision
  if (previous) {
    previous.resolve(false)
  }
  return new Promise<boolean>((resolve) => {
    setPendingDecision({ presented, resolve })
  })
}

/**
 * Current pending host key, or null when the dialog is idle.
 */
export function useSshHostKeyPrompt(): SshPresentedHostKey | null {
  const pending = useSyncExternalStore(subscribe, getPendingDecision, getPendingDecision)
  return pending?.presented ?? null
}

/**
 * Renders the host-key trust dialog when a decision is pending, null
 * otherwise. Mounted once beside the other root surfaces.
 */
export function SshHostKeyPrompt(): React.JSX.Element | null {
  const pending = useSyncExternalStore(subscribe, getPendingDecision, getPendingDecision)

  const decide = (accept: boolean): void => {
    if (!pending) {
      return
    }
    pending.resolve(accept)
    setPendingDecision(null)
  }

  if (pending === null) {
    return null
  }

  return (
    <Dialog
      open
      onOpenChange={(isOpen) => {
        if (!isOpen) {
          decide(false)
        }
      }}
    >
      <DialogContent className="sm:max-w-md" showCloseButton={false}>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-sm">
            <ShieldQuestion className="size-4 text-muted-foreground" aria-hidden="true" />
            Unknown host key
          </DialogTitle>
          <DialogDescription className="text-xs leading-relaxed">
            This server presented a host key that is not in your known hosts. Trust-on-first-use:
            accept only if you recognize this fingerprint.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-1 text-xs">
          <div>
            <span className="text-muted-foreground">Algorithm: </span>
            <span className="font-mono">{pending.presented.algorithm}</span>
          </div>
          <div>
            <span className="text-muted-foreground">Fingerprint: </span>
            <span className="font-mono break-all">{pending.presented.fingerprint}</span>
          </div>
        </div>
        <DialogFooter>
          <Button type="button" size="sm" variant="outline" onClick={() => decide(false)}>
            Reject
          </Button>
          <Button type="button" size="sm" autoFocus onClick={() => decide(true)}>
            Accept once
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
