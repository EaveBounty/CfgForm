> 🌐 [中文](PRIVACY.md) ｜ **English**

# Data Transparency & Privacy

> CfgForm is designed under the premise that "users may not trust software to touch their configs." Therefore we **lay out every step of read, write, and network behavior transparently**, and leave an audit trail. This document is a complete, honest statement of exactly what data the program touches.

## One-Sentence Overview

- **User-side `configurator`: 100% offline, zero network requests, no LLM.**
- **Developer-side `prep-tool`: a single network request** — to the developer's own configured OpenAI-compatible endpoint, to generate the `.cfgform` sidecar.
- **No key is ever written to disk, logs, or `.cfgform`.**

---

## 1. `configurator` (User-Side Editor)

| Item | Content |
| --- | --- |
| **What it reads** | `*.cfgform` (compatible with `*.jsonform`) sidecar files in its directory; and their paired target config files (`config.json`/`.env`/`app.toml`/`docker-compose.yml`…). Only these — does not scan subdirectories or read unrelated files. |
| **What it writes** | ① Edited values written back to **the target config file itself** (original name and form, atomic write); ② A **timestamped backup** `<target-name>.<UTC timestamp>.bak` before writing; ③ The **audit log** `cfgform-audit.log` in the same directory; ④ **Only when you explicitly click** "Save as environment override" or "Save built-in template as .cfgform" does it write/update a `.cfgform` sidecar in the same directory (purely local operation, still no network). |
| **What it sends over the network** | **Nothing. Fully offline. Makes no network requests. Contains no LLM calls.** |
| **What it never exfiltrates** | Everything. Config contents, keys, file paths all stay within the local filesystem. |

### User-Side 100% Offline Statement

All of `configurator`'s Rust commands only perform local file I/O: `default_scan_dir` / `scan_dir` / `load_pair` / `preview_save` / `commit_save` / `read_audit_tail`. **None of them initiate a network request**; the source code has no dependency on any HTTP client. You can disconnect from the network and verify this yourself.

---

## 2. `prep-tool` (Developer-Side Generator)

| Item | Content |
| --- | --- |
| **What it reads** | ① The target config file you select; ② The project README (truncated to ~6000 characters); ③ Up to 5 related source files (filenames containing `config`/`settings`/`schema`, extensions `.ts/.py/.go/.rs/.json`, each truncated to ~3000 characters; automatically skips `node_modules`/`.git`/`target`/`dist`/`build`/`.venv`/`__pycache__`); ④ Marker files for tech stack detection (`package.json`/`tsconfig.json`/`pyproject.toml`/`requirements.txt`/`go.mod`/`Cargo.toml`). |
| **What it writes** | ① `<target-filename>.cfgform` (append pairing); ② appends a record to `cfgform-audit.log`; ③ **only when you click "Save endpoint settings (no key)"** does it write the non-secret Base URL/Model to `settings.json` in the OS app-config directory (**never contains the key**, auto-filled on next launch to prefill the UI); ④ **only when you explicitly click "Remember key to local .env" (with a safety warning before writing)** does it write the key in plaintext to the `.env` in the exe directory (`.env` is gitignored). **Never modifies your original config file.** |
| **What it sends over the network** | **A single** HTTP request → see §2.1 below. |
| **What it never exfiltrates** | The LLM key (only sent as an `Authorization: Bearer` header with that one request — **not in the payload, never written to disk, never in logs/sidecar**). Other than that one request to the endpoint **you configured yourself**, no data is sent to any third party. |

### 2.1 The Sole Network Request

| Dimension | Content |
| --- | --- |
| **Triggered by** | Developer clicking "Generate" in the UI (`generate_metadata` command). |
| **Target** | `<your configured Base URL>/chat/completions` (OpenAI-compatible; **default recommended: DeepSeek `https://api.deepseek.com`**; can also be changed to OpenAI, local models, or any compatible endpoint). |
| **Method / Auth** | `POST`, `Authorization: Bearer <your key>`. |
| **Request body (payload)** | A chat-completions request: `model`, `temperature: 0.2`, `response_format: {type: json_object}`, plus messages — system prompt + user content. **User content includes**: target file format, tech stack, baseline JSON Schema inferred from config values, README excerpt, selected source file excerpts. In other words: **your config structure and selected source/docs will be sent to the LLM you designate**. |
| **Response handling** | Takes `choices[0].message.content`, defensively parses out `{schema, ui}`, assembles into `.cfgform` and writes to disk. |

> Therefore: whether to go online, where to send data, and which model to use is **entirely up to the developer's own Key/BaseURL/Model choice**. The user side is completely unaware of this and will never trigger any network activity.

---

## 3. LLM Key: Three-Tier Priority & "Never Touches Disk" Guarantee

`prep-tool` resolves keys with the following priority (see `resolve_key`):

1. **UI temporary input** (highest) — only in memory, lost on close, leaves no trace.
2. **System environment variables** — `CFGFORM_LLM_API_KEY` / `CFGFORM_LLM_BASE_URL` / `CFGFORM_LLM_MODEL` (backward-compatible with legacy `JSONFORM_LLM_*` names).
3. **`.env` file in the program's install directory** (lowest).

> **Non-secret persistence**: the Base URL / Model can be written via the "Save endpoint settings (no key)" button to `settings.json` in the OS app-config directory and auto-filled on launch — `settings.json` **never contains the key**. The UI has three buttons: "Save endpoint settings (no key)", "Remember key to local .env" (opt-in; writes the plaintext key to the exe-directory `.env` after a safety warning), and "Clear saved key".

Guarantees:

- Keys are **never written to disk** (except the one you yourself placed in the exe directory's `.env`, protected by your directory permissions).
- Keys are **never written to the audit log**, **never written to `.cfgform`**, **never placed in the LLM request payload body** (only as the HTTP `Authorization` header).
- `.env` is listed in `.gitignore` to prevent accidental commits.

---

## 4. Audit Log Format Samples

### 4.1 `configurator` Save Trail (User Side)

`commit_save` appends to `cfgform-audit.log` (secret fields only recorded as "modified", value not stored):

```
==== 2026-06-20T10:15:30Z Config Saved ====
Target file: config.json (format: json)
Backup file: config.json.20260620T101530Z.bak
  Field mode: dev -> prod
  Field port: 8080 -> 443
  Field https: false -> true
  Field apiKey: modified (secret, value not recorded)
```

### 4.2 `prep-tool` Generation Trail (Developer Side)

`write_cfgform` appends to `cfgform-audit.log`:

```
============================================================
[2026-06-20T10:15:30Z] prep-tool generated sidecar metadata
  Target config: config.json
  Target format: json
  Detected tech stack: node
  Model used: deepseek-chat
  Source files read: README.md, src/config.ts
  Sidecar written: config.json.cfgform
  Note: Original file config.json was not modified (original name and form). No keys recorded in this log.
```

---

## 5. Backup Strategy

- Every `commit_save` first performs an `fs::copy` to create **`<target-name>.<UTC timestamp>.bak`** (e.g., `config.json.20260620T101530Z.bak`).
- The backup file **contains real values (including plaintext keys)** — it is a file on your machine, protected by your directory permissions; this is so you can fully roll back. Do not commit `.bak` files containing keys to version control (they are already `.gitignore`d via `*.bak`).
- Write-back uses **atomic replacement**: first write a hidden temp file `.<name>.tmp`, then `rename` to overwrite the target — even a power loss mid-write will not leave a half-written corrupt file.

---

## 6. `.env` Handling

- `prep-tool` may read LLM configuration from a `.env` file in its **install directory** (lowest priority), only for reading `CFGFORM_LLM_*` / `JSONFORM_LLM_*`; it also **writes the key in plaintext to that `.env` only when you explicitly click "Remember key to local .env" (after a safety warning)**.
- That `.env` file is **already `.gitignore`d** to prevent accidentally committing keys to the repository.
- When the target config itself is a `.env` file (`format: env`), `configurator` treats it as a normal config for read/write; fields marked `ui:secret` within it (e.g., `API_KEY`/`DB_PASSWORD`) are masked in the form and their values are not recorded in the audit log.

---
Changelog: 2026-06-21 Doc-accuracy pass — documented prep-tool's newly-added non-secret persistence (`settings.json`, never contains the key), the opt-in "Remember key to local .env" write, and the "Clear saved key" button; updated the "what it writes", key-priority, and `.env` handling sections accordingly.
