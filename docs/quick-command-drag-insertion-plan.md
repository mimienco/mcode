# Quick Command Drag Insertion Plan

## Goal

Let developers reuse saved terminal commands and agent prompts without retyping them. A saved item can be dragged from the existing Quick Commands menu into either a terminal or the native chat composer. Dropping inserts text only: it never executes a shell command or sends a chat message.

## Product shape

- Keep **Quick Commands** as the single saved-item library and persistence model.
- Keep the existing project/global scopes, local/remote host ownership, search, edit, copy, and delete flows.
- Keep global commands available in folder workspaces and floating terminals even when no repository scope exists.
- Make every command row visibly draggable and expose the same drag payload to terminal and native-chat surfaces.
- Make result activation insert by default for keyboard, touch, and pointer users; keep **Run** as a separate explicit row action.
- Retain Copy as a secondary action for reuse outside MCode.

## Structure

```text
Saved TerminalQuickCommand
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
TabBarQuickCommandItem    TerminalQuickCommandsSubmenu
  draggable row             draggable row
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
- No second persistence store or migration of existing quick commands.
- No cloud sync changes, new remote protocol fields, or mobile drag-and-drop in this slice.
- No arbitrary external text-to-terminal drop, which would weaken the explicit trust boundary.
