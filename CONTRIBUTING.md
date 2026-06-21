> 🌐 **中文** ｜ [English](CONTRIBUTING.en.md)

# 参与贡献（CONTRIBUTING）

感谢你愿意为 **CfgForm** 贡献！本指南覆盖：开发环境搭建、如何新增一个格式适配器、构建与验证命令、代码风格、如何提交内置 schema 库条目、PR 流程。

> 重要：`.cfgform` 边车的字段语义以 [`spec/cfgform-spec.md`](spec/cfgform-spec.md)（v2.0）为**唯一真相源**。任何改变行为的 PR，**必须先改规范**，再改代码与文档，保持三者一致。

---

## 1. 开发环境搭建

前置依赖：

- **Node.js 18+**
- **Rust（stable，Windows 使用 MSVC 工具链）**
- **WebView2 运行时**（Windows 10/11 通常已预装）

两个应用各自独立：

```pwsh
# 用户侧编辑器
cd configurator
npm install
npm run tauri dev

# 开发者侧生成器
cd prep-tool
npm install
npm run tauri dev
```

若 `cargo` 不在 PATH（本机经验），先注入再做后端检查：

```pwsh
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo check      # 在对应 app 的 src-tauri 目录下
```

---

## 2. 如何新增一个格式适配器

适配器是 CfgForm 唯一与"具体格式"耦合的地方。核心契约只有两个函数（见 `configurator/src-tauri/src/adapters.rs`）：

```rust
pub fn parse(format: &str, text: &str) -> Result<serde_json::Value, String>;
pub fn serialize(format: &str, value: &Value, original: &str) -> Result<String, String>;
```

新增格式 `xxx` 的步骤：

1. **注册格式名**：在 `adapters.rs::normalize_format` 中接受 `"xxx"`（如有别名也在此归一，例如 `yml → yaml`）。
2. **实现 parse**：写 `fn parse_xxx(text: &str) -> Result<Value, String>`，把文本解析成**规范数据树**（`serde_json::Value`）。
   - 注意类型映射：若该格式所有值都是字符串（如 `.env`），数据树中就应是字符串。
   - 空内容应返回空对象 `Value::Object(Map::new())` 而非报错。
3. **实现 serialize（无损优先）**：写 `fn serialize_xxx(value: &Value, original: &str) -> Result<String, String>`。
   - **务必利用 `original` 做最小改写**：逐行/逐节点比对，只重写发生变化处，保留注释、空行、键顺序。
   - 行尾统一 `\n`，UTF-8 无 BOM，确保末尾换行。
4. **接上 match 分支**：在 `adapters.rs::parse` 与 `serialize` 的 `match normalize_format(format)` 中加 `"xxx" => ...`。
5. **同步 prep-tool**：在 `prep-tool/src-tauri/src/lib.rs` 的 `parse_config` 加分支，并在 `detect_format_impl` 加扩展名/内容嗅探规则（prep-tool 只需 parse，无损 serialize 由 configurator 负责）。
6. **如实更新文档**：在 `spec/cfgform-spec.md §6` 保真度表、根 `README.md` 矩阵、`configurator/README.md` 中**诚实标注**该格式的"值往返/注释保留/键顺序"真实情况。**不要夸大**——若注释无法保留就标 ⚠️。
7. **加样例**：在 `sample/` 增加一对真实的 `xxx` + `xxx.cfgform`，并确保 schema 与目标文件字段一致、可被 AJV 校验。

> 现有适配器可作模板：`env`/`ini`（行级保格式）、`toml`（`toml_edit` 文档级保格式）、`json`（`serde_json` `preserve_order`）、`yaml`（外科手术式行级改写，改值保留注释/键序/锚点，仅深层结构性增删才回退 `serde_yaml` 全量序列化）。

---

## 3. 构建与验证命令

提交前请在改动涉及的应用目录执行：

```pwsh
npm run build     # tsc + vite build —— 必须 0 TypeScript 错误

$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo check       # 在 src-tauri 下 —— 必须通过
```

两者均通过方可提 PR。涉及保存逻辑的改动，请手动验证 **Dry-run 预览 → 备份 → 原子写回 → 审计日志**链路正常。

---

## 4. 代码风格

- **Rust**：`cargo fmt` 默认风格；错误用 `Result<_, String>` 返回**中文人话**信息（与现有命令一致）；文件 IO 注意原子写、备份、不静默吞错。
- **TypeScript/React**：遵循现有 tsconfig 严格设置，0 编译错误；组件、命名、调用 `invoke` 的方式参考现有 `src/App.tsx`。
- **面向用户的所有文案/错误/日志使用中文人话**，避免裸露的技术术语吓到非技术用户。
- **安全红线不可破**：不硬编码任何密钥；`ui:secret` 值不得进入日志/边车/预览明文；写文件必须先备份再原子替换。

---

## 5. 如何提交内置 schema 库条目

`schemas/` 收录常见配置文件的精心制作 `.cfgform` 模板（如 `package.json`、`tsconfig.json`、`docker-compose.yml`）。提交一个新条目：

1. 在 `schemas/` 新建 `<常见文件名>.cfgform`，必须是**合法的 v2.0**（含 `$cfgform:"2.0"`、`target`、`format`、`title`、`schema`(draft-07)、`ui`、`meta`）。
2. `schema` 应**适度宽松**（通用模板会面对各种真实文件，建议 `additionalProperties: true`、`required` 最小化），同时为常见字段提供 `description`/`enum`/范围与 `ui:help` 中文提示。
3. `ui` 为常见敏感字段（如含 token/password 的字段）加 `ui:secret`，为不应被改的字段加 `ui:readOnly`。
4. `meta.generatedBy` 标为人工策划（例：`"curated/cfgform-schemas"`），`meta.llm.used: false`。
5. 在 `schemas/README.md` 的清单中登记该条目与用途，并保留"复制到目标文件旁、改名为 `<你的文件名>.cfgform`"的使用说明。

> 注意：自动匹配已对编译期内置的三类模板（`package.json` / `tsconfig.json` / `docker-compose.yml`）在 `configurator` 中实现；`schemas/` 的新条目主要作为**可复制模板**，要让某条目也享受自动匹配，需将其模板内置进 `configurator` 二进制（`include_str!`），因此提交普通条目时无需实现自动匹配逻辑。

---

## 6. PR 流程

1. Fork 并基于 `main` 创建特性分支（如 `feat/adapter-hcl`、`fix/env-quote`、`schemas/add-vite-config`）。
2. 若改变行为：**先更新 `spec/cfgform-spec.md`**，再改代码与所有相关文档。
3. 本地通过 `npm run build` 与 `cargo check`；如涉及保存链路，手动验证一遍。
4. 提交信息用清晰的中文或英文，描述"改了什么、为什么、影响范围"。
5. 在 PR 描述里勾选自检：规范已同步 / 构建通过 / 保真度表如实更新 / 加了样例（如适用）。
6. 一次 PR 聚焦一件事，便于审阅。

期待你的贡献！
