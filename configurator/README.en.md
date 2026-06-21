> 🌐 [中文](README.md) ｜ **English**

# Universal Configurator (configurator, User Side)

A desktop application (Tauri v2 + React 19 + TypeScript + Vite) for visually and safely editing multi-format config files, **LLM-free**, targeting non-technical users. Follows `../spec/cfgform-spec.md` v2.0 (format-agnostic universal config layer).

## Features

- On startup, scans the directory for `*.cfgform` sidecar files (compatible with legacy `*.jsonform`, treated as `format=json`), auto-pairing with target files via `target`.
- Top-level `format` field determines the format adapter: `json` / `env` / `toml` / `ini` / `yaml` / `compose`. The sidebar and form header display a format badge.
- Renders editable forms via RJSF (`schema` + `ui`), with AJV8 real-time validation (natively supporting `if/then/else`, `dependencies` conditional/cross-field constraints), required-field red stars, and Chinese error summaries.
- **Secret masking**: `ui:secret:true` fields rendered as password inputs with a "Show/Hide" toggle; their values never enter the audit log (only recorded as "modified (secret, value not recorded)"), masked by default in preview.
- **Read-only lock**: `ui:readOnly:true` fields are greyed out and annotated "🔒 Author Locked."
- **Default value diff / reset**: Lists fields differing from `schema.default`, provides per-field "Reset to default."
- **Multi-environment profiles**: When the sidecar contains `profiles { active, list, overrides }`, an environment switcher is displayed at the top; on load, `effective data = base target data ⊕ overrides[active]`; switching environments re-applies that environment's overrides on top of the base and re-renders. Provides a "Save current changes as overrides for [active] environment" button: writes the diff (current form vs base target) back to the sidecar `profiles.overrides[active]` (pretty-printed JSON, no BOM, `\n`), with an audit line "Updated sidecar profiles.overrides[active]", without corrupting other sidecar fields. `Save…` still writes effective values back to the single target file (backup + atomic write + audit).
- **Built-in schema library (out-of-the-box)**: Compile-time built-in curated templates for `package.json` / `tsconfig.json` / `docker-compose.yml` (also covering `docker-compose.yaml`, `compose.yml`). If these target files exist in a directory but lack an accompanying `.cfgform`/`.jsonform`, they are auto-paired with built-in templates (source marked "built-in library"), reading the real target file's current values to render an editable form; a banner "Using built-in schema library (project did not ship a .cfgform)" is shown at the top of the form; saving writes back to the target file as usual, and **does not auto-write a sidecar**. An optional button "Save built-in template as .cfgform in this directory" is also provided. Only when neither a sidecar exists nor the file matches the built-in library is the prompt "Missing form description file" shown.
- **Two-step save (Dry-run)**: First `preview_save` generates "the file text to be written + line-level diff" (secrets masked by default, revealable with warning) → upon confirmation, `commit_save` backs up `<target-name>.<UTC-timestamp>.bak` → atomic write (temp file → rename) → per-field Chinese audit log written to `cfgform-audit.log`.

## Per-Format Lossless Write-Back Fidelity (Stated Honestly)

| format | Read load | Write save | Comments / Order |
| --- | --- | --- | --- |
| json | ✅ | ✅ | No comments; preserves key order (serde_json `preserve_order` enabled), 2-space, UTF-8 no BOM, `\n` |
| env | ✅ | ✅ | Line-level preservation: only changed lines rewritten; comments/blank lines/order kept verbatim; new keys appended at end |
| toml | ✅ | ✅ | `toml_edit` surgical, preserves comments and order |
| ini | ✅ | ✅ | Line-level preservation; changed values rewritten in place; new keys appended (new root keys at top, new section keys at end) |
| yaml | ✅ | ✅ (value edits preserve comments) | Surgical line-level rewrite: comments/key order/anchors fully preserved; only **structural addition/removal of nested keys** triggers full-document fallback (⚠️, review Dry-run before saving) |
| compose | ✅ | ✅ (value edits preserve comments) | Reuses yaml adapter, same as above |

> YAML serialization uses "surgery + safety net": only inline-rewrites the line where the value actually changed (preserving indentation/key name/trailing inline comments); untouched lines are output verbatim; the rewrite result is re-parsed for validation — if semantically inconsistent with the target (complex structural additions/removals, type swaps, etc.), it automatically falls back to `serde_yaml` full serialization (data correct, but that save loses comments).

Format adapter implementations are in `src-tauri/src/adapters.rs`.

## Rust Commands (src-tauri/src/lib.rs)

- `default_scan_dir() -> String`
- `scan_dir(dir: String) -> Vec<PairInfo>` (`PairInfo` now includes `builtin: bool` and `source: String`; built-in library pair has `cfgform_path` empty, `builtin=true`, `source="内置库"`)
- `load_pair(cfgform_path: String) -> LoadResult`
- `load_builtin(target_path: String) -> LoadResult` (renders with built-in template, reads real target file current values; `profiles` always null)
- `preview_save(target_path, format, data) -> PreviewResult`
- `commit_save(target_path, format, data, secret_paths) -> SaveResult`
- `read_audit_tail(dir, max_lines) -> String`
- `save_profile_overrides(cfgform_path: String, active: String, overrides_value: Value) -> ()` (writes back to sidecar `profiles.overrides[active]`, atomic write + audit, non-destructive to other fields)
- `save_builtin_sidecar(target_path: String) -> String` (optional: saves built-in template as `.cfgform` alongside the target; returns the written path; errors without overwriting if already exists)

## Development & Verification

```pwsh
npm install
npm run build          # tsc && vite build
# cargo (cargo not on PATH, inject first)
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"; cargo check   # under src-tauri
```

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

---

Changelog:
- 2026-06-21 Completed three roadmap capabilities: ① YAML/compose surgical serialization (value edits preserve comments/key order/anchors; structural additions/removals only trigger fallback; fidelity promoted from ⚠️ to ✅); ② Full profiles multi-environment implementation (base⊕overrides[active] effective data, environment switching, `save_profile_overrides` sidecar override write-back, "experimental" label removed); ③ Built-in schema library auto-pairing (`load_builtin`, built-in library banner, optional sidecar save `save_builtin_sidecar`); `npm run build` 0 errors, `cargo check` passed.
- 2026-06-20 Refactored to v2.0 multi-format architecture: added `adapters.rs` (json/env/toml/ini/yaml/compose adapters), `preview_save`/`commit_save` two-step save, `.cfgform` sidecar (compatible with `.jsonform`), secret masking/read-only lock/defaults-reset/conditional validation/profiles/Dry-run preview; `npm run build` 0 errors, `cargo check` passed.
- 2026-06-20 Implemented user-side configurator: Rust file I/O five commands (scan/load/save/audit), backup+atomic write+audit trail, RJSF+AJV Chinese form UI; both `npm run build` and `cargo check` passed.
