// Why: Claude's OAuth refresh token is single-use — two concurrent rotations
// for the same account invalidate every copy but one and can strand a stored
// token as invalid_grant. Keyed promise chains serialize rotation attempts per
// account no matter which entry path (runtime auth, usage preview) triggers them.
const lockTails = new Map<string, Promise<unknown>>()

export function withOauthRefreshLock<T>(key: string, fn: () => Promise<T>): Promise<T> {
  const tail = lockTails.get(key) ?? Promise.resolve()
  const run = tail.then(fn, fn)
  const settled = run.catch(() => {})
  lockTails.set(key, settled)
  void settled.then(() => {
    if (lockTails.get(key) === settled) {
      lockTails.delete(key)
    }
  })
  return run
}
