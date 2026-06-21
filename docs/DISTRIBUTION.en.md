> 🌐 [中文](DISTRIBUTION.md) ｜ **English**

# Building & Cross-Platform Distribution (DISTRIBUTION)

> This document answers: "What exactly do I ship, how do I produce packages for each platform, and what does the user receive?" Automation lives at [`.github/workflows/release.yml`](../.github/workflows/release.yml).

## 1. What you publish

This is a **monorepo with two independent apps**, each targeting a different audience and released separately:

| App | Audience | Distributable | LLM / network |
| --- | --- | --- | --- |
| `configurator` | **End users** (non-technical) | Installer or standalone exe | No — 100% offline |
| `prep-tool` | **Developers / authors** | Installer or standalone exe | Yes (developer's own key) |

**Do NOT bundle these into the app installer**: `sample/`, `schemas/`, source code, `node_modules`, `产品经理Agent.md` — they are repo-side demos/templates/docs and ship with the GitHub repository; the app itself only needs its executable.

> The `schemas/` built-in templates are **compiled into the `configurator` binary** (`include_str!`), so users get auto-generated forms for `package.json`/`tsconfig.json`/`docker-compose.yml` with no extra files.

## 2. Standalone exe or installer? (important)

| Form | Pros | Caveats |
| --- | --- | --- |
| **Standalone exe** (`tauri build --no-bundle`) | Single file, no install, drop it next to the config and double-click | **Depends on the system WebView2 runtime**; preinstalled on Win10/11, may be missing on older systems → white screen |
| **NSIS/MSI installer** (`tauri build`) | **Detects and bootstraps WebView2**, registers Start-menu/uninstall entries, more professional | Slightly larger; **the first NSIS installer build downloads the NSIS toolchain from GitHub (needs network)**, and MSI likewise needs WiX |

**Conclusion**: For a real release, **prefer the installer** (it solves the WebView2 "platform support file" problem); you may additionally attach the standalone exe for advanced users.

> ⚠️ Honest note: **this project's build validation produced only the standalone exes so far** (`configurator.exe` ≈ 10.0 MB, `prep-tool.exe` ≈ 12.5 MB, via `tauri build --no-bundle`, depending on the system WebView2). Installers are a **documented capability** (local `tauri build`, or the GitHub Actions CI in §4 below) that was **not actually produced in this project's validation**.

## 3. Per-platform artifacts (must be built on the matching OS)

**Tauri cannot cross-compile macOS/Linux packages from Windows** — each platform's package must be built on that platform (or its CI runner).

| Platform | Artifacts | Prerequisites |
| --- | --- | --- |
| **Windows** | `*-setup.exe` (NSIS), `*.msi` (WiX) | Rust (MSVC) + WebView2; output under `src-tauri/target/release/bundle/{nsis,msi}/` |
| **macOS** | `*.dmg`, `*.app` | Build on macOS; signing + notarization (Apple cert) needed to avoid Gatekeeper warnings |
| **Linux** | `*.deb`, `*.rpm`, `*.AppImage` | Build on Linux; needs `libwebkit2gtk-4.1-dev` etc. (see CI) |

## 4. Recommended: one command, three platforms (GitHub Actions)

The repo ships [`.github/workflows/release.yml`](../.github/workflows/release.yml) (built on the official `tauri-apps/tauri-action`, matrix = two apps × three platforms). Usage:

```bash
git tag v1.0.0
git push origin v1.0.0
```

On a pushed tag, CI builds both apps on Windows/macOS/Linux runners and uploads the artifacts to a **draft GitHub Release** for you to review and publish. No need to set up all three toolchains locally.

## 5. Local build (Windows)

```pwsh
# Inside a VS Developer shell (to avoid the cygwin link.exe shadowing):
$vs = & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -latest -property installationPath
Import-Module (Join-Path $vs "Common7\Tools\Microsoft.VisualStudio.DevShell.dll")
Enter-VsDevShell -VsInstallPath $vs -DevCmdArguments "-arch=x64" -SkipAutomaticLocation
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

cd configurator           # or prep-tool
npx tauri build                    # NSIS + MSI installers (+ exe)
npx tauri build --bundles nsis     # NSIS installer only
npx tauri build --no-bundle        # standalone exe only
```

> ⚠️ Never use raw `cargo build` for a production build (it does not embed the frontend → white screen / "localhost refused to connect").

> 📡 **The first NSIS installer build needs network**: Tauri's NSIS bundler downloads the NSIS toolchain from GitHub on first build (MSI needs WiX); it fails in an offline environment. The standalone exe (`--no-bundle`) has no such network dependency.

## 6. Code signing (avoid security warnings — optional but recommended)

- **Windows**: an unsigned exe triggers a SmartScreen "unknown publisher" warning. With a code-signing certificate, set `bundle.windows.certificateThumbprint` in the app's `tauri.conf.json` (or sign in CI).
- **macOS**: an unsigned/un-notarized app is blocked by Gatekeeper. You need an Apple Developer certificate to sign + `notarytool` to notarize; inject via the `APPLE_*` secrets in CI (see comments in `release.yml`).

## 7. LLM config persistence (prep-tool)

So you don't reconfigure on every launch:

- **Non-secret settings (Base URL / Model)**: click "Save endpoint settings" to write them to `settings.json` in the user config dir; they **auto-fill on next launch** (never contains the key).
- **API key**: precedence is **in-UI temporary input > env var `CFGFORM_LLM_API_KEY` > `.env` in the program directory**. Click "Remember key to local .env" (with a safety confirmation) to persist it and skip re-typing, click "Clear saved key" to remove it, or set a system env var. `.env` is gitignored.
- Default recommendation is **DeepSeek** (`https://api.deepseek.com`, OpenAI-compatible; default model `deepseek-chat`, with a stronger DeepSeek "pro" model usable if you have one — the field is editable).

## 8. End-user usage

1. Download and run the `configurator` installer (or the portable exe).
2. Place it in (or point it at) the directory containing the target config file + its `.cfgform` sidecar.
3. Double-click → auto-scan → edit the form → Dry-run preview → save (auto-backup).

## 9. Release checklist (suggested)

- [ ] Version numbers in sync (each app's `package.json` and `src-tauri/tauri.conf.json` `version`).
- [ ] `LICENSE` and the `<Your Name>` placeholder at the end of the README replaced with your name.
- [ ] Generate checksums for installers (e.g. `Get-FileHash *.exe -Algorithm SHA256`).
- [ ] Release notes include: supported platforms, WebView2 note, changelog.

---
Changelog: 2026-06-20 Added the distribution guide — clarifying per-app distributables, standalone exe vs installer, three-platform packaging (incl. GitHub Actions automation), signing, and LLM persistence.

Changelog: 2026-06-21 Doc-accuracy pass — added the honest note that the first NSIS installer build downloads its toolchain from GitHub (needs network); clarified that only standalone exes (10.0/12.5 MB) were verified in this project and installers are a capability not yet produced; added the DeepSeek default model `deepseek-chat` and the "Clear saved key" button.
