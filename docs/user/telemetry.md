# Privacy & Telemetry

What anonymous usage data MCode collects, what it never collects, and how to opt out.

This page describes the anonymous product-usage telemetry collected in packaged MCode builds, what is never collected, and how to opt out.

## Summary

- **Anonymous.** Events are keyed by a random ID stored locally on your machine. No account, email, IP address, or user name is collected.
- **No content.** We never transmit file contents, prompts, agent output, terminal output, repo names, branch names, URLs, paths, or commit messages.
- **Always off when:** `DO_NOT_TRACK=1` or `MCODE_TELEMETRY_DISABLED=1` is set, or the app runs in CI.

## What we collect

Alongside each event we include basic build and platform information: the MCode version, your operating system (e.g. `darwin` / `win32` / `linux`), CPU architecture, coarse OS release string, release channel (`stable` or `rc`), and the anonymous local ID described above. No hostname, no username, no IP.

The categories of behavior we observe:

- **Lifecycle** — when the app opens. Used to estimate daily, weekly, and monthly active users.
- **Repos and workspaces** — when you add a repo or create a workspace. We record *how* you did it (e.g. folder picker vs. clone URL; command palette vs. drag-and-drop), never the repo name, URL, path, branch name, or any free-form text.
- **Agents** — when you start an agent, which agent kind it was (from a fixed list like `claude-code`, `codex`, `gemini`, etc.), and where you launched it from. Never the prompt, model details, or agent output.
- **Agent errors** — a coarse error category and which agent kind was involved. We never see raw error messages or stack traces; per-incident detail stays in a local diagnostic trace file on your machine and only reaches MCode if you explicitly share a diagnostic bundle.
- **Settings** — when you toggle one of a small whitelisted set of feature-flag or UX preferences. We record which preference changed and whether it's a boolean or an enum, never the raw value of any free-form setting.
- **Privacy controls** — when you opt in or out of telemetry, so we can tell from aggregate data whether the consent UI is working.

Every field transmitted is either a fixed enum value, a version string, or the anonymous local ID. No free-form strings from any UI input ever leave your machine.

## What we never send

- No file paths, repo names, branch names, URLs, commit messages, or current working directory.
- No agent prompts, responses, or terminal contents.
- No raw error messages or stack frames. Per-incident error context stays in a local diagnostic trace file on your machine and only reaches us if you explicitly share a diagnostic bundle.
- No user account information tied to your events.
- No IP address and no precise geoip; country is the only geographic signal derived from the request.
- No vendor-side user profile tied to your anonymous ID.

## How to opt out

You can disable telemetry in three ways. Any one of them is sufficient; they compose.

1. **In the app.** Settings → Privacy → toggle "Share anonymous usage data" off. The change is immediate and persistent.
2. **`DO_NOT_TRACK=1`** — community-standard environment variable. Disables transmission for that launch. Unsetting it restores your stored preference on the next launch.
3. **`MCODE_TELEMETRY_DISABLED=1`** — MCode-specific kill switch with the same semantics as `DO_NOT_TRACK`.

Running under CI (for example `CI`, `GITHUB_ACTIONS`, or `GITLAB_CI` present) also disables transmission.

To additionally stop local diagnostic file writes, set `MCODE_DIAGNOSTICS_DISABLED=1`.

## Where the data goes

- **Vendor:** PostHog Cloud, **United States** region.
- **Retention:** PostHog Cloud's plan-level default per their pricing page. No custom retention window is set.
- **Access:** project membership is restricted to a small number of MCode maintainers.

## Related

- [Settings reference](settings.md) — the Privacy pane.
- [Troubleshooting](troubleshooting.md) — logs and diagnostics.
