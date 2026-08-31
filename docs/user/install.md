# Install

Download MCode for macOS, Windows, or Linux, and opt into RC builds.

## Download

MCode is a desktop app. Grab the build for your platform from the download page:

- **macOS:** Apple Silicon (`.dmg`) · Intel (`.dmg`)
- **Windows:** installer (`.exe`)
- **Linux:** AppImage · `.deb`

Older versions are kept on the releases page.

## First launch

On first launch MCode will:

- Ask for access to your home directory so it can add repos.
- Offer to import `~/.claude`, `~/.codex`, and Ghostty terminal settings if present.
- Drop you on an empty landing screen where you add your first repo.

## Updates

MCode auto-updates by default, tracking the **stable** channel. Stable releases are vetted; **RC (release candidate)** builds ship new features first, often daily.

There is no permanent in-app opt-in for the RC channel. Modifier clicks on **Check for Updates** ([Settings → General → Updates](settings.md), or the app / Help menu):

| Modifier                                               | Effect                                                                        |
| ---------------------------------------------------- | ----------------------------------------------------------------------------- |
| **Shift+click**                                        | Include the latest **RC** prerelease                                          |
| **Cmd+click** (macOS) / **Ctrl+click** (Windows/Linux) | Latest **perf**-tagged prerelease                                             |
| **Option+click** (macOS only)                          | Pick a **validated local macOS build** that passes MCode's compatibility checks |

You can still download any build directly from the releases page.

## Platform notes

### macOS

Signed and notarized. On first launch, macOS may still ask you to confirm — that's normal for Electron-based apps.

### Windows

The default shell can be set to PowerShell or CMD under [Settings → Terminal](settings.md). Most users want PowerShell.

### Linux

AppImage and `.deb` builds are available. See the releases page for details.
