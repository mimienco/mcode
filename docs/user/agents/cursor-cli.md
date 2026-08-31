# Cursor CLI in MCode

Cursor CLI is Cursor's command-line agent. MCode runs it with first-class support — launch from the combobox, full OSC state detection, and restart chip on exit.

## Setup

1. Install Cursor CLI per [Cursor's docs](https://cursor.com/cli).
2. Log in once.
3. MCode auto-detects the CLI on `PATH`.

## Launching

Pick **Cursor** from the combobox. MCode launches the CLI scoped to the worktree. Cursor's TUI emits the state events MCode needs for agent state dots.

## Model selection

Model selection is driven by Cursor's own settings. MCode doesn't override it — configure inside the CLI.
