> 🌐 **中文** ｜ [English](cfgform-spec.en.md)

# `.cfgform` 边车元数据规范 v2.1

> **唯一真相源（Single Source of Truth）**。开发者侧 `prep-tool` 按本规范**生成**，用户侧 `configurator` 按本规范**消费**。任何代码改动都必须先改本文件。
>
> v2.0 核心变化：从"仅 JSON"升级为**格式无关的通用配置理解层**。引入**格式适配器**与**格式中立后缀 `.cfgform`**；新增密钥掩码、Dry-run 预览、条件校验、profiles、默认值/重置、只读锁定、内置 schema 库。

---

## 0. 产品定位（为何而做）

| 维度 | 说明 |
| --- | --- |
| **真实痛点** | 配置文件普遍"作者意图没说清"：小白看不懂、老手嫌乱，且 JSON/YAML/.env 极易因括号/缩进/BOM/引号出错。 |
| **产品本质** | 一个**通用配置理解与安全编辑层**。文件格式只是序列化外壳；稀缺价值是那层**语义说明（结构约束 + 人话 + 作者意图）**。 |
| **护城河** | `.cfgform` 边车（schema + ui + meta），格式无关、可复用、可被生态共享。 |
| **两类受众** | 开发者（用 `prep-tool` 一次性生成边车）；最终用户（用 `configurator` 双击安全编辑，全程无 LLM、无感知）。 |

---

## 1. 设计铁律

1. **格式无关核心**：渲染/校验/留痕/备份对所有格式一致；格式差异全部隔离在「格式适配器」内。
2. **不污染、不困扰**：所有元数据合并进**单个 `.cfgform` 文件**（仅本生态识别），普通用户不会误碰。
3. **原文件保持原名原样**：编辑后**写回原文件本身**，绝不改名；写前自动备份。
4. **无损回写优先**：尽最大努力保留原文件的**注释、键顺序、格式**（各格式保真度见 §6）。
5. **全程留痕 + 数据透明**：读了什么 / 写了什么 / 备份到哪 / 哪个字段从 X 变 Y / 调了哪个模型 / 读了哪些源文件，全部写人话审计日志；**密钥永不入日志、不入边车、不入预览明文**。

---

## 2. 文件命名约定（v2.0：改为"追加"配对，消除歧义）

| 角色 | 文件名规则 | 示例 |
| --- | --- | --- |
| 目标配置（用户编辑、目标程序消费） | 原名原样，永不改名 | `config.json` / `.env` / `app.toml` / `docker-compose.yml` |
| **边车元数据**（本规范） | **目标完整文件名 + `.cfgform`** | `config.json.cfgform` / `.env.cfgform` / `docker-compose.yml.cfgform` |
| 备份 | 目标名 + UTC 时间戳 + `.bak` | `config.json.20260620T101530Z.bak` |
| 审计日志 | 同目录固定名 | `cfgform-audit.log` |

> **为何"追加"而非"换后缀"**：换后缀会让 `config.json` 与 `config.yaml` 的边车撞名；追加后缀全局唯一、格式无关，且 `configurator` 只需对 `*.cfgform` 去掉 `.cfgform` 即得目标文件名。
>
> **向后兼容**：`configurator` 仍识别旧式 `xxx.jsonform`（视为 `format=json`），但 `prep-tool` 只产出新式 `.cfgform`。

---

## 3. `.cfgform` 文件结构（始终是 JSON）

```jsonc
{
  "$cfgform": "2.0",                    // 规范版本，必填
  "target": "docker-compose.yml",       // 目标文件名（与边车同目录），必填
  "format": "compose",                  // json|env|toml|yaml|ini|compose，必填（见 §5）
  "title": "服务编排配置",
  "schema": { /* JSON Schema draft-07：结构与硬约束 */ },
  "ui": { /* RJSF uiSchema + 本规范扩展项（见 §4） */ },
  "profiles": {                          // 可选，多环境（见 §4.4）
    "active": "dev",
    "list": ["dev", "staging", "prod"]
  },
  "meta": {
    "generatedBy": "prep-tool/2.0.0",
    "generatedAt": "2026-06-20T10:15:30Z",
    "stackDetected": "node",
    "llm": { "used": true, "model": "deepseek-chat",
             "note": "类型由原文件推断 + 源码/README 经 LLM 精化；约束与帮助文字由 LLM 生成，请作者复核。" },
    "sources": ["README.md", "src/config.ts"]
  }
}
```

> `meta.format` 也镜像在顶层 `format`（顶层为权威）。`schema`/`ui` 永远基于**规范数据树**（适配器解析后的中立 JSON），与目标文件物理格式无关。
>
> **版本号说明**：本文档的**修订版本**为 v2.1（见文末变更日志）；而落盘字段 `$cfgform` 是**格式兼容版本**，当前仍为 `"2.0"`，两者刻意分离——文档修订不必同步抬升边车的兼容版本号，请勿把 `$cfgform` 写成 `"2.1"`。

---

## 4. `ui` 扩展项（在 RJSF uiSchema 基础上新增）

### 4.1 标准 RJSF
`ui:widget`(text/textarea/password/url/email/range/radio/select/checkbox/color/date)、`ui:placeholder`、`ui:help`（人话提示）、`ui:options`、`ui:enumNames`（枚举中文含义）、`ui:order`、`ui:collapsible`/`ui:collapsed`。

### 4.2 `ui:secret`（密钥掩码，安全关键）
字段标 `"ui:secret": true` 时：① 默认以 `••••••` 掩码显示，提供"显示/隐藏"按钮；② **该字段值绝不写入审计日志**（日志只记 `字段 X：已修改（密文，不记录值）`）；③ Dry-run 预览中该值**默认掩码**，可手动临时展开但有醒目警告；④ 备份文件仍含真实值（属本地文件，受目录权限保护）。`prep-tool` 对常见密钥名（key/token/secret/password/dsn/credential…）自动建议 `ui:secret`。

### 4.3 `ui:readOnly`（作者锁定只读）
字段标 `"ui:readOnly": true` 时表单中置灰不可改，并标注"作者锁定"。用于作者不希望用户改动的字段（如内部版本号）。

### 4.4 profiles（多环境）✅ 已实现
顶层 `profiles = { active, list:[...], overrides:{ <名>:{ <点路径>:值 } } }`。`configurator` 顶部提供环境切换；有效数据 = 基础目标数据 ⊕ `overrides[active]`，切换时以基础为底**非累积**叠加。保存：有效值经统一管线写回单一目标文件；另提供"将当前修改保存为【active】覆盖"按钮，把（当前 vs 基础）差异写入边车 `profiles.overrides[active]` 并保存边车（留痕）。

### 4.5 默认值 / 差异 / 重置
`schema` 中的 `default` 用于：① 新建字段填充；② 表单中"与默认值不同"的字段高亮；③ 每字段"重置为默认"按钮。

### 4.6 条件 / 跨字段校验
直接使用 JSON Schema 原生 `if/then/else`、`dependencies`、`allOf`，由 AJV 执行、RJSF 渲染。例：`mode=prod` 时 `https` 必填。`prep-tool` 的 LLM 提示词应在能推断时产出此类条件约束。

---

## 5. 格式适配器契约

每种格式实现一个适配器，**只**负责"文件文本 ⇄ 规范数据树"，其余核心不变：

```
文本(bytes) ──parse──▶ 规范数据树(serde_json::Value) ──▶ schema+ui ──▶ RJSF 表单 ──▶ 编辑
规范数据树 ──serialize(保留注释/顺序/密文/格式)──▶ 文本(bytes) ──原子写入──▶ 目标文件
```

| `format` | 目标文件示例 | 解析库/方式 | 说明 |
| --- | --- | --- | --- |
| `json` | `config.json` | serde_json | 已实现（v1）。 |
| `env` | `.env` | 自研行级解析 | KV、保留注释/顺序、引号/多行、`ui:secret` 常见。 |
| `toml` | `app.toml`,`pyproject.toml` | `toml_edit` | **外科手术式无损**，保留注释/顺序。 |
| `yaml` | `*.yml/*.yaml` | `serde_yaml`(+局部改写) | 保真度见 §6，注释保留为已知难点。 |
| `ini` | `*.ini/*.properties/*.conf` | 自研 section/KV | section + KV，保留注释/顺序。 |
| `compose` | `docker-compose.yml` | yaml 适配器 + **内置 compose schema** | 免 LLM，开箱即用（内置 schema 库）。 |

---

## 6. 各格式无损回写保真度（数据透明，文档必须如实标注）

| format | 值往返 | 注释保留 | 键顺序 | 备注 |
| --- | --- | --- | --- | --- |
| json | ✅ | —（JSON 无注释） | ✅（保留对象键序） | 2 空格缩进、UTF-8 无 BOM、`\n`。 |
| env | ✅ | ✅ | ✅ | 仅改动被编辑行，未触行原样保留。 |
| toml | ✅ | ✅ | ✅ | `toml_edit` 保证。 |
| ini | ✅ | ✅ | ✅ | 同 env 的行级策略。 |
| yaml | ✅ | ✅ 值编辑保留（深层结构增删回退） | ✅ 尽量 | 外科手术式行内改写 + 正确性自检兜底；仅深层键增删/类型变更回退整文档规整化。 |
| compose | 同 yaml | 同 yaml | ✅ 尽量 | 同上。 |

---

## 7. 两侧行为契约

### 7.1 `prep-tool`（开发者侧，生成）
1. 选择目标配置文件 → 由扩展名/内容**判定 format** → 探测技术栈（`meta.stackDetected`）。
2. 用对应适配器 parse → 规范数据树 → 推断结构基线 schema（任意格式通用）。
3. 调 LLM（OpenAI 兼容，读 README/源码）→ 精化类型、生成约束/枚举含义/帮助文字/`ui:secret`/条件约束 → 合成 `schema`+`ui`。
4. **注入并留痕**：写出 `<target>.cfgform`；**不修改原文件**；写 `cfgform-audit.log`（读了哪些文件、用了哪个模型、写出哪个边车）。
5. LLM 凭据优先级：**界面临时输入（仅内存）> 系统环境变量 > 安装目录 `.env`**；密钥绝不落盘/入日志/入边车。

### 7.2 `configurator`（用户侧，消费，无 LLM）
1. 启动扫描所在目录的 `*.cfgform`（兼容旧 `*.jsonform`）。
2. 每个边车：用 `format` 适配器读目标文件 → 规范数据树 → RJSF（schema+ui）渲染 → AJV 实时校验 → 人话中文错误、必填红星、密钥掩码、只读锁定、默认值差异高亮。
3. **保存（两步式）**：先**Dry-run 预览**——序列化为目标格式原文 + 与现文件 diff（密钥默认掩码）→ 用户确认 → 备份原文件 → **原子写入**（temp→rename）防损坏 → 逐字段写审计日志（密钥不记值）。
4. 目录只有目标文件而无 `.cfgform`：友好提示"缺少表单说明文件，请作者用 prep-tool 生成"。

---

## 8. 安全红线
- 写前必备份；原子写入防中断损坏；UTF-8 无 BOM、`\n` 行尾（env/ini/yaml 保留原行尾策略）。
- 密钥：`ui:secret` 字段绝不入日志、不入边车、预览默认掩码；LLM Key 绝不硬编码/落盘/入日志；`.env` 入 `.gitignore`。
- 审计日志只记文件名/字段路径/非密值变更。

---
变更日志：
- 2026-06-20 v2.1：profiles 完整实现（overrides 应用 + 回写边车）；内置 schema 库自动匹配落地（package.json/tsconfig/docker-compose）；YAML/compose 升级为外科手术式回写（保留注释/锚点/顺序，含正确性自检兜底）。
- 2026-06-20 v2.0：升级为格式无关通用配置层；后缀 `.jsonform`→`.cfgform`（追加式配对、向后兼容）；新增 format 字段与格式适配器契约、各格式保真度表、ui:secret/readOnly/profiles/默认值重置/条件校验、Dry-run 两步保存、内置 schema 库。
- 2026-06-20 v1.0：初版（仅 JSON）。
