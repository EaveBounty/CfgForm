> 🌐 [中文](README.md) ｜ **English**

# Runnable Samples (sample)

This directory contains **real paired samples across 4 formats**, each pair being a "target config file + a valid `.cfgform` v2.0 sidecar." Opening this directory with `configurator` immediately shows forms, real-time validation, secret masking, read-only locking, conditional validation, and all other features.

## Directory Contents

| Target File | Sidecar File | format | Demo Highlights |
| --- | --- | --- | --- |
| `config.json` | `config.json.cfgform` | `json` | Enums / numeric ranges / required / defaults / `ui:secret` (apiKey) / **conditional validation: when `mode=prod`, `https` is required and must be true** |
| `.env` | `.env.cfgform` | `env` | All string values / `ui:secret` masking (`API_KEY`, `DB_PASSWORD`) / enum radio / line-level preservation |
| `app.toml` | `app.toml.cfgform` | `toml` | Nested tables / `ui:readOnly` author lock (`version`) / `ui:secret` (`database.url`) / lossless comment & order preservation |
| `docker-compose.yml` | `docker-compose.yml.cfgform` | `compose` | Nested services / port & volume arrays / `ui:secret` (`db`'s `POSTGRES_PASSWORD`) / YAML value edits surgically preserve comments/key order/anchors (only deep structural add/remove falls back to full-document normalization) |

## How to Open This Directory with configurator for Verification

`configurator` in **dev mode** defaults to scanning the "current working directory," and in **production build** scans the "directory of the executable." Pick either:

### Method A: Dev mode pointed at this directory (fastest)

Start dev mode from the `configurator` directory; it will scan its launch working directory. The simplest approach is to switch the working directory to `sample/` first, then launch:

```pwsh
# From repo root
cd sample
npm --prefix ..\configurator install
npm --prefix ..\configurator run tauri dev
```

> Note: `default_scan_dir` returns `current_dir()` in debug builds. If your launch script fixes the working directory, you can manually change it to the absolute path of this `sample/` via the "Directory" input in the app and re-scan.

### Method B: Place the built app into this directory (closest to real user scenario)

```pwsh
# First build the user-side editor
npm --prefix ..\configurator run tauri build
# Copy the output (executable under src-tauri/target/release/bundle/) into this sample/ directory and double-click to run
```

After launch, the program will scan the 4 `.cfgform` files in the same directory and render each as a form.

## What You Should See

- 4 configs presented as **visual forms**, required fields marked with red stars, Chinese `ui:help` hints below each field.
- In `config.json`, changing `mode` to **production (prod)** immediately makes `https` required and must be enabled (conditional validation in effect).
- `apiKey`, `.env`'s `API_KEY`/`DB_PASSWORD`, `app.toml`'s `database.url`, and compose's `POSTGRES_PASSWORD` are all displayed as **`••••••` masked**, with a clickable show/hide toggle.
- `app.toml`'s `version` field is **greyed out and read-only**, annotated "Author Locked."
- Clicking save first enters a **Dry-run preview** (shows the text to be written + line-level diff, secrets masked by default); upon confirmation, it backs up + atomically writes back, generating `cfgform-audit.log` and `*.bak` in this directory.

> These `.bak` and `cfgform-audit.log` files are runtime artifacts and are ignored via the repo root `.gitignore`.
