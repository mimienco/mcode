# Quick Command Drag Insertion Plan

## Goal

Let developers reuse saved terminal commands and agent prompts without retyping them. A saved item can be dragged from the **Snippets** menu into either a terminal or the native chat composer. Dropping inserts text only: it never executes a shell command or sends a chat message.

## Product shape

- Keep **Command** and its terminal-specific persistence unchanged.
- Store **Snippets** separately as generic `{ id, name, text }` cards with no Terminal/Agent type, Append Enter setting, repository scope, or execution behavior.
- Show **Snippets** in project terminals, project chats, and no-project chats/terminals.
- Make every snippet card visibly draggable and expose the same plain-text drag payload to terminal and native-chat surfaces.
- Keep Copy as the accessible non-drag alternative and Edit/Delete as quiet card actions.
- Retain Copy as a secondary action for reuse outside MCode.

## Structure

```text
SavedSnippet { id, name, text }
        |
        v
quick-command-drag-data.ts
  - versioned internal MIME payload
  - bounded parser
  - text/plain interoperability
        |
        +----------------------+
        |                      |
        v                      v
Snippets launcher          TerminalQuickCommandsSubmenu
  insert/drag rows           draggable row
        |
        +----------------------+
                    |
          +---------+----------+
          |                    |
          v                    v
NativeChatComposerField   TerminalPane
  insert at caret           write to targeted pane
  keep draft unsent         omit Enter; keep unexecuted
```

## Behavior contract

1. Only MCode's custom MIME type activates an insertion target; arbitrary external text drops keep their existing behavior.
2. The payload is versioned and parsed defensively. Invalid, empty, or over-limit text is ignored.
3. Chat insertion uses the composer's public typed-insertion path so caret placement, draft persistence, autocomplete state, and focus stay consistent.
4. Terminal insertion resolves the pane under the drop target, sends the literal saved body without a carriage return, records user input activity, and focuses that pane.
5. Dragging an agent prompt and dragging a terminal command use the same body-text contract; the destination decides what that text means.
6. Existing file-drop behavior remains unchanged and can coexist on both surfaces.
7. Each destination owns eligibility. Mixed chat/terminal splits route by the exact leaf under the pointer, and a disabled composer cannot be bypassed by a source-side fallback.
8. Electron/xterm's missing-`drop` edge case uses an in-memory drag-finish handshake; only the eligible destination under the final coordinates may acknowledge and insert it.

## Verification

- Unit tests for drag payload round-trip and rejection boundaries.
- Component test that saved rows publish the drag payload.
- Native chat test that a drop inserts at the current caret without sending.
- Terminal test that a drop targets the correct split pane and never appends Enter.
- Renderer typecheck, targeted Vitest suites, full suite where practical, design-system detector, and rendered Electron verification.

## Non-goals

- No automatic execution on drop.
- No migration or reinterpretation of existing quick commands.
- No cloud sync changes, new remote protocol fields, or mobile drag-and-drop in this slice.
- No arbitrary external text-to-terminal drop, which would weaken the explicit trust boundary.
