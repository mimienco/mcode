export type TerminalOutputObservation = {
  ptyId: string
  data: string
}

type TerminalOutputObserver = (observation: TerminalOutputObservation) => void

const observers = new Set<TerminalOutputObserver>()

export function subscribeTerminalOutputObserver(observer: TerminalOutputObserver): () => void {
  observers.add(observer)
  return () => observers.delete(observer)
}

export function publishTerminalOutput(observation: TerminalOutputObservation): void {
  for (const observer of Array.from(observers)) {
    try {
      observer(observation)
    } catch (error) {
      console.warn('[workspace-diagnostics] output observer failed', error)
    }
  }
}
