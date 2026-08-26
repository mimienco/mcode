# Reusable prompt and command palette

Research date: 2026-08-26

## Recommendation

Extend the existing **Quick Commands** feature into a cross-surface prompt/command palette. Keep one saved-item store and add target adapters for terminal and native chat; do not create a second snippet store or move text through the system clipboard.

Make **Insert** the common primary action. A drop performs the same insertion at the target caret and never sends the chat message or presses Enter in the terminal. Keep **Run** as a separate, explicitly labelled terminal action and **Copy** as an explicit secondary action. This makes drag an accelerator rather than the only way to use an item, as required by WCAG 2.2's dragging guidance and the keyboard-equivalence rule for interactive controls. ([WCAG 2.2 Dragging Movements](https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements), [Using ARIA, rule 3](https://www.w3.org/TR/using-aria/#third))

## What already exists in MCode

- [`TerminalQuickCommand`](../../src/shared/terminal-quick-command-types.ts) already represents terminal commands and agent prompts, with global/repository scopes.
- [`terminal-quick-commands.ts`](../../src/shared/terminal-quick-commands.ts) already owns normalization, limits, mutations, multiline flattening, and backward-compatible action defaults.
- [`TabBarQuickCommandsMenu.tsx`](../../src/renderer/src/components/tab-bar/TabBarQuickCommandsMenu.tsx) already provides a searchable tab-bar palette; [`TabBarQuickCommandItem.tsx`](../../src/renderer/src/components/tab-bar/TabBarQuickCommandItem.tsx) already supports Run, Copy, Edit, and Delete.
- [`terminal-quick-command-dispatch.ts`](../../src/renderer/src/components/terminal-pane/terminal-quick-command-dispatch.ts) and [`run-quick-command-in-new-tab.ts`](../../src/renderer/src/lib/run-quick-command-in-new-tab.ts) already distinguish insertion into a live PTY from an explicit new-tab run/agent launch.
- [`NativeChatComposer`](../../src/renderer/src/components/native-chat/NativeChatComposer.tsx) exposes `insertTypedText`, and [`use-native-chat-typed-insertion.ts`](../../src/renderer/src/components/native-chat/use-native-chat-typed-insertion.ts) already preserves selection, caret, controlled draft state, focus, and history reset.
- [`TerminalPane.tsx`](../../src/renderer/src/components/terminal-pane/TerminalPane.tsx) already resolves a drop to the pane under the pointer. Its terminal paste stack handles bracketed paste, control-sequence sanitization, remote transport limits, SSH/Windows differences, and stale-target protection.
- The repo already has bounded, versioned custom drag payload precedents in [`ai-vault-session-drag.ts`](../../src/renderer/src/lib/ai-vault-session-drag.ts) and [`linear-board-drag-payload.ts`](../../src/renderer/src/lib/linear-board-drag-payload.ts).

The smallest safe change is therefore a new insertion contract and drag codec around these seams, plus UI wording that broadens “Quick Commands” to prompts and commands.

## Interaction and accessibility contract

Use a labelled dialog/popover with a search combobox and a single-action result list: arrow keys move the active result, Enter inserts, and Escape closes. After insertion, focus moves to the destination caret. The WAI-ARIA listbox pattern explicitly does **not** support interactive descendants inside an option; if Copy/Edit/Delete remain inside each result row, render the results as an APG grid or move those actions to an adjacent toolbar/details region. ([APG Combobox](https://www.w3.org/WAI/ARIA/apg/patterns/combobox/), [APG Listbox](https://www.w3.org/WAI/ARIA/apg/patterns/listbox/), [APG Grid](https://www.w3.org/WAI/ARIA/apg/patterns/grid/), [APG Dialog](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/))

Each item also needs a native button reachable by Tab and activatable with Enter/Space. Dragging is optional mouse/pen affordance; the button supplies both the keyboard alternative and the non-dragging single-pointer alternative. Do not add `aria-grabbed` or `aria-dropeffect`; both properties are deprecated. ([ARIA `aria-grabbed`](https://www.w3.org/TR/wai-aria/#aria-grabbed), [ARIA `aria-dropeffect`](https://www.w3.org/TR/wai-aria/#aria-dropeffect))

If native HTML drag is used, keep the accessible action model above rather than trying to emulate keyboard dragging. `@dnd-kit` is useful when the application itself owns spatial movement/reordering and provides keyboard sensors and live announcements, but inserting text into existing xterm/textarea drop targets needs native `DataTransfer` interoperability. ([dnd-kit accessibility](https://dndkit.com/legacy/guides/accessibility/))

## Drag-and-drop and security contract

Define one codec, for example `application/x-mcode-prompt-command+json`, with an exact, versioned, bounded shape. Resolve an internal item ID against current authoritative state when possible; treat the payload and the `text/plain` fallback as untrusted input. Custom MIME types are routing hints, not an authentication boundary.

The HTML drag data store is writable during `dragstart`, protected during intermediate events, and readable during `drop`. Consequently:

1. In `dragstart`, synchronously set `effectAllowed = 'copy'`, the custom payload, and a `text/plain` fallback.
2. In `dragover`, inspect `dataTransfer.types` only; when supported, call `preventDefault()` and set `dropEffect = 'copy'`.
3. In `drop`, call `preventDefault()`/`stopPropagation()`, synchronously copy the needed strings out of `DataTransfer` **before any `await`**, then validate version, exact keys, item existence, character/byte limits, and accepted target type.
4. Never consume `text/html`. Never include secrets, environment values, credentials, or hidden metadata in either payload.

These lifecycle constraints follow the HTML Standard: `setData` only changes the read/write store, `getData` returns an empty string in protected mode, and the target must cancel `dragover`/`drop` to accept the operation. The browser withholds the drag body from documents crossed en route, but releases it to the final drop target, so the final target must still be treated as a disclosure boundary. ([WHATWG drag data store](https://html.spec.whatwg.org/multipage/dnd.html#the-drag-data-store), [DataTransfer](https://html.spec.whatwg.org/multipage/dnd.html#the-datatransfer-interface), [drag-and-drop processing](https://html.spec.whatwg.org/multipage/dnd.html#drag-and-drop-processing-model), [security risks](https://html.spec.whatwg.org/multipage/dnd.html#security-risks-in-the-drag-and-drop-model))

For terminal destinations, reject NUL and sanitize ESC/control sequences through the existing paste code. Do not append CR/LF on Insert or Drop. Multiline content must use the existing protected paste path, not raw `transport.sendInput`, because pasted newlines and bracketed-paste state can determine whether text is inserted or executed. xterm.js documents `Terminal.paste(data)` as the input method that performs paste transformations; `Terminal.write()` is the terminal-output path and is wrong for user insertion. ([xterm.js `Terminal.paste`](https://xtermjs.org/docs/api/terminal/classes/terminal/#paste), [xterm.js terminal API](https://xtermjs.org/docs/api/terminal/classes/terminal/))

## Direct insertion versus clipboard

Direct insertion should be the default:

- **Native chat:** call the existing `NativeChatComposerHandle.insertTypedText(body)` so React state, selection, caret, autocomplete, draft persistence, and undo/history behavior remain coherent.
- **Terminal:** capture the pane under the pointer, then route the text through the existing paste coordinator/bracketing/sanitization stack and refocus that xterm instance. Do not call `write()` and do not synthesize a DOM paste event.
- **Other native textarea inputs:** use the owning controlled-state API; for an uncontrolled native text control, `setRangeText()` is the standards-based selection replacement primitive. ([WHATWG `setRangeText`](https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#dom-textarea/input-setrangetext-dev))

Clipboard Copy remains useful for pasting outside MCode, but it should be explicit and retain the current preload IPC path and success/failure feedback. Clipboard APIs are permission/privacy-sensitive, and the W3C specification identifies self-XSS through copied shell commands as a security concern. Direct insertion also avoids overwriting whatever the user already has on the system clipboard. ([W3C Clipboard API](https://www.w3.org/TR/clipboard-apis/#security-considerations), [Electron clipboard](https://www.electronjs.org/docs/latest/api/clipboard))

## Implementation structure

1. **Domain and persistence:** retain the existing quick-command schema and migration behavior. Add only optional, backward-compatible presentation/target metadata if needed; keep the existing limits and host/repository ownership rules.
2. **Insertion service:** introduce a small target-neutral operation such as `insertSavedItem(item, target, source)` where `source` is `click | keyboard | drop`. Terminal and chat adapters own their platform-specific insertion and focus behavior. A distinct `runSavedItem` remains explicit.
3. **Drag codec:** add one shared MIME constant plus pure encode/decode/size-validation functions, following the existing AI Vault/Linear drag modules. Use one synchronous DOM listener boundary and pass decoded plain data to async adapters.
4. **Palette UI:** evolve the existing tab-bar Quick Commands button/menu rather than adding another launcher. Make Insert the result-row activation, expose Run and Copy as clearly separate actions, preserve configurable cross-platform shortcut handling, and restore/move focus deliberately.
5. **Drop targets:** add the custom MIME type alongside the existing file MIME checks in terminal and native-chat surfaces. Resolve the precise split pane/composer, show copy-cursor/drop highlighting, and never accept a drop while the target is disabled or stale.
   Keep destination eligibility authoritative if Electron/xterm omits the final DOM `drop`: a synchronous drag-finish handshake may carry final coordinates, but the source row must never insert on the destination's behalf.
6. **Verification:** unit-test codec version/size/tampering/protected-mode cases; caret/selection insertion; no-submit guarantees; multiline/control-character terminal handling; disabled/stale targets; SSH/remote transport; and IME. Add Playwright coverage for mouse drag plus keyboard/click insertion and focus restoration.

## Decision summary

- Reuse Quick Commands storage, host ownership, and palette UI.
- Insert directly into the destination; do not use clipboard as transport.
- Drop and default row activation insert only; Run is explicit.
- Use native HTML `DataTransfer` with a custom versioned type plus `text/plain` fallback.
- Route terminal text through MCode's paste safety stack and chat text through `insertTypedText`.
- Treat drag payloads as untrusted and potentially disclosed to the final drop target.
- Preserve accessible button/keyboard/click operation independent of dragging.
