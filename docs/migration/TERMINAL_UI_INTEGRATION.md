# Terminal UI Integration — Renderer Terminal Stack ↔ Rust PTY Layer

Design doc for connecting the existing renderer terminal stack
(`pty-transport.ts` 1,162 ln, `pty-connection.ts` 9,700 ln, `pty-dispatcher.ts`
340 ln) to the new Rust PTY layer (`src-tauri/src/pty_commands.rs` over
`crates/mcode-pty`). Grounded 2026-08-31; every count below was grepped from
the actual files, not estimated. Contract background: `PTY_CONTRACT_SPEC.md`
§4 (channel index) and §5 (binary streaming design).

## 0. Scope and the deliberate deferral

`pty-connection.ts` is 9,700 lines with 38 `window.api.pty.*` calls spanning
pane serializers, delivery healing, foreground-process authority, and
cold-restore handshakes. This wave does **not** rewrite that file. It defines
the **transport seam** — the single factory where Electron IPC and the Tauri
PTY client diverge — so the byte lane (spawn → data → write → resize → kill →
exit) runs on Rust immediately, and every deferred surface keeps its Electron
wiring until its Rust slice lands. A dedicated pass owns the pane-serializer
slice afterwards.

---

## 1. Inventory: every distinct `window.api.pty.*` method in the terminal stack

Counts are call sites, grepped literally as `window.api.pty.<method>`:

- `pty-transport.ts`: 10 call sites (write 1, writeAccepted 1, spawn 1,
  kill 2, resize 3, claimViewport 2).
- `pty-connection.ts`: 38 call sites across 11 distinct methods (below).
- `pty-dispatcher.ts`: 7 call sites (onData, onExit, onReplay,
  onWriteUnavailable, onDeliveryResyncRequest, respondDeliveryResync,
  rendererDispatcherReady).
- Adjacent (same stack, outside those three files): `terminal-pty-ack-gate.ts:36`
  calls `window.api.pty.ackData?.(...)` once; `terminal-pane-recovery.ts:220`
  and `use-terminal-pane-lifecycle.ts:2041-2045` call `hasPty`.

Dispositions:

- **(a)** — covered by `src/renderer/src/desktop/pty-client.ts` +
  `src-tauri/src/pty_commands.rs` today (`pty_spawn`, `pty_write`,
  `pty_resize`, `pty_kill`, `pty_has_pty`; events `pty://data`, `pty://exit`).
- **(b)** — maps to an existing `mcode-pty`/`mcode-process` capability that is
  not yet exposed as a Tauri command (thin command-layer work, no crate work).
- **(c)** — session/delivery/serializer machinery deferred with the
  pane-serializer slice (the deferred pass, §3).

| Method | File (call sites) | Count | Disposition |
| --- | --- | --- | --- |
| `spawn` | pty-transport.ts:813 | 1 | **(a)** `pty_spawn` — client `spawn(spec)` |
| `write` | pty-transport.ts:594 | 1 | **(a)** `pty_write` — client `write(id, data)` |
| `writeAccepted` | pty-transport.ts:730 | 1 | **(a\*)** fold into `pty_write` returning `bool`; the 16 MiB / 16 KiB input bounds (spec §5.2) move into `pty_write` validation |
| `resize` | pty-transport.ts:1032,1115,1118 | 3 | **(a)** `pty_resize` — client `resize(id, cols, rows)` |
| `kill` | pty-transport.ts:861,1044 | 2 | **(a)** `pty_kill` — client `kill(id)` (bounded 5 s flush ladder already in `pty_commands.rs`) |
| `onData` | pty-dispatcher.ts:103 | 1 | **(a)** `pty://data` — client `onData` (`PtyDataPayload`) |
| `onExit` | pty-dispatcher.ts:189 | 1 | **(a)** `pty://exit` — client `onExit` (`{id, code}`); no `preserveRendererBinding` / `incarnationId` under Tauri (remote-sleep machinery is Electron) |
| `ackData` | terminal-pty-ack-gate.ts:36 (adjacent) | 1 | **(b)** `PtySession::ack(processed_chars)` (`session.rs:176`) already implements the cumulative max-merge — expose `pty_ack_data` command; renderer gate keeps its local total as the dedupe baseline |
| `hasPty` | terminal-pane-recovery.ts:220, use-terminal-pane-lifecycle.ts:2041 (adjacent) | 2 | **(a)** `pty_has_pty` — client `hasPty(id)` (already used by recovery, not by the transport trio) |
| `getSize` | pty-connection.ts:4448,4724 | 2 | **(b)** `PtySession::applied_size()` (`session.rs:165`) exists — expose `pty_get_size` returning `{cols,rows}\|null` |
| `signal` | pty-connection.ts:4483,7534,7557,8728 | 4 | **(b)** SIGWINCH-only today; under Tauri `pty_resize` subsumes the WINCH side effect; a real `pty_signal` needs a `PtySession::signal` passthrough over `portable_pty`'s child (kill ladder exists, signal does not) — expose only if a non-WINCH signal consumer appears |
| `getForegroundProcess` | pty-connection.ts:2181 | 1 | **(b)** process-table read belongs to `mcode-process` (descendant scan exists, 25 tests) — expose `pty_get_foreground_process` reading through the supervisor |
| `confirmForegroundProcess` | pty-connection.ts:2182 | 1 | **(b)** same as above with forced-fresh semantics (`pty_confirm_foreground_process`) |
| `claimViewport` | pty-transport.ts:1106,1116 | 2 | **(c)** viewport-authority chaining is main-side provider bookkeeping (`pty.ts:7538-7565`); no byte-owner mirror — deferred with delivery-authority slice |
| `reportGeometry` | pty-connection.ts:4645 | 1 | **(c)** measurement-only sibling of resize for mobile-fit restore targets — no Tauri consumer until mobile pairing lands |
| `getMainBufferSnapshot` | pty-connection.ts:6121,8154 | 2 | **(c)** authoritative ANSI snapshot + `pendingDeliveryStartSeq` needs the headless emulator/model snapshot store — pane-serializer slice |
| `ackColdRestore` | pty-connection.ts:8493,8595,8621,8684 | 4 | **(c)** clears the daemon adapter's sticky cold-restore cache (`pty.ts:7617-7626`) — cold restore is Electron daemon machinery |
| `declarePendingPaneSerializer` | pty-connection.ts:5407,8932,9180 | 3 | **(c)** pane-serializer handshake (`pty.ts:8082-8130`) — pane-serializer slice |
| `settlePaneSerializer` | pty-connection.ts:4948,5475,5542,9037,9274 | 5 | **(c)** pane-serializer slice |
| `clearPendingPaneSerializer` | pty-connection.ts:4943,4954,5446,5477,5545,5566,8983,9000,9025,9045,9229,9237,9262,9282 | 14 | **(c)** pane-serializer slice |
| `reportRendererSerializerReady` | pty-connection.ts:4970 | 1 | **(c)** pane-serializer slice |
| `onReplay` | pty-dispatcher.ts:181 | 1 | **(c→b)** incremental SSH reattach replay (`pty://replay`); mcode-ssh transport exists (29 tests) but the pty-replay channel is not wired — lands with SSH pty ownership (spec §9.3) |
| `onWriteUnavailable` | pty-dispatcher.ts:174 | 1 | **(b)** Rust flow control is credit-based (`GlobalCredit`, `ByteOwner::can_send`); a provider-side hard failure maps to a `pty://write-unavailable` emit from `pty_write`'s error path — small command-layer slice, kept with delivery work |
| `onDeliveryResyncRequest` | pty-dispatcher.ts:215 | 1 | **(c)** delivery resync probe (`pty.ts:3121`) — delivery-healing slice |
| `respondDeliveryResync` | pty-dispatcher.ts:216 | 1 | **(c)** delivery-healing slice |
| `rendererDispatcherReady` | pty-dispatcher.ts:225 | 1 | **(c)** boot-window send-holding handshake; Tauri emit has no boot drop window today — becomes a no-op under the Tauri transport, real slice only if a held-send window is introduced |

Totals: 24 distinct methods + `ackData`/`hasPty` adjacent; **55 call sites**
in the transport trio (10 + 38 + 7), 58 with the adjacent two. Disposition
spread: (a) 8 methods / 12 call sites ride the seam now; (b) 5 methods / 8
call sites are thin command exposures; (c) 11 methods / 35 call sites stay on
Electron wiring until their slices land.

---

## 2. Transport seam design

### 2.1 The single edit point

`pty-connection.ts:4026` is the only construction site for local transports:

```ts
: createIpcPtyTransport(transportOptions)
```

The design keeps `PtyTransport` (`pty-transport-types.ts:135-220`) untouched
and introduces one runtime-selected factory in a new small module, e.g.
`terminal-pane/pty-transport-factory.ts`:

```ts
import { isTauri } from '@tauri-apps/api/core'
import { createIpcPtyTransport } from './pty-transport'
import { createTauriPtyTransport } from './pty-transport-tauri'

export function createLocalPtyTransport(opts: IpcPtyTransportOptions): PtyTransport {
  return isTauri() ? createTauriPtyTransport(opts) : createIpcPtyTransport(opts)
}
```

This mirrors the established runtime-selection pattern already in the tree
(`fs-client.ts:196-208 createRuntimeFsClient`,
`runtime-desktop-app-lifecycle-client.ts:47-48`). `pty-connection.ts:4026`
switches from `createIpcPtyTransport(...)` to `createLocalPtyTransport(...)`;
`createIpcPtyTransport` itself is **not** modified — it remains the Electron
transport verbatim.

`createTauriPtyTransport(opts)` wraps `createTauriPtyClient(invoke, listen)`
(`pty-client.ts:95-145`, already DTO-tested in `pty-client.test.ts`) and
implements only the `PtyTransport` members that the (a)-rows support:

| `PtyTransport` member | Tauri backing |
| --- | --- |
| `connect` | `client.spawn({program?, argVec?, cwd?, shellOverride?, cols, rows, env?})` → `PtySpawnResponse{id, shell}`. Electron-only spawn fields (`commandDelivery`, `launchConfig`, `resumeProviderSession`, `launchToken`, `launchAgent`, `startupCommandDelivery`, reattach/snapshot unions) are absent by design — those flows are (b)/(c) surface, and `connect` returns the plain id until their slices land |
| `attach` | re-register handlers on the dispatcher port + `resize` when `cols/rows` given; no eager-buffer replay source exists under Tauri until snapshots land |
| `disconnect` / `destroy` | `client.kill(id)` (bounded flush ladder in `pty_commands.rs:345-370`) |
| `sendInput` / `sendInputImmediate` | `client.write(id, data)` through the existing `createPtyInputWriteQueue` (coalescing is renderer-owned, transport-agnostic) |
| `resize` | `client.resize(id, cols, rows)` |
| `isConnected` / `getPtyId` / `getConnectionId` / `getLocalSessionMetadata` | local state, identical semantics |
| `detach` | unregister handlers only — no kill (same contract as `pty-transport.ts:1052-1069`) |

Deliberately **absent** (capability-negotiated optionals — absence is the
negotiation): `sendInputAccepted`, `claimViewport`, `setOutputPaused`,
`getRecoveryState`, `retryRecovery`, `notifyErrorSurfaceDismissed`,
`getRuntimeEnvironmentId`, `getExecutionHostId`, `getRemotePlatform`,
`serializeBuffer`, `serializeBufferOutcome`, `resetCrossChunkParserState`.
Consumers already branch on these being optional
(`pty-transport-types.ts:175-219`).

### 2.2 Event plumbing

`pty-dispatcher.ts` is a singleton router keyed by PTY id with one global
listener per channel (`pty-dispatcher.ts:50-52`). The Tauri transport plugs
into the **same** router: `client.onData(listener)` / `client.onExit(listener)`
replace the `window.api.pty.onData/onExit` calls inside
`attachPtyPushListeners`/`attachPtySecondaryPushListeners`. Concretely, the
dispatcher's subscription set is parameterized by a `PtyEventPort`:

```ts
type PtyEventPort = {
  onData: (l: (p: { id: string; data: string; seq?: number; rawLength?: number;
    transformed?: boolean; dropped?: boolean; restoreMarker?: PtyRestoreMarker }) => void) => Promise<() => void>
  onExit: (l: (p: { id: string; code: number }) => void) => Promise<() => void>
}
```

- Electron port: the existing `window.api.pty.onData/onExit`.
- Tauri port: `client.onData` / `client.onExit`, mapping
  `PtyDataPayload{id: number, chunks: PtyDataChunk[]}` → per-chunk fan-out so
  the downstream router (`handleDispatchedPtyData`,
  `pty-dispatcher.ts:114-171`) keeps its exact shape. Chunk mapping: `data`,
  `seq`, `rawLength`, `transformed` pass through; `dropped`/`restoreMarker`
  are handled before fan-out (§2.3).

Note the id-type seam: Electron PTY ids are strings (`remote:`-prefixed ids
included); `pty_spawn` returns a numeric handle. The Tauri event port renders
the handle as its decimal string (`String(id)`) at the port boundary only, so
every downstream map key stays a string and no consumer changes. `pty_kill` /
`pty_write` / `pty_resize` parse it back with `Number(id)` inside the Tauri
transport. Remote/SSH ids never reach the Tauri transport (they are guarded by
`runtimeEnvironmentId` at `pty-connection.ts:4024`).

### 2.3 Sequence numbers and dedupe → cumulative ACK

Rust owns the sequence (`PTY_CONTRACT_SPEC.md` §5.1): each `PtyDataChunk.seq`
is the cumulative output sequence **after** the chunk (char count,
`byte_owner.rs:57-68`), with `rawLength` present when `transformed` broke the
1:1 length relation. The chunk's covered span is therefore
`startSeq = seq - (rawLength ?? data.length)`.

The renderer-side dedupe rule maps onto the existing cumulative-ACK machinery
in `terminal-pty-ack-gate.ts`:

- `processedPtyCharTotals` (`terminal-pty-ack-gate.ts:29`) already holds the
  monotonic per-PTY total of processed chars — the exact baseline the spec's
  dedupe rule needs: `if (startSeq < processedChars) { skip overlap }`
  (§5.1 "**Target**" bullet).
- The gate's deferred crediting (`deliverPtyDataWithDeferredAck`,
  `terminal-pty-ack-gate.ts:101-107`) stays byte-for-byte identical: credit
  fires when the output scheduler consumes, not at dispatch.
- The **only** delta under Tauri: `window.api.pty.ackData?.(ptyId, chars,
  processedChars)` (`terminal-pty-ack-gate.ts:36`) has no Rust receiver yet —
  the optional call silently no-ops. The local total keeps advancing (it is
  the dedupe baseline), and the (b)-row `pty_ack_data` command later re-arms
  Rust-side credit reclamation (`PtySession::ack` already implements the
  cumulative max-merge, `session.rs:174-177`; `ByteOwner::ack`,
  `byte_owner.rs:415`).
- `restoreMarker` (in-band on `PtyDataPayload` under Tauri, vs the
  out-of-band `pty://model-restore-needed` channel in Electron) flushes the
  baseline: everything `≤ markerSeq` is only recoverable from the model
  snapshot, so the dedupe window resets to `markerSeq` and the (c)-surface
  snapshot restore takes over when it lands.
- `dropped` chunks carry salvaged query bytes (`DroppedOutput`,
  `byte_owner.rs:82-87`); they are written to the terminal but must **not**
  advance the ACK credit beyond their reported `seq` (the covered bytes are
  gone, not re-deliverable); only the OSC-9999 parser-carry reset
  (`resetCrossChunkParserState` semantics) applies across the gap.

On exit, the dispatcher already clears per-id totals (`pty-dispatcher.ts:195-196`);
this stays, aligning renderer accounting with Rust's fresh-per-session owner.

### 2.4 What does NOT change

- `createPtyOutputProcessor` (title/bell/agent-status pipeline,
  `pty-transport.ts:145-554`) — transport-agnostic, reused as-is.
- The input write queue, chunking (`iterateTerminalInputChunks`), and the
  16 MiB input bound — renderer-owned.
- The eager-buffer/pre-handler buffering layer (`pty-shutdown-data-suspension`,
  `registerEagerPtyBuffer`) — operates on router events, not on `window.api`.
- `pty-connection.ts` beyond the one construction site.

---

## 3. Deferred surfaces (not in this seam)

These channels keep their Electron wiring until their Rust slice lands. The
Tauri transport simply never touches them.

### 3.1 Pane serializers (the dedicated follow-up pass)

`declarePendingPaneSerializer` (3), `settlePaneSerializer` (5),
`clearPendingPaneSerializer` (14), `reportRendererSerializerReady` (1) — 23
call sites — plus their consumer `getMainBufferSnapshot` (2). The
main↔renderer serialization round-trip (`pty:serializeBuffer:request/response`,
spec §4 row 85) needs a reverse-direction request lane whose Tauri shape
(main→webview invoke) is an open integration point (spec §9.4). Slice:
`pty_declare_pending_serializer` + `pty_settle_serializer` +
`pty_clear_serializer` + `pty_report_serializer_ready` commands and the
snapshot store; `ackColdRestore` (4 call sites) rides along since cold-restore
consumption is part of the same daemon-adapter surface.

### 3.2 Delivery resync / healing

`onDeliveryResyncRequest` + `respondDeliveryResync` (2 call sites) and the
unprompted `pty_report_renderer_delivery_state` lane (spec §5.6). Under Tauri
the event channel is same-process webview emit — the lost-ACK failure mode
these heal is structurally narrower — so this slice is intentionally last:
first re-arm `pty_ack_data` (b-row), then measure whether resync is still
reachable, and only then port the probe/response pair.

### 3.3 Hidden-delivery gate and delivery authority

`claimViewport` (2), `reportGeometry` (1), `setOutputPaused`,
`initiallyHidden` spawn marking — the visibility registry / viewport
authority / background-thinning plane (`pty.ts:7740-7840`) has no byte-owner
mirror in Rust yet. The Rust side has the primitives (`pause_producer` /
`resume_producer`, `session.rs:153-160`; interactive carve-out in
`can_send`), so the slice maps gate semantics onto producer pauses + credit
lanes, but it is out of this wave's scope.

### 3.4 `pty://replay` (SSH reattach)

`onReplay` (1 call site) serves incremental SSH scrollback replay
(spec §4 row 64). mcode-ssh (29 tests) and mcode-wire (21 tests) are green,
but the pty-replay channel and SSH pty ownership (`ptyOwnership`,
`remote:`-prefix guards, spec §9.3) are the ssh command-exposure next action —
not this seam.

### 3.5 Feature-flag isolation gate

Both transports must never be live in the same page for the same pane class.
The gate is the factory itself plus two assertions:

1. `createLocalPtyTransport` selects by `isTauri()` exactly once per
   construction; there is no runtime migration path between transports for a
   live pane (a pane that needs a (c)-surface Electron channel must run under
   Electron entirely — enforced by dispatching the deferred methods against
   `window.api.pty` only, which under Tauri is `undefined`, making any
   accidental cross-wiring a loud `TypeError`, not silent misbehavior).
2. `pty-transport-test-harness.ts` / `pty-connection-test-environment.ts`
   instantiate transports explicitly, so unit tests pin each transport's
   capability set; a test asserting the Tauri transport's optional surface is
   exactly the (a)-row members guards against silent drift.

---

## 4. Verification plan

### 4.1 Unit tests (vitest, co-located)

1. **`pty-transport-factory.test.ts`** (new, next to the factory):
   - `isTauri() === false` → returns the object `createIpcPtyTransport`
     produces (identity via a spy-seeded module mock — assert the returned
     object's `sendInput`/`connect` behavior matches the Electron transport,
     and that no `@tauri-apps/api/core` import resolved).
   - `isTauri() === true` → returns the Tauri transport; assert its optional
     surface: `sendInputAccepted === undefined`, `claimViewport ===
     undefined`, `serializeBuffer === undefined` (the flag-isolation drift
     guard from §3.5), and `getPtyId() === null` before connect.
   - Exactly one selection per construction (call-count on the mocked
     selector).
2. **`pty-transport-tauri.test.ts`** (new): DTO mapping against the recorded
   `recordedInvoke` pattern from `pty-client.test.ts`:
   - `connect` → one `pty_spawn` with camelCase spec; `sendInput` →
     `pty_write`; `resize` → `pty_resize`; `disconnect` → `pty_kill`.
   - `onData` fan-out: a `PtyDataPayload` with two chunks produces two router
     events with `startSeq = seq - (rawLength ?? data.length)` and string ids.
   - id round-trip: `String(id)` out, `Number(id)` back on write/resize/kill.
   - exit: `PtyExitPayload` routes once, clears per-id state.
3. **Seq-dedupe against the pty-client DTOs** (extend
   `src/renderer/src/components/terminal-pane/terminal-pty-ack-gate.test.ts`):
   - Feed chunks whose `startSeq < processedChars` → skipped; equal → kept;
     transformed chunks (`rawLength` present) compute the overlap from the
     source span, not the delivered length.
   - `restoreMarker` present → baseline resets to `markerSeq`.
   - `ackData` absence (no Electron bridge) leaves `processedPtyCharTotals`
     advancing — the Tauri no-op delta from §2.3.

### 4.2 E2E smoke (Playwright, `tests/e2e/`)

Extend the pty smoke path (pattern: `golden-fresh-profile-terminal.spec.ts`):

- Launch the Tauri build in smoke mode (`smoke_test_mode`,
  `src-tauri/src/lib.rs:165`) with an isolated WebView2 data dir; add a
  `SmokeTestMode::Pty` variant whose entrypoint mounts one TerminalPane wired
  through `createLocalPtyTransport` under `isTauri() === true`.
- The pane spawns `cmd /c echo` (Windows shell selection per
  `pty_commands.rs:209-251`) with `cols: 80, rows: 24`; assert:
  1. rendered output contains the echoed text (proves spawn → `pty://data`
     fan-out → output processor → xterm render end-to-end);
  2. typing a line reaches the shell (`pty_write` round-trip visible in
     output);
  3. resize to `cols: 100` relayouts (proves `pty_resize` + applied size);
  4. killing the pane emits `pty://exit` once and the pane shows
     "Process exited" without a stray second exit event.
- Chunk-boundary stress: spawn a high-volume producer (`cmd /c type
  <large-file>`) and assert the seq-dedupe leaves no duplicated lines in the
  scrollback and no gap (first/last line present), bounding the §2.3 rule
  under real batcher behavior (2 ms / 16 KiB batches).

### 4.3 Contract pins (Rust, `src-tauri/tests/`)

- `pty_command_contract.rs`: each command's happy path + error shape through
  `DesktopCommandError` (codes from `PtyError::code()`), mirroring
  `pty-client.test.ts` DTO assertions so the two sides cannot drift.

---

## 5. Rollout order

1. Land the (b) command exposures needed by the smoke (`pty_ack_data`,
   `pty_get_size`) — thin `pty_commands.rs` additions over existing
   `PtySession` methods.
2. Land `pty-transport-factory.ts` + `pty-transport-tauri.ts` with the §4.1
   unit tests (Electron behavior provably unchanged — factory default).
3. Wire `pty-connection.ts:4026` to the factory.
4. Add `SmokeTestMode::Pty` and the §4.2 E2E assertions.
5. Pane-serializer slice (deferred pass) then flips `terminal.ui` to done.
