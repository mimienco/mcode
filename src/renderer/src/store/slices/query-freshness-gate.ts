// Why: PERF-17 — shared coalescing + freshness gate so startup and Settings
// share one read path instead of stacking identical IPC round-trips.
export type QueryFreshnessGate = {
  /** Runs fetch unless an identical read is in flight or the last one is still fresh. */
  run: (fetch: () => Promise<void>) => Promise<void>
}

export function createQueryFreshnessGate(freshMs: number): QueryFreshnessGate {
  const query: { fetchedAt: number; inFlight: Promise<void> | null } = {
    fetchedAt: 0,
    inFlight: null
  }
  return {
    run: (fetch) => {
      if (query.inFlight) {
        return query.inFlight
      }
      if (Date.now() - query.fetchedAt < freshMs) {
        return Promise.resolve()
      }
      const inFlight = fetch()
        .then(() => {
          query.fetchedAt = Date.now()
        })
        .finally(() => {
          query.inFlight = null
        })
      query.inFlight = inFlight
      return inFlight
    }
  }
}
