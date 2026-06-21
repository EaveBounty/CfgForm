> 🌐 **中文** ｜ [English](README.en.md)

# 通用配置器（configurator，用户侧）

可视化、安全编辑多种格式配置文件的桌面应用（Tauri v2 + React 19 + TypeScript + Vite），**不含 LLM**，面向非技术用户。遵循 `../spec/cfgform-spec.md` v2.0（格式无关通用配置层）。

## 功能

- 启动时扫描所在目录的 `*.cfgform` 边车文件（兼容旧式 `*.jsonform`，按 `format=json` 处理），按 `target` 与目标文件自动配对。
- 顶层 `format` 字段决定格式适配器：`json` / `env` / `toml` / `ini` / `yaml` / `compose`。侧栏与表单头显示格式徽标。
- 用 RJSF（`schema` + `ui`）渲染可编辑表单，AJV8 实时校验（原生支持 `if/then/else`、`dependencies` 条件/跨字段约束），必填红星，中文错误汇总。
- **密钥掩码**：`ui:secret:true` 字段以密码框显示，带"显示/隐藏"开关；其值不入审计日志（仅记"已修改（密文，不记录值）"），预览默认掩码。
- **只读锁定**：`ui:readOnly:true` 字段置灰禁用并标注"🔒 作者锁定"。
- **默认值差异 / 重置**：列出与 `schema.default` 不同的字段，提供逐字段"重置为默认"。
- **多环境 profiles**：边车含 `profiles { active, list, overrides }` 时顶部显示环境切换；加载时「有效数据 = 基线目标数据 ⊕ overrides[active]」，切换环境即在基线之上重新套用该环境覆盖并重渲染。提供「将当前修改保存为【active】环境的覆盖」按钮：把（当前表单 vs 基线目标）的差异写回边车 `profiles.overrides[active]`（美化 JSON、无 BOM、`\n`），并写审计行「更新边车 profiles.overrides[active]」，不破坏边车其它字段。`保存…` 仍把有效值写回单一目标文件（备份 + 原子写入 + 审计）。
- **内置 schema 库（开箱即用）**：编译期内置 `package.json` / `tsconfig.json` / `docker-compose.yml`（含 `docker-compose.yaml`、`compose.yml`）精选模板。目录中若有这些目标文件但「无自带 `.cfgform`/`.jsonform`」，自动以内置模板配对（来源标记"内置库"），读取真实目标文件现值即可渲染编辑；表单顶部显示横幅"使用内置 schema 库（项目未随附 .cfgform）"，保存照常写回目标文件，且**不自动写边车**。另提供可选按钮"将内置模板另存为本目录的 .cfgform"。仅当既无边车又不命中内置库时，才提示"缺少表单说明文件"。
- **两步式保存（Dry-run）**：先 `preview_save` 生成"将写入的文件原文 + 行级 diff"（密钥默认掩码，可警示性展开）→ 确认后 `commit_save` 备份 `<目标名>.<UTC时间戳>.bak` → 原子写入（临时文件→重命名）→ 逐字段中文审计写入 `cfgform-audit.log`。

## 各格式无损回写保真度（如实标注）

| format | 读取 load | 回写 save | 注释/顺序 |
| --- | --- | --- | --- |
| json | ✅ | ✅ | 无注释；保留键序（启用 serde_json `preserve_order`），2 空格、UTF-8 无 BOM、`\n` |
| env | ✅ | ✅ | 行级保留：仅改动行重写，注释/空行/顺序原样，新键追加末尾 |
| toml | ✅ | ✅ | `toml_edit` 外科手术式，保留注释与顺序 |
| ini | ✅ | ✅ | 行级保留；改值原样回写；新键追加（新根键置顶、新段键末尾） |
| yaml | ✅ | ✅（改值保留注释） | 外科手术式行级改写：注释/键序/锚点全部保留；仅**嵌套键的结构性增删**会触发整文档回退（⚠️，写前看 Dry-run） |
| compose | ✅ | ✅（改值保留注释） | 复用 yaml 适配器，同上 |

> YAML 序列化采用「外科手术 + 安全网」：仅就地改写值确实变化的那一行（保留缩进/键名/行尾内联注释），未触行原样输出；改写结果会被重新解析校验，若与目标语义不一致（复杂结构性增删/类型互换等）自动回退为 `serde_yaml` 全量序列化（数据正确，该次丢注释）。

格式适配器实现见 `src-tauri/src/adapters.rs`。

## Rust 命令（src-tauri/src/lib.rs）

- `default_scan_dir() -> String`
- `scan_dir(dir: String) -> Vec<PairInfo>`（`PairInfo` 新增 `builtin: bool` 与 `source: String`；内置库配对 `cfgform_path` 为空、`builtin=true`、`source="内置库"`）
- `load_pair(cfgform_path: String) -> LoadResult`
- `load_builtin(target_path: String) -> LoadResult`（用内置模板渲染，读取真实目标文件现值；`profiles` 恒为 null）
- `preview_save(target_path, format, data) -> PreviewResult`
- `commit_save(target_path, format, data, secret_paths) -> SaveResult`
- `read_audit_tail(dir, max_lines) -> String`
- `save_profile_overrides(cfgform_path: String, active: String, overrides_value: Value) -> ()`（写回边车 `profiles.overrides[active]`，原子写入 + 审计，非破坏其它字段）
- `save_builtin_sidecar(target_path: String) -> String`（可选：把内置模板另存为目标旁的 `.cfgform`，返回写出路径；已存在则报错不覆盖）

## 开发与验证

```pwsh
npm install
npm run build          # tsc && vite build
# cargo（cargo 未在 PATH，需先注入）
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"; cargo check   # 在 src-tauri 下
```

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

---
变更日志：
- 2026-06-21 完成路线图三项能力：① YAML/compose 外科手术式序列化（改值保留注释/键序/锚点，结构性增删才回退，保真度由 ⚠️ 升为 ✅）；② profiles 多环境完整实现（基线⊕overrides[active] 有效数据、环境切换、`save_profile_overrides` 写回边车覆盖、去除"实验性"）；③ 内置 schema 库自动配对（`load_builtin`、内置库横幅、可选另存边车 `save_builtin_sidecar`）；`npm run build` 0 错误、`cargo check` 通过。
- 2026-06-20 重构为 v2.0 多格式架构：新增 `adapters.rs`（json/env/toml/ini/yaml/compose 适配器）、`preview_save`/`commit_save` 两步保存、`.cfgform` 边车（兼容 `.jsonform`）、密钥掩码/只读锁定/默认值重置/条件校验/profiles/Dry-run 预览；`npm run build` 0 错误、`cargo check` 通过。
- 2026-06-20 实现用户侧配置器：Rust 文件 IO 五命令（扫描/加载/保存/审计）、备份+原子写入+审计留痕、RJSF+AJV 中文表单 UI；`npm run build` 与 `cargo check` 均通过。
