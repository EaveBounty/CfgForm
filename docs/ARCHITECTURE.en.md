> 🌐 [中文](ARCHITECTURE.md) ｜ **English**

# Architecture & Design Principles

> Companion reading: the single source of truth spec [`../spec/cfgform-spec.md`](../spec/cfgform-spec.md). This document explains "why it is designed this way"; the spec defines "what must be done."

## 1. Design Philosophy

CfgForm is built on two core convictions:

### 1.1 Format-Agnostic Core + Semantic Sidecar (The Moat)

A config file's **physical format (JSON/YAML/.env…) is merely a serialization shell**. What is truly scarce, reusable, and worth long-term preservation is the **semantic annotation layer**:

- **Structural constraints**: field types, required fields, ranges, enums, regex patterns, inter-field conditions.
- **Human-readable hints**: what a field accepts, why, what pitfalls exist.
- **Authorial intent**: which fields should not be touched by users (read-only lock), which are sensitive secrets (masking).

This knowledge does not live in the file syntax, nor can it be reliably "auto-inferred" from source code — it lives in the author's head, in the README, in issue replies. CfgForm captures it once into a **`.cfgform` sidecar** (schema + ui + meta), format-agnostic and ecosystem-shareable. This is the moat: what others cannot copy is this layer of understanding.

### 1.2 Non-Polluting, Non-Intrusive; The Original File Is Sacred

- All metadata is consolidated into a **single `.cfgform` file**, recognized only by this ecosystem — regular users won't stumble upon it.
- The target config file **always keeps its original name and form**: edits are written back to the file itself, never renamed; automatically backed up before writes; best-effort preservation of comments/key order/formatting.
- Full audit trail: what was read, what was written, where the backup is, which field changed from X to Y — all recorded in plain-language audit logs.

---

## 2. Overall Architecture

CfgForm consists of two independent desktop applications that share a single spec and the same adapter contracts:

```
                 ┌──────────────────────────────────────────────┐
                 │            spec/cfgform-spec.md (v2.1)         │
                 │                 Single Source of Truth          │
                 └───────────────┬───────────────┬───────────────┘
                                 │ generate per spec │ consume per spec
                                 ▼                    ▼
   Developer Side                                         User Side
┌────────────────────────────────┐         ┌──────────────────────────────────────┐
│ prep-tool (has LLM, one network call) │         │ configurator (no LLM, 100% offline)       │
│                                │         │                                        │
│ detect_format / detect_stack   │         │ scan_dir / load_pair                   │
│ adapters::parse → data tree    │         │ adapters::parse → data tree             │
│ infer_schema (baseline)        │  .cfgform│ RJSF(schema+ui) + AJV real-time validation│
│ generate_metadata (LLM refine) │ ───────▶ │ preview_save (Dry-run + diff)          │
│ write_cfgform (sidecar only)   │  sidecar│ commit_save (backup→atomic write→audit)│
└────────────────────────────────┘         │ adapters::serialize → write to original │
        │ never modifies original file      └──────────────────────────────────────┘
        ▼                                                  │
   <target>.cfgform                                        ▼
                                               <target> + <target>.<ts>.bak + cfgform-audit.log
```

Both sides use **Tauri v2 + React 19 + TypeScript + Vite**. The frontend uses **RJSF (react-jsonschema-form)** for form rendering and **AJV 8** for validation; the backend uses **Rust** for file I/O and format adaptation.

---

## 3. Canonical Data Tree

This is the central concept of the entire system. **Any config file, regardless of format, is first parsed by its corresponding adapter into a neutral JSON value tree (`serde_json::Value` in Rust).** After that, all logic — schema validation, RJSF rendering, field diffing, secret identification — only operates on this tree, with zero knowledge of the original format.

| Stage | Data Form |
| --- | --- |
| Input | Original file byte stream (with comments, indentation, quotes, BOM, and other format details) |
| After parse | **Canonical Data Tree** (pure structured values: object/array/string/number/boolean/null) |
| During edit | Canonical Data Tree (mutated by the form) |
| After serialize | Original-format byte stream (adapter best-effort stitches format details back on) |

Key consequences (must understand, or schemas will be written incorrectly):

- **`.env` / `.ini` values are all strings in the data tree**. Even if the config says `PORT=8080`, after parsing it is the string `"8080"`, so the corresponding schema field type must be `string` (use `pattern` to constrain to numeric form) — **do not write `integer`**.
- **`toml` / `yaml` / `json` preserve native types** (integer, float, boolean, array, nested object).
- **`compose` reuses the yaml adapter**; its data tree is the YAML parse result.

`schema` and `ui` are always authored against this canonical data tree, decoupled from the target file's physical format.

---

## 4. Format Adapter Extension Points (parse / serialize Contract)

Each format only needs to implement one adapter, **responsible solely for "file text ⇄ canonical data tree"** — the rest of the core requires zero changes. Contract (see `configurator/src-tauri/src/adapters.rs`):

```rust
// text → canonical data tree (used by both prep-tool and configurator)
pub fn parse(format: &str, text: &str) -> Result<serde_json::Value, String>;

// canonical data tree + original text → text (only configurator write-back needs this; original is for lossless format preservation)
pub fn serialize(format: &str, value: &Value, original: &str) -> Result<String, String>;
```

Design highlights:

- **`serialize` receives `original` text** — this is the key to lossless write-back: the adapter diffs old vs. new data trees and **only rewrites the parts that changed**, leaving untouched comments/blank lines/order intact.
  - `env`/`ini`: line-level strategy, compare line by line, only value lines are rewritten, new keys appended.
  - `toml`: based on `toml_edit`'s `DocumentMut`, minimal mutations on the original document object — comments and order naturally preserved.
  - `json`: `serde_json` with `preserve_order`, 2-space indent, UTF-8 no BOM, `\n`.
  - `yaml`/`compose`: **surgical line-level rewrite** — only rewrites scalar lines that changed, preserving comments/anchors/order; only deep nested key add/remove or type change falls back to full document normalization (with correctness self-check safety net — data never incorrect).
- The true fidelity of each format **must be honestly documented** (see root README and spec §6) — this is the "data transparency" promise.

> **To add a new format** you only need to: register its name in `normalize_format` → implement `parse_xxx` / `serialize_xxx` → wire into the `parse`/`serialize` match branches → add extension mapping in `prep-tool`'s `detect_format_impl` → update the spec fidelity table. Detailed steps in [`../CONTRIBUTING.md`](../CONTRIBUTING.md).

---

## 5. Two-App Model & Data Flow Sequence

### 5.1 prep-tool (Developer Side, Generate Sidecar)

```
1. Select target file → detect_format (extension/content sniffing) → detect_stack (node/python/go/rust/generic; **context/labeling only — no source type-system extraction**)
2. adapters::parse → canonical data tree → infer_schema infers baseline Schema (**baseline types come from the config VALUES, not from source AST / TS interface / Pydantic / Go|Rust struct**; universal across formats)
3. gather_sources reads README (≤6000 chars) + up to 5 related source files (config/settings/schema keywords)
4. generate_metadata: sends "format + tech stack + baseline schema + source context" to OpenAI-compatible LLM,
   returns refined schema (supplemented description/enum/min/max/pattern/if-then) + ui (ui:help/enumNames/secret/readOnly)
5. suggest_secrets heuristic (key/token/secret/password/dsn/credential/private…) forces ui:secret,
   as a safety net even if the LLM missed it
6. write_cfgform: writes <target>.cfgform (append pairing) + appends to cfgform-audit.log; original file zero changes
```

### 5.2 configurator (User Side, Consume Sidecar, Two-Step Save)

```
1. default_scan_dir / scan_dir: scans directory for all *.cfgform (compatible with *.jsonform), pairs by target with config files
   (orphan target files — .json/.env/.toml… with no sidecar — are also listed with hint "Ask the author to use prep-tool")
2. load_pair: read sidecar → parse target file with format adapter → return {schema, ui, data, profiles}
3. RJSF(schema+ui) render + AJV real-time validation: required red asterisks, Chinese errors, secret masking, read-only greyed out, default diff highlighting
4. preview_save (Dry-run): serialize out "what will be written" + LCS line-level diff against current file (secrets masked by default)
5. User confirmation → commit_save:
      backup <target>.<UTC timestamp>.bak
      → write temp file .<name>.tmp → rename to replace (atomic, prevents half-written corruption)
      → structured per-field diff written to cfgform-audit.log (ui:secret fields only logged as "modified (secret, value not recorded)")
```

---

## 6. Technical Decision Rationale

### 6.1 Why Tauri Instead of Electron

| Dimension | Tauri v2 | Electron |
| --- | --- | --- |
| Artifact size | **~5–15 MB** | 80–150 MB (ships Chromium) |
| Startup speed | **Instant** | Slower |
| Rendering layer | Reuses system WebView2/WKWebView | Bundled Chromium |
| Backend | Rust (safe file I/O, strongly-typed adapters) | Node.js |
| Security sandbox | Capability whitelist | Requires manual hardening |

For a tool that "sits in a config directory, double-click to open, used by non-technical users," **small footprint, instant startup, offline, and secure** are hard requirements. Tauri satisfies all of them at once, and the Rust backend makes format adapter parsing/lossless write-back more reliable.

### 6.2 Why RJSF + AJV

- **RJSF (react-jsonschema-form)** is a mature "JSON Schema → form" rendering library, natively supporting `uiSchema` custom widgets (radio/range/password/select…), perfectly carrying `.cfgform`'s `ui` extensions.
- **AJV 8** is the de-facto standard JSON Schema validator, **natively supporting draft-07 `if/then/else`, `dependencies`, `allOf`**, making cross-field constraints like "`https` required when `mode=prod`" zero-cost to implement.
- Together they make "structure + validation + presentation" entirely driven by declarative `.cfgform`, so core code never needs per-config custom UI.

---

## 7. `.cfgform` Contract Essentials

A `.cfgform` is always valid JSON. Key fields (full definitions in spec §3, §4):

```jsonc
{
  "$cfgform": "2.0",            // spec version, required
  "target": "config.json",      // target filename in same directory, required
  "format": "json",             // json|env|toml|yaml|ini|compose, required (top-level is authoritative)
  "title": "Application Main Config",
  "schema": { /* JSON Schema draft-07: structure & hard constraints */ },
  "ui": { /* RJSF uiSchema + extensions: ui:help/ui:secret/ui:readOnly/ui:enumNames… */ },
  "profiles": { "active": "dev", "list": ["dev","prod"], "overrides": {} },  // optional: multi-environment overrides
  "meta": {
    "generatedBy": "prep-tool/2.0.0",
    "generatedAt": "2026-06-20T10:15:30Z",
    "stackDetected": "node",
    "llm": { "used": true, "model": "...", "note": "Constraints/help generated by LLM. Author should review." },
    "sources": ["README.md", "src/config.ts"]
  }
}
```

`ui` safety/experience extensions: `ui:secret` (secret masking), `ui:readOnly` (author lock), `ui:help` (Chinese human-readable hints), `ui:enumNames` (Chinese enum meanings), `ui:order`, `ui:widget`, `ui:placeholder`.

---

## 8. Security Model

| Mechanism | Implementation | Purpose |
| --- | --- | --- |
| Pre-write backup | `<target>.<UTC timestamp>.bak` (`fs::copy`) | Roll back mistaken edits |
| Atomic write | Write `.<name>.tmp` → `fs::rename` to replace | Prevent half-written file corruption |
| Dry-run two-step save | `preview_save` shows content+diff first; `commit_save` persists only after confirmation | Eliminate blind edits; what you see is what gets written |
| Key three-tier priority | UI temporary input (memory only) > env var `CFGFORM_LLM_*` (backward-compatible with `JSONFORM_LLM_*`) > exe-directory `.env` | Flexible without forcing disk persistence |
| Key zero-exfiltration | Keys never written to disk/log/`.cfgform`; `ui:secret` field values excluded from audit, preview masked by default; `.env` in `.gitignore` | Prevent leakage |
| Full audit trail | `cfgform-audit.log` human-readable record of filename/field path/non-secret value changes | Traceability and trust |
| Encoding standard | UTF-8 no BOM, `\n` line endings (env/ini/yaml preserve original line-ending policy) | Cross-platform consistency, prevent encoding errors |

> For per-app "read/write/send/never exfiltrate" checklists and samples, see [`PRIVACY.md`](PRIVACY.md).

---
Changelog: 2026-06-21 Doc-accuracy pass — the architecture diagram's spec label is now v2.1 (the sidecar `$cfgform` field value remains `"2.0"`, intentionally distinct); clarified that infer_schema's baseline types come from the config VALUES rather than a source type system/AST, and detect_stack is for context/labeling only.
