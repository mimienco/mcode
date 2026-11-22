const DEFAULT_COLS = 80
const DEFAULT_ROWS = 24
// Why: RPC payloads are untrusted JSON; huge-but-finite dimensions reach
// @xterm/headless unclamped and can throw RangeError after the PTY subprocess
// already spawned, orphaning the live shell. Cap like the relay's resize clamp.
const MAX_COLS = 500
const MAX_ROWS = 500

export function isValidPtySize(cols: number, rows: number): boolean {
  return Number.isFinite(cols) && Number.isFinite(rows) && cols >= 1 && rows >= 1
}

export function normalizePtySize(cols: number, rows: number): { cols: number; rows: number } {
  const size = isValidPtySize(cols, rows)
    ? { cols: Math.floor(cols), rows: Math.floor(rows) }
    : { cols: DEFAULT_COLS, rows: DEFAULT_ROWS }
  return {
    cols: Math.min(MAX_COLS, Math.max(1, size.cols)),
    rows: Math.min(MAX_ROWS, Math.max(1, size.rows))
  }
}
