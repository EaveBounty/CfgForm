> 🌐 [中文](README.md) ｜ **English**

# prep-tool (Developer Side · v2.0.0)

Generates `.cfgform` sidecar metadata (schema + ui + meta) for multi-format config files. **Only generates sidecars; never modifies original files.**
The user side is consumed by `configurator` for safe editing. Spec at `../spec/cfgform-spec.md` (single source of truth).

## Supported Formats (PARSE → Canonical Data Tree)

| format    | Trigger                                | Parser               |
| --------- | -------------------------------------- | -------------------- |
| `json`    | `*.json`                               | serde_json           |
| `env`     | `.env` / `*.env`                       | Custom line-level KV |
| `toml`    | `*.toml`                               | toml_edit            |
| `yaml`    | `*.yml` / `*.yaml`                     | serde_yaml           |
| `ini`     | `*.ini` / `*.properties` / `*.conf`    | Custom section/KV    |
| `compose` | `docker-compose.*` / `compose.*`       | YAML adapter         |

`detect_format` first infers from filename/extension, falls back to content sniffing, and if still uncertain defaults to `json`.

## Four-Step Wizard

1. Select project → detect format (`detect_format`) + tech stack (`detect_stack`) → parse to canonical data tree → infer baseline Schema.
2. Configure LLM (OpenAI compatible). **Default endpoint = DeepSeek `https://api.deepseek.com` (recommended)**, default model `deepseek-chat` (if a stronger DeepSeek `pro`/`V`-series model is available, enter its id; editable). Credential priority: **UI temporary input (memory only) > env vars `CFGFORM_LLM_*` (compatible with legacy `JSONFORM_LLM_*`) > program-dir `.env`**; the key is held only in memory by default. See "LLM Config Persistence" below.
3. Generate: baseline Schema + README/source refined by LLM (constraints, `if/then/else` conditional validation, Chinese help text, `ui:enumNames`,
   `ui:secret`, `ui:readOnly`), merged with heuristic secret suggestions (`suggest_secrets`). Real-time RJSF preview, secret fields masked by default.
4. Write: output `<target-filename>.cfgform` in the project directory (**append-style pairing**), append to `cfgform-audit.log` with Chinese audit trail.

## LLM Config Persistence

- **Endpoint settings (non-secret) persistence**: Base URL and Model are saved to `settings.json` under the OS **app-config directory** (`app_config_dir()`), and are **auto-loaded to prefill** the UI on next launch; the **API key is never saved**.
- **API key resolution priority**: UI temporary input (memory only) > env vars `CFGFORM_LLM_API_KEY` / `CFGFORM_LLM_BASE_URL` / `CFGFORM_LLM_MODEL` (compatible with legacy `JSONFORM_LLM_*`) > program (exe) dir `.env`.
- UI buttons:
  - **"保存接口设置（不含密钥）"** (Save endpoint settings, excluding key): writes the current Base URL/Model to `settings.json` (no key).
  - **"记住密钥到本机 .env"** (Remember key to local .env): opt-in; after a warning, writes the key (along with Base URL/Model) in **plaintext** to the exe-dir `.env` for long-term convenience.
  - **"清除已保存密钥"** (Clear saved key): removes the saved key line from the exe-dir `.env` (keeping Base URL/Model).
- `.env` is already in `.gitignore` and will not be committed.

## Security Red Lines

- Original file: original name, original form, never modified.
- LLM key never written to disk / logged / stored in `.cfgform`; `.env` already in `.gitignore`.
- Env var names: priority `CFGFORM_LLM_{API_KEY,BASE_URL,MODEL}`, compatible with legacy `JSONFORM_LLM_*`.

## Development

```sh
npm install
npm run build         # tsc + vite, 0 TS errors
npm run tauri dev     # launch desktop app
```

Rust backend check (src-tauri): `cargo check`.

---

Changelog:
- 2026-06-20 v2.0.0: Upgraded from JSON-only to multi-format (json/env/toml/yaml/ini/compose); suffix `.jsonform`→`.cfgform` (append-style pairing);
  added format adapters, `detect_format`, `suggest_secrets` secret heuristics, `ui:secret`/`ui:readOnly`/conditional validation in prompts, secret masking preview;
  command `write_jsonform`→`write_cfgform`, audit log `jsonform-audit.log`→`cfgform-audit.log`.
