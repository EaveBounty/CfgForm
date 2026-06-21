> 🌐 [中文](README.md) ｜ **English**

# Built-in Schema Library (schemas)

This directory collects **curated `.cfgform` templates for common config files**. They are all hand-curated, valid v2.0 sidecars, ready out-of-the-box to let `configurator` render these common files as visual forms with Chinese annotations — **no need to run `prep-tool`, no LLM required**.

## Current Entries

| Template | Applicable Target File | format | Description |
| --- | --- | --- | --- |
| `package.json.cfgform` | `package.json` | `json` | npm package manifest: name/version/scripts/dependencies and other common fields + Chinese hints |
| `tsconfig.json.cfgform` | `tsconfig.json` | `json` | TypeScript compile config: common compilerOptions items + enums & hints |
| `docker-compose.yml.cfgform` | `docker-compose.yml` | `compose` | Generic Compose: services (any service names) / volumes / networks template |

## How to Use (Manual Copy / Customization)

> Note: the three file types `package.json` / `tsconfig.json` / `docker-compose.yml` are already **auto-matched to built-in templates** by `configurator` (see "Auto-Matching" below), so manual copying is usually unnecessary; the steps below apply to **other filenames** or when you want to **ship and customize** a sidecar.

1. Pick a template matching your file, e.g. `package.json.cfgform`.
2. **Copy it to the same directory alongside your target file**, and **rename it to `<your-filename>.cfgform`** (append-style pairing):

   ```
   your-project/
   ├─ package.json
   └─ package.json.cfgform   ← copied from this directory's package.json.cfgform and renamed
   ```

   > The naming rule is "target full filename + `.cfgform`." For example, if the target is `tsconfig.app.json`, the sidecar should be `tsconfig.app.json.cfgform`, and the `"target"` inside the template should be changed to `"tsconfig.app.json"`.

3. Fine-tune as needed: modify `target` (must match the actual filename), add/remove fields, add `"ui:secret": true` to fields holding passwords/tokens, add `"ui:readOnly": true` to fields you don't want users to modify.
4. Open that directory with `configurator` to see the form.

## Design Conventions

- These templates are **moderately permissive** (`additionalProperties: true`, `required` minimized) to accommodate diverse real-world files without false positives.
- `meta.generatedBy` is set to `curated/cfgform-schemas`, `meta.llm.used: false`, indicating hand-curated, no LLM involvement.
- Field types follow the canonical data tree of the corresponding format: `json` preserves native types; `compose` goes through YAML parsing.

## Auto-Matching (Implemented)

> **Auto-matching is now implemented in `configurator`.** Templates for the three common files `package.json`, `tsconfig.json`, and `docker-compose.yml` (including `docker-compose.yaml`/`compose.yml`) are **compiled into** the `configurator` binary at build time (`include_str!`). When such target files exist in a directory but lack an accompanying `.cfgform`/`.jsonform`, `configurator` **automatically applies the built-in template** to render a form (with a top banner "Using built-in schema library"), with no manual copy-and-rename needed; saving writes back to the target file as usual without auto-writing a sidecar, and an optional "Save built-in template as .cfgform in this directory" button is provided.
>
> The `.cfgform` files in this directory are identical to the built-in templates and primarily serve as **copyable, customizable templates**: for filenames beyond the three above (e.g., `tsconfig.app.json`), or when you want to ship and fine-tune a sidecar within your project. In that case, follow the "manual copy" workflow above.

Contributions of additional entries are welcome; see [`../CONTRIBUTING.md`](../CONTRIBUTING.md) §5 for submission guidelines.
