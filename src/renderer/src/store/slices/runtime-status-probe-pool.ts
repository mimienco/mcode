// Why: PERF-09 — status probes for many servers must never run as an
// unbounded wave; both the Servers pane and boot hydration share this pool.

export const RUNTIME_STATUS_PROBE_PARALLELISM = 4

/** Runs workers over items with at most `limit` concurrently active promises. */
export async function runWithPool<T>(
  items: readonly T[],
  limit: number,
  worker: (item: T) => Promise<void>
): Promise<void> {
  let nextIndex = 0
  const runners = Array.from({ length: Math.min(limit, items.length) }, async () => {
    while (nextIndex < items.length) {
      const item = items[nextIndex]
      nextIndex += 1
      await worker(item)
    }
  })
  await Promise.all(runners)
}
