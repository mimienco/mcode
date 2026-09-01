# Bundle Config Parity — electron-builder → Tauri

Reference: `D:/ADE/orca/config/electron-builder.config.cjs` (read-only original).
Rewrite: `src-tauri/tauri.conf.json` + `config/vite.tauri.config.ts` (branch `rewrite/tauri-full`).

Status legend: **done** = implemented in this pass, **gap** = needs later work/credentials, **n/a** = Tauri equivalent does not exist.

## 1. Identity

| electron-builder | Tauri equivalent | State | Notes |
|---|---|---|---|
| `appId: 'com.mcode.desktop'` | `identifier: "com.mcode.desktop"` | **done** | Identical value, already present. |
| `productName: 'MCode'` | `productName: "MCode"` | **done** | Already present. |
| `extraMetadata.version` (dev-channel overrides via `MCODE_HOURLY/DAILY/ADHOC_BUILD_VERSION`, `MCODE_LOCAL_BUILD_VERSION`) | `version: "1.4.178"` (static) | **gap** | Tauri CLI has no equivalent of electron-builder's `extraMetadata` injection. Dev-channel builds need CI to rewrite `version` in `tauri.conf.json` (or pass `--config` with an override object) per channel, mirroring `devChannelBuildVersion` / `localBuildVersion` in the original config lines 109–113. |

## 2. Channels & publish

| electron-builder | Tauri equivalent | State | Notes |
|---|---|---|---|
| Channels: stable, RC (implicit), **hourly / daily / adhoc** dev channels, each mapped to a **separate GitHub repo** (`mcode`, `mcode-hourly`, `mcode-daily`, `mcode-adhoc`), dev channels published as `prerelease` | `crates/mcode-updater` `ReleaseChannel` enum: Stable, Rc, Hourly, Daily, Adhoc — one endpoint per channel, all HTTPS, all distinct (enforced by `ReleaseFeedConfiguration`) | **done** (design) / **gap** (values) | The channel model matches 1:1. The concrete feed URLs are runtime-injected by release CI (D-010) — see §6. |
| `publish: { provider: 'github', owner: 'stablyai', repo: devChannelRepo ?? 'mcode', releaseType }` | No declarative publish in `tauri.conf.json`; Tauri's GitHub-action publisher (v2 `tauri-action`) handles upload | **gap** | Publishing requires the GitHub token / `tauri-action` workflow wiring; nothing fake is checked in. Repo-per-channel selection must be a CI lane decision (see §7). |

## 3. Per-platform targets & artifact names

| electron-builder | Tauri equivalent | State | Notes |
|---|---|---|---|
| `mac.target`: `dmg` + `zip`, arch `x64, arm64` | `bundle.targets: "all"` | **gap** | `"all"` covers dmg/app/deb/nsis/msi. The electron-builder **zip** target (used by Squirrel.Mac / auto-update) has no Tauri analog — Tauri's macOS updater consumes the `.app.tar.gz` produced by `createUpdaterArtifacts` instead. If standalone zip artifacts are still needed for Homebrew/Casks, CI must tar manually. |
| `nsis` (Windows) — electron-builder implicitly, plus `win` block | `bundle.windows.nsis` | **done** (basics) | See §4. |
| `nsis.artifactName: 'mcode-windows-setup.${ext}'` | none | **gap** | Tauri 2 schema has **no `artifactName`** field at all (verified against `schema.tauri.app/config/2` — searched every definition). Bundler output names are fixed (e.g. `MCode_1.4.178_x64-setup.exe`, `MCode_1.4.178_x64_en-US.msi`, `MCode_1.4.178_aarch64.dmg`). If the old names are contractual (download links, Homebrew Cask, CI gates), CI must rename after bundling. |
| `dmg.artifactName: 'mcode-macos-${arch}.${ext}'` | none | **gap** | Same: no artifact-name control; CI renames or consumers adapt. |
| `appImage.artifactName: 'mcode-linux${, -arm64}.${ext}'` | none | **gap** | Same. |
| `deb.packageName: 'mcode-ide'`, `deb.artifactName: 'mcode-ide_${version}_${arch}.${ext}'` | `bundle.linux.deb` | **gap** | Tauri derives the deb package name from `productName` (→ `mcode`/`MCode`), not `mcode-ide`. The original deliberately avoids claiming the GNOME `mcode` package name (Ubuntu ships GNOME MCode as `mcode`). Options: set `productName` for the linux lane via `--config` override, or accept a new package identity. Needs a product decision. |
| `deb.depends` / `rpm.depends` (`python3`, `xvfb`, `xdotool`, `xclip`, `at-spi2-core`, …) | `bundle.linux.deb.depends` / `bundle.linux.rpm.depends` | **gap** | Fields exist in the schema but the Tauri shell has no serve-sim/xvfb headless-serve port yet, so the dependency set is not yet meaningful. Revisit when the `mcode serve` headless path is ported (original config lines 520–535). |
| `deb.afterInstall/afterRemove`, `rpm.afterInstall/afterRemove` (`resources/linux/packaging/after-{install,remove}.sh` — symlink CLI onto PATH) | `bundle.linux.deb.preInstallScript/postInstallScript/…` | **gap** | Scripts exist in `resources/linux/packaging/` but reference the Electron-era `mcode-ide` CLI layout; port after the CLI story lands. |
| `linux.executableName: 'mcode-ide'`, `StartupWMClass: 'mcode'` | `mainBinaryName` (top-level, unset) | **gap** | Tauri uses `productName` ("MCode") as the binary name. WM_CLASS mismatch matters only when the Linux desktop port is exercised. |
| `rpm` target | included in `targets: "all"` | **done** | Original ships AppImage+deb only; "all" additionally produces rpm/msi — harmless superset, can be narrowed to `["nsis","msi","dmg","app","deb","appimage"]` if exact matching is required. |
| `win.executableName: 'MCode'` | binary name from `productName` | **done** | Matches. |
| `directories.buildResources: 'resources/build'` (icons) | `bundle.icon: ["icons/icon.png","icons/icon.ico","icons/icon.icns"]` | **done** | `src-tauri/icons/` already contains the generated icon set. |
| `win.icon: 'resources/build/icon.ico'` / `mac.icon: 'resources/build/icon.icns'` | same icon list | **done** | Tauri picks the right icon per platform. |

## 4. NSIS (Windows installer)

| electron-builder | Tauri equivalent | State | Notes |
|---|---|---|---|
| (default one-click, per-user install; HKCU) | `bundle.windows.nsis.installMode: "currentUser"` | **done** | Set in this pass — matches the original's per-user default (HKCU metadata), avoids UAC. Original never sets `perMachine`; Tauri `both`/`perMachine` would change the update flow. |
| `nsis.shortcutName: '${productName}'` / `createDesktopShortcut: 'always'` / `uninstallDisplayName` | none | **n/a** | Tauri NSIS always creates desktop/start-menu shortcuts from `productName`; no config surface. |
| `nsis.deleteAppDataOnUninstall` (explicitly `false`) | none | **n/a** | Tauri NSIS never deletes app data on uninstall — the original's intent (keep durable profile under userData) is preserved by default. |
| `nsis.include: nsis/daemon-host-uninstall.nsh` (stop/remove relocated terminal daemon on real uninstall, guarded by `${isUpdated}`) | none | **gap** | Tauri supports `installerHooks` (schema key) for custom NSIS hook logic. Requires porting the `.nsh` logic into a Tauri NSIS hook template once the relocated daemon exists in the Tauri app. Not safe to set now (references Electron-era paths). |

## 5. Protocols / deep link

| electron-builder | Tauri equivalent | State | Notes |
|---|---|---|---|
| `protocols: [{ name: 'MCode', schemes: ['mcode'] }]` | `plugins.deep-link.desktop.schemes: ["mcode"]` | **done** | Already present. Tauri registers the `mcode` scheme via the deep-link plugin on Windows (registry) / macOS (Info.plist) / Linux (desktop file). |

## 6. Updater

| electron-builder | Tauri equivalent | State | Notes |
|---|---|---|---|
| electron-updater + `app-update.yml` baked into install; Authenticode `publisherName: 'SignPath Foundation'` verification (dropped for unsigned dev channels via `verifyUpdateCodeSignature: false`) | `tauri-plugin-updater` 2.10.1 + `mcode-updater` crate (minisign signature verification; no OS-signature check concept) | **done** (mechanism) / **gap** (trust-chain decision) | The original relies on Authenticode publisher verification; Tauri verifies the **payload signature** (minisign) instead. Windows dev channels in the original ship unsigned — with Tauri, every artifact must be minisign-signed at release time instead. Decision on how "unsigned dev channel" maps to Tauri's always-signed model is pending (UPDATER_AND_RELEASE.md). |
| Feed: GitHub releases atom feed per channel repo (`mcode`, `mcode-hourly`, `mcode-daily`, `mcode-adhoc`) | `plugins.updater.endpoints: []` + `pubkey: ""` (static bootstrap) | **gap** | **Deliberately empty placeholders — do not fake.** Per D-010 and UPDATER_AND_RELEASE.md: `mcode-updater` requires a non-empty minisign public key and 5 distinct HTTPS endpoints (stable/rc/hourly/daily/adhoc) injected at runtime by release CI; the static config stays empty because the plugin requires *some* config object at init. Missing config yields the structured `updater_not_configured` error. Where real values go (needs credentials/CI): ① `pubkey` ← output of `tauri signer generate` (CI secret, injected via `--config` or env at build); ② `endpoints` ← one `https://github.com/stablyai/{mcode,mcode-hourly,mcode-daily,mcode-adhoc}/releases/latest/download/latest.json`-style URL per channel (exact form set by the release workflow that publishes `latest.json`). |
| `bundle.createUpdaterArtifacts` equivalent: electron-builder auto-generates `latest.yml` / `latest-mac.yml` blocks | `bundle.createUpdaterArtifacts: true` | **done** | Already present; produces `.sig` + `latest.json` inputs for the feeds. |
| Update install: NSIS re-run + `uninstallOldVersion` | `UpdateInstall` shutdown intent → atomic install via shutdown checkpoint protocol (D-009/D-010) | **done** (design) | Not a bundle-config item; recorded for parity. |

## 7. Resources bundled into the app

Original `extraResources`/`asarUnpack` (Electron `process.resourcesPath`) vs Tauri `bundle.resources` (resolved at runtime via `PathResolver::resource_dir()`).

All four resource trees **exist** under the rewrite repo root `resources/` and are now declared (map form, so directory structure lands under `$RESOURCE/<name>` exactly like the original `from/to` pairs):

| Original extraResources entry | Original `to` | Tauri `resources` entry (added) | State |
|---|---|---|---|
| `resources/notification-sounds` (win/mac/linux common set — reached via `asarUnpack: ['resources/**']` and consumed with `?asset` imports / packaged sound selection) | `resources/notification-sounds` → `$RESOURCE/notification-sounds` | `"../resources/notification-sounds": "notification-sounds"` | **done** |
| `resources/plugins/launch` (`bundledPluginResources` — immutable install inputs, verified byte-exact at startup) | `plugins/launch` | `"../resources/plugins/launch": "plugins/launch"` | **done** |
| `resources/onboarding/feature-wall` (`featureWallResources`) | `onboarding/feature-wall` | `"../resources/onboarding/feature-wall": "onboarding/feature-wall"` | **done** |
| `resources/skills` (`skillFreshnessResources` — identity metadata for freshness detection) | `skills` | `"../resources/skills": "skills"` | **done** |

Map form is required, not slice form: with `"../resources/x"` in a plain array, `tauri-utils` `resource_relpath` turns each `..` into a literal `_up_` directory under `$RESOURCE` (verified in `tauri-utils-2.9.3/src/resources.rs::resource_relpath`); the map form joins stripped paths under the declared destination.

**Not carried over (still gaps, intentionally):**

| Original entry | Why not set now | Follow-up |
|---|---|---|
| `out/relay` (`relayExtraResource`) — SSH relay deploy resolves bundles from `process.resourcesPath` | Build output of the Electron relay build lane; the Tauri relay story is not ported yet. | Bundle once the relay asset build exists in this repo's build graph. |
| `createPackagedRuntimeNodeModuleResources(...)` → `Resources/node_modules` (runtime dep closure for main bundle, packaged CLI, SSH paths, speech worker) | Electron-specific: exists because Electron runs Node code from the asar/unpacked layout. Tauri moves this logic into Rust crates + sidecars; there is no `node_modules` runtime in the shipped app. | **n/a for Tauri** — replaced by Rust crates and, eventually, sidecar binaries. |
| `node_modules/sherpa-onnx-{darwin-${arch},linux-${arch},win-x64}` (speech native DLLs/addons) | See decision below. | See decision below. |
| `resources/{win32,darwin,linux}/bin/*` CLI launchers, `agent-browser-*`, `serve-sim`, `MCode Computer Use.app`, computer-use runtimes, notification-status/keyboard-layout helpers | All are Electron-process or native-helper concerns not yet ported. | Per-feature parity rows; each needs a Tauri `bundle.externalBin` (sidecar) or resource entry when ported. |

### Sherpa-onnx (speech) decision

The renderer/main flow **fetches speech models at runtime** (`src/main/speech/model-download-catalog.ts` downloads pinned files from HuggingFace into a local model cache — this is already the Electron behavior too; models were never shipped in the bundle). What *is* shipped in Electron is the **sherpa-onnx native library** (`sherpa-onnx-c-api.dll`, `onnxruntime.dll`, platform packages under `node_modules/sherpa-onnx-{win,darwin,linux}-*`) that the speech worker loads via `getSherpaModulePath()` (`process.resourcesPath/node_modules/<pkg>` when packaged).

**Decision: the sherpa native DLLs must be IN the Tauri bundle.** The FFI spike (`crates/mcode-voice`, delay-loaded `sherpa-onnx-c-api.dll`, resolved via `MCODE_SHERPA_LIB_DIR` or `<workspace>/node_modules/sherpa-onnx-win-x64`) proves the linkage; a packaged Tauri app must have the DLLs next to the binary or under `$RESOURCE`. The safe edit is **not made here** because:

1. The DLLs live in `node_modules/` (not `resources/`), are pnpm-symlinked, and their packaged location in the Tauri process model (sidecar vs `$RESOURCE` vs beside exe) is undecided pending the voice feature port.
2. Pointing `bundle.resources` at a `node_modules` path creates a fragile build-time dependency on pnpm install state.

Follow-up (voice parity row): add `bundle.resources` entry or `externalBin` sidecar for `node_modules/sherpa-onnx-win-x64` (and per-platform packages), plus the runtime lookup change from `sherpa_lib_dir()`'s workspace walk to the packaged path.

## 8. Other original config machinery (n/a to declarative Tauri config)

- `asarUnpack` — no asar in Tauri; frontend ships as plain files (`frontendDist: ../out/tauri-renderer`), backend is compiled Rust. **n/a.**
- `files: [...]` exclusion list — `vite.tauri.config.ts` builds only `src/renderer` into `out/tauri-renderer`; nothing else enters the bundle. **n/a.**
- `afterPack` verifiers (`verifyPackagedPluginResources`, `verifyPackagedDaemonEntryBoots`, `verifySkillsCliRuntime`, glibc floor, chmods, mac helper signing) — Electron packaging hooks. Tauri equivalents are CI-side artifact checks. **gap for CI**, not for `tauri.conf.json`.
- `beforeBuild` native rebuild / `npmRebuild` — no Node native modules ship in Tauri. **n/a.**
- macOS `extendInfo` (usage descriptions), `entitlements.mac.plist`, hardened runtime, notarization | Tauri `bundle.macOS`: `entitlements`, `infoPlist` keys exist in schema | **gap** — requires Apple credentials; set in release CI config (see §9), not checked in.
- `forceCodeSigning` | Tauri Windows signing via `bundle.windows.certificateThumbprint`/`signCommand` (schema), macOS `signingIdentity` | **gap** — credentials only; deliberately not set (would break local unsigned builds like the original's dev channels).

## 9. Needs credentials / CI (summary)

1. **Updater pubkey + endpoints** — minisign keypair (release CI secret) → `plugins.updater.pubkey`; per-channel feed URLs → `plugins.updater.endpoints` (injected at build time via `--config` override; static config stays empty by design, D-010).
2. **Windows code signing** — SignPath / certificate thumbprint → `bundle.windows.certificateThumbprint` + `timestampUrl`; or a `signCommand`. Dev-channel "unsigned" lane needs a policy decision (§6).
3. **macOS signing/notarization** — `APPLE_CERTIFICATE`, team ID, notary credentials → `bundle.macOS.signingIdentity`, `providerShortName`, `entitlements`, plus `infoPlist` usage-description strings ported from the original `extendInfo`.
4. **Publish pipeline** — `tauri-action` (or equivalent) GitHub workflow: repo-per-channel selection (`mcode` / `mcode-hourly` / `mcode-daily` / `mcode-adhoc`), `prerelease` flag for dev channels, `latest.json` publication, artifact gates (§3 rename step if old artifact names are contractual).
5. **Dev-channel version stamping** — CI rewrite of `version` per channel (replaces `extraMetadata`).
