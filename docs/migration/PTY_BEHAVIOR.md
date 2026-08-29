# PTY and Process Behavior

## Current contract to preserve

The Electron implementation owns PTYs outside the renderer, forwards output through preload, tracks acknowledgements, and can resynchronize/reconstruct renderer state after reload. Existing tests cover raw terminal rendering, shell/agent exits, high output, hidden panes, parking, foreground state, resize, and recovery. Patched `node-pty` behavior is a requirement source, not disposable implementation detail.

## Target contract

- ConPTY on Windows and Unix PTYs on macOS/Linux.
- Opaque process and PTY handles; no PID authority in the renderer.
- Argument vectors and explicit environment policy; no shell-string concatenation.
- Correct rows/columns, resize coalescing, UTF-8 and arbitrary byte chunks.
- Process groups/job objects and deterministic descendant cleanup.
- Graceful stop with a bounded forced-kill fallback.
- Structured exit code/signal/error and `live`/`unverifiable`/`exited` status boundaries.
- Multiple concurrent sessions, bounded memory, and no output loss under sustained load.
- Renderer reload detach/reattach with sequence numbers, ACKs, bounded replay, gap detection, and explicit resync.
- Crash/orphan reconciliation without silently adopting unrelated processes.

## Required evidence

Golden byte streams, shell profile tests, resize storms, 10+ parallel PTYs, large output, Unicode, IME, child/grandchild teardown, renderer reload, application shutdown under load, WSL, and same-machine throughput/latency/RAM comparisons. Crate selection is pending a measured spike.
