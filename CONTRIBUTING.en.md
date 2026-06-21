> 🌐 [中文](CONTRIBUTING.md) ｜ **English**

# Contributing

Thank you for contributing to **CfgForm**! This guide covers: dev environment setup, how to add a format adapter, build & verification commands, code style, how to submit a built-in schema library entry, and the PR process.

> Important: the field semantics of the `.cfgform` sidecar are governed by [`spec/cfgform-spec.md`](spec/cfgform-spec.md) (v2.0) as the **single source of truth**. Any PR that changes behavior **must update the spec first**, then the code and docs, to keep all three in sync.

---

## 1. Development Environment Setup

Prerequisites:

- **Node.js 18+**
- **Rust (stable; MSVC toolchain on Windows)**
- **WebView2 Runtime** (pre-installed on Windows 10/11)

The two apps are independent:

```pwsh
# User-side editor
cd configurator
npm install
npm run tauri dev

# Developer-side generator
cd prep-tool
npm install
npm run tauri dev
```

If `cargo` is not on PATH (first-hand experience), inject it first, then run backend checks:

```pwsh
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo check      # inside the app's src-tauri directory
```

---

## 2. How to Add a Format Adapter

Adapters are the only place in CfgForm coupled to a specific format. The core contract is just two functions (see `configurator/src-tauri/src/adapters.rs`):

```rust
pub fn parse(format: &str, text: &str) -> Result<serde_json::Value, String>;
pub fn serialize(format: &str, value: &Value, original: &str) -> Result<String, String>;
```

Steps to add a new format `xxx`:

1. **Register the format name**: accept `"xxx"` in `adapters.rs::normalize_format` (normalize aliases here too, e.g., `yml → yaml`).
2. **Implement parse**: write `fn parse_xxx(text: &str) -> Result<Value, String>` that parses text into a **canonical data tree** (`serde_json::Value`).
   - Note on type mapping: if all values in this format are strings (like `.env`), they must be strings in the data tree.
   - Empty content should return an empty object `Value::Object(Map::new())` rather than an error.
3. **Implement serialize (lossless-first)**: write `fn serialize_xxx(value: &Value, original: &str) -> Result<String, String>`.
   - **Must leverage `original` for minimal rewrites**: compare line-by-line or node-by-node, only rewrite the parts that changed, preserving comments, blank lines, and key order.
   - Line endings: unified `\n`, UTF-8 no BOM, ensure trailing newline.
4. **Wire into match branches**: in `adapters.rs::parse` and `serialize`'s `match normalize_format(format)`, add `"xxx" => ...`.
5. **Sync prep-tool**: add a branch in `prep-tool/src-tauri/src/lib.rs`'s `parse_config`, and add extension/content-sniffing rules in `detect_format_impl` (prep-tool only needs parse; lossless serialize is configurator's job).
6. **Honestly update docs**: in `spec/cfgform-spec.md §6` fidelity table, root `README.md` matrix, and `configurator/README.md`, **honestly annotate** the format's true "value round-trip / comment preservation / key order" status. **Do not exaggerate** — if comments cannot be preserved, mark ⚠️.
7. **Add samples**: add a real paired `xxx` + `xxx.cfgform` in `sample/`, and ensure the schema matches the target file fields and passes AJV validation.

> Existing adapters can serve as templates: `env`/`ini` (line-level format preservation), `toml` (`toml_edit` document-level preservation), `json` (`serde_json` `preserve_order`), `yaml` (surgical line-level rewrite; value edits preserve comments/key order/anchors, only deep structural add/remove falls back to full `serde_yaml` serialization).

---

## 3. Build & Verification Commands

Before submitting, run these in the directory of the affected app:

```pwsh
npm run build     # tsc + vite build — must have 0 TypeScript errors

$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo check       # inside src-tauri — must pass
```

Both must pass before opening a PR. For changes involving save logic, manually verify the **Dry-run preview → backup → atomic write → audit log** chain works correctly.

---

## 4. Code Style

- **Rust**: `cargo fmt` default style; errors return `Result<_, String>` with **Chinese human-readable** messages (consistent with existing commands); file I/O must use atomic write, backup, and never silently swallow errors.
- **TypeScript/React**: follow existing tsconfig strict settings, 0 compilation errors; refer to existing `src/App.tsx` for component patterns, naming, and `invoke` call style.
- **All user-facing text/errors/logs use Chinese human-readable language**, avoiding raw technical jargon that intimidates non-technical users.
- **Safety red lines are non-negotiable**: never hardcode any key; `ui:secret` values must not appear in logs/sidecar/preview plaintext; write-back must always backup first then atomically replace.

---

## 5. How to Submit a Built-in Schema Library Entry

`schemas/` collects carefully crafted `.cfgform` templates for common config files (e.g., `package.json`, `tsconfig.json`, `docker-compose.yml`). To submit a new entry:

1. Create `<common-filename>.cfgform` under `schemas/`. It must be **valid v2.0** (containing `$cfgform:"2.0"`, `target`, `format`, `title`, `schema` (draft-07), `ui`, `meta`).
2. The `schema` should be **moderately permissive** (a generic template will face diverse real files; recommend `additionalProperties: true`, keep `required` minimal), while providing `description`/`enum`/range and `ui:help` Chinese hints for common fields.
3. `ui` should add `ui:secret` for commonly sensitive fields (e.g., those containing token/password) and `ui:readOnly` for fields that should not be modified.
4. `meta.generatedBy` should be marked as human-curated (e.g., `"curated/cfgform-schemas"`), `meta.llm.used: false`.
5. Register the entry and its purpose in `schemas/README.md`'s inventory, and retain the usage instruction: "Copy next to your target file, rename to `<your-filename>.cfgform`."

> Note: auto-matching is already implemented in `configurator` for the three compile-time built-in templates (`package.json` / `tsconfig.json` / `docker-compose.yml`); new entries in `schemas/` primarily serve as **copyable templates** — to make an entry also benefit from auto-matching, its template must be embedded into the `configurator` binary (`include_str!`), so submitting an ordinary entry requires no auto-matching logic.

---

## 6. PR Process

1. Fork and create a feature branch from `main` (e.g., `feat/adapter-hcl`, `fix/env-quote`, `schemas/add-vite-config`).
2. If changing behavior: **update `spec/cfgform-spec.md` first**, then the code and all related docs.
3. Locally pass `npm run build` and `cargo check`; if the save chain is affected, manually verify it once.
4. Write clear Chinese or English commit messages describing "what changed, why, and the scope of impact."
5. In the PR description, check the self-review boxes: spec synced / build passes / fidelity table honestly updated / samples added (if applicable).
6. One PR focuses on one thing, for ease of review.

We look forward to your contribution!
