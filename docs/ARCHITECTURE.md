> 🌐 **中文** ｜ [English](ARCHITECTURE.en.md)

# 架构与原理（ARCHITECTURE）

> 配套阅读：规范唯一真相源 [`../spec/cfgform-spec.md`](../spec/cfgform-spec.md)。本文解释"为什么这样设计"，规范定义"必须怎么做"。

## 1. 设计哲学

CfgForm 建立在两条核心信念之上：

### 1.1 格式无关核心 + 语义边车（护城河）

配置文件的**物理格式（JSON/YAML/.env…）只是序列化外壳**。真正稀缺、可复用、值得长期沉淀的，是那层**语义说明**：

- **结构约束**：字段类型、必填、范围、枚举、正则、字段间条件。
- **人话提示**：这个字段填什么、为什么、踩过什么坑。
- **作者意图**：哪些字段不该被用户改（只读锁定）、哪些是敏感密钥（掩码）。

这些知识不在文件语法里，也无法可靠地从源码"自动推断"——它们存在于作者脑子里、README 里、Issue 回复里。CfgForm 把它们一次性封装进 **`.cfgform` 边车**（schema + ui + meta），格式无关、可被生态共享。这就是护城河：别人复制不走的，是这层理解。

### 1.2 不污染、不困扰；原文件神圣不可侵犯

- 所有元数据合并进**单个 `.cfgform` 文件**，只被本生态识别，普通用户不会误碰。
- 目标配置文件**永远原名原样**：编辑后写回它自身，绝不改名；写前自动备份；尽力保留注释/键顺序/格式。
- 全程留痕：读了什么、写了什么、备份到哪、哪个字段从 X 变 Y——全部写人话审计日志。

---

## 2. 整体架构

CfgForm 由两个独立但共享同一份规范与同一套适配器契约的桌面应用组成：

```
                 ┌──────────────────────────────────────────────┐
                 │            spec/cfgform-spec.md (v2.1)         │
                 │                 唯一真相源                      │
                 └───────────────┬───────────────┬───────────────┘
                                 │ 按规范生成      │ 按规范消费
                                 ▼                ▼
   开发者侧                                                   用户侧
┌────────────────────────────────┐         ┌──────────────────────────────────────┐
│ prep-tool (含 LLM, 联网一次)    │         │ configurator (无 LLM, 100% 离线)        │
│                                │         │                                        │
│ detect_format / detect_stack   │         │ scan_dir / load_pair                   │
│ adapters::parse → 数据树        │         │ adapters::parse → 数据树                │
│ infer_schema (基线)            │  .cfgform│ RJSF(schema+ui) + AJV 实时校验          │
│ generate_metadata (LLM 精化)   │ ───────▶ │ preview_save (Dry-run + diff)          │
│ write_cfgform (只写边车)        │  边车    │ commit_save (备份→原子写→审计)          │
└────────────────────────────────┘         │ adapters::serialize → 写回原文件        │
        │ 绝不修改原文件                      └──────────────────────────────────────┘
        ▼                                                  │
   <target>.cfgform                                        ▼
                                              <target> + <target>.<ts>.bak + cfgform-audit.log
```

两侧均为 **Tauri v2 + React 19 + TypeScript + Vite**，前端用 **RJSF（react-jsonschema-form）** 渲染表单、**AJV 8** 做校验，后端用 **Rust** 负责文件 IO 与格式适配。

---

## 3. 规范数据树（Canonical Tree）

这是整个系统的中枢概念。**任何格式的配置文件，第一步都被对应适配器解析成一棵中立的 JSON 值树（Rust 中为 `serde_json::Value`）。** 之后所有逻辑——schema 校验、RJSF 渲染、字段 diff、密钥识别——都只面对这棵树，对原始格式一无所知。

| 阶段 | 数据形态 |
| --- | --- |
| 输入 | 原文件字节流（含注释、缩进、引号、BOM 等格式细节） |
| parse 后 | **规范数据树**（纯结构化值：对象/数组/字符串/数字/布尔/null） |
| 编辑中 | 规范数据树（被表单修改） |
| serialize 后 | 原格式字节流（适配器尽力把格式细节贴回去） |

几个关键后果（务必理解，否则 schema 会写错）：

- **`.env` / `.ini` 的值在数据树里全是字符串**。即便配置里写 `PORT=8080`，解析后是字符串 `"8080"`，因此对应 schema 字段类型应为 `string`（可用 `pattern` 约束为数字形态），**不要写 `integer`**。
- **`toml` / `yaml` / `json` 保留原生类型**（整数、浮点、布尔、数组、嵌套对象）。
- **`compose` 复用 yaml 适配器**，其数据树就是 YAML 解析结果。

`schema` 与 `ui` 永远针对这棵规范数据树编写，与目标文件的物理格式解耦。

---

## 4. 格式适配器扩展点（parse / serialize 契约）

每种格式只需实现一个适配器，**只负责"文件文本 ⇄ 规范数据树"**，核心其余部分零改动。契约（见 `configurator/src-tauri/src/adapters.rs`）：

```rust
// 文本 → 规范数据树（prep-tool 与 configurator 都需要）
pub fn parse(format: &str, text: &str) -> Result<serde_json::Value, String>;

// 规范数据树 + 原文 → 文本（仅 configurator 回写需要；original 用于无损保格式）
pub fn serialize(format: &str, value: &Value, original: &str) -> Result<String, String>;
```

设计要点：

- **`serialize` 接收 `original` 原文**，这是无损回写的关键：适配器对比新旧数据树，**只重写发生变化的部分**，未触及的注释/空行/顺序原样保留。
  - `env`/`ini`：行级策略，逐行比对，仅改值行被重写，新键追加。
  - `toml`：基于 `toml_edit` 的 `DocumentMut`，在原文档对象上做最小增删改，注释与顺序天然保留。
  - `json`：`serde_json` 启用 `preserve_order`，2 空格、UTF-8 无 BOM、`\n`。
  - `yaml`/`compose`：**外科手术式行内改写**——仅重写发生变化的标量行，保留注释/锚点/顺序；仅深层嵌套键增删或类型变更才回退整文档规整化（带正确性自检兜底，数据永不出错）。
- 各格式的真实保真度**必须如实写入文档**（见根 README 与规范 §6），这是"数据透明"的承诺。

> **新增一个格式**只需：在 `normalize_format` 注册名字 → 实现 `parse_xxx` / `serialize_xxx` → 在 `parse`/`serialize` 的 match 分支接上 → 在 `prep-tool` 的 `detect_format_impl` 加扩展名映射 → 更新规范保真度表。详细步骤见 [`../CONTRIBUTING.md`](../CONTRIBUTING.md)。

---

## 5. 两-App 模型与数据流时序

### 5.1 prep-tool（开发者侧，生成边车）

```
1. 选目标文件 → detect_format（扩展名/内容嗅探）→ detect_stack（node/python/go/rust/generic；**仅用于上下文/标注，不做源码类型系统抽取**）
2. adapters::parse → 规范数据树 → infer_schema 推断基线 Schema（**类型基线取自配置的"值"，而非源码 AST / TS interface / Pydantic / Go|Rust struct**；任意格式通用）
3. gather_sources 读 README(≤6000字) + 至多 5 个相关源文件(config/settings/schema 关键词)
4. generate_metadata：把"格式 + 技术栈 + 基线 schema + 源码上下文"发给 OpenAI 兼容 LLM，
   返回精化后的 schema(补 description/enum/min/max/pattern/if-then) + ui(ui:help/enumNames/secret/readOnly)
5. suggest_secrets 启发式（key/token/secret/password/dsn/credential/private…）强制补 ui:secret，
   即便 LLM 漏标也兜底
6. write_cfgform：写出 <target>.cfgform（追加式配对）+ 追加 cfgform-audit.log；原文件零改动
```

### 5.2 configurator（用户侧，消费边车，两步保存）

```
1. default_scan_dir / scan_dir：扫描目录所有 *.cfgform（兼容 *.jsonform），按 target 与目标文件配对
   （孤儿目标文件——有 .json/.env/.toml… 但无边车——也列出并提示"请作者用 prep-tool 生成"）
2. load_pair：读边车 → 用 format 适配器 parse 目标文件 → 返回 {schema, ui, data, profiles}
3. RJSF(schema+ui) 渲染 + AJV 实时校验：必填红星、中文错误、密钥掩码、只读置灰、默认值差异高亮
4. preview_save（Dry-run）：serialize 出"将写入的原文" + 与现文件做 LCS 行级 diff（密钥默认掩码）
5. 用户确认 → commit_save：
     备份 <target>.<UTC时间戳>.bak
     → 写临时文件 .<name>.tmp → rename 替换（原子，防写一半损坏）
     → 结构化 diff 逐字段写 cfgform-audit.log（ui:secret 字段只记"已修改（密文，不记录值）"）
```

---

## 6. 技术选型理由

### 6.1 为何 Tauri 而非 Electron

| 维度 | Tauri v2 | Electron |
| --- | --- | --- |
| 产物大小 | **约 5–15MB** | 80–150MB（自带 Chromium） |
| 启动速度 | **秒开** | 较慢 |
| 渲染层 | 复用系统 WebView2/WKWebView | 内置 Chromium |
| 后端 | Rust（安全文件 IO、强类型适配器） | Node.js |
| 安全沙箱 | 能力（capabilities）白名单 | 需自行加固 |

对一个"放在配置目录、双击就开、给非技术用户用"的工具，**体积小、秒开、离线、安全**是硬指标。Tauri 把这些一次满足，Rust 后端也让格式适配器的解析/无损回写更可靠。

### 6.2 为何 RJSF + AJV

- **RJSF（react-jsonschema-form）** 是成熟的"JSON Schema → 表单"渲染库，原生支持 `uiSchema` 自定义控件（radio/range/password/select…），正好承载 `.cfgform` 的 `ui` 扩展。
- **AJV 8** 是事实标准的 JSON Schema 校验器，**原生支持 draft-07 的 `if/then/else`、`dependencies`、`allOf`**，让"`mode=prod` 时 `https` 必填"这类跨字段约束零成本落地。
- 二者组合让"结构 + 校验 + 展示"全部由声明式的 `.cfgform` 驱动，核心代码无需为每个配置写定制 UI。

---

## 7. `.cfgform` 契约要点

一个 `.cfgform` 始终是合法 JSON，关键字段（完整定义见规范 §3、§4）：

```jsonc
{
  "$cfgform": "2.0",            // 规范版本，必填
  "target": "config.json",      // 同目录目标文件名，必填
  "format": "json",             // json|env|toml|yaml|ini|compose，必填（顶层为权威）
  "title": "应用主配置",
  "schema": { /* JSON Schema draft-07：结构与硬约束 */ },
  "ui": { /* RJSF uiSchema + 扩展：ui:help/ui:secret/ui:readOnly/ui:enumNames… */ },
  "profiles": { "active": "dev", "list": ["dev","prod"], "overrides": {} },  // 可选：多环境覆盖
  "meta": {
    "generatedBy": "prep-tool/2.0.0",
    "generatedAt": "2026-06-20T10:15:30Z",
    "stackDetected": "node",
    "llm": { "used": true, "model": "...", "note": "约束/帮助由 LLM 生成，请作者复核。" },
    "sources": ["README.md", "src/config.ts"]
  }
}
```

`ui` 的安全/体验扩展项：`ui:secret`（密钥掩码）、`ui:readOnly`（作者锁定）、`ui:help`（中文人话提示）、`ui:enumNames`（枚举中文含义）、`ui:order`、`ui:widget`、`ui:placeholder`。

---

## 8. 安全模型

| 机制 | 实现 | 目的 |
| --- | --- | --- |
| 写前备份 | `<target>.<UTC时间戳>.bak`（`fs::copy`） | 误改可回滚 |
| 原子写入 | 写 `.<name>.tmp` → `fs::rename` 替换 | 防写一半导致文件损坏 |
| Dry-run 两步保存 | `preview_save` 先出原文+diff，确认后 `commit_save` 才落盘 | 杜绝"盲改"，所见即所写 |
| 密钥三级优先 | 界面临时输入(仅内存) > 环境变量 `CFGFORM_LLM_*`(兼容 `JSONFORM_LLM_*`) > exe 目录 `.env` | 灵活且不强制落盘 |
| 密钥零外泄 | 密钥绝不写磁盘/日志/`.cfgform`；`ui:secret` 字段值不入审计、预览默认掩码；`.env` 入 `.gitignore` | 防泄露 |
| 全程审计 | `cfgform-audit.log` 人话记录文件名/字段路径/非密值变更 | 可追溯、建立信任 |
| 编码规范 | UTF-8 无 BOM、`\n` 行尾（env/ini/yaml 保留原行尾策略） | 跨平台一致、防编码错误 |

> 逐 App 的"读/写/发送/绝不外传"清单与样例见 [`PRIVACY.md`](PRIVACY.md)。

---
变更日志：2026-06-21 文档准确性校订——架构图规范标注改为 v2.1（边车 `$cfgform` 字段值仍为 `"2.0"`，刻意区分）；明确 infer_schema 的类型基线取自配置的"值"而非源码类型系统/AST，detect_stack 仅用于上下文标注。
