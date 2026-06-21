> 🌐 **中文** ｜ [English](PRIVACY.en.md)

# 数据透明与隐私（PRIVACY）

> CfgForm 的设计前提是"用户可能不信任软件去改他们的配置"。因此我们把**每一步读写与网络行为都摊开讲清楚**，并用审计日志留痕。本文是关于"程序到底碰了你的什么数据"的完整、诚实声明。

## 一句话总览

- **用户侧 `configurator`：100% 离线，零网络请求，无 LLM。**
- **开发者侧 `prep-tool`：仅一次网络请求**——发往开发者自己配置的 OpenAI 兼容接口，用于生成 `.cfgform` 边车。
- **任何密钥永不写入磁盘、日志或 `.cfgform`。**

---

## 1. `configurator`（用户侧编辑器）

| 项目 | 内容 |
| --- | --- |
| **读取什么** | 所在目录下的 `*.cfgform`（兼容 `*.jsonform`）边车文件；以及它们配对的目标配置文件（`config.json`/`.env`/`app.toml`/`docker-compose.yml`…）。仅读这些，不扫描子目录、不读无关文件。 |
| **写入什么** | ① 编辑后写回**目标配置文件本身**（原名原样，原子写入）；② 写入前生成的**时间戳备份** `<目标名>.<UTC时间戳>.bak`；③ 同目录的**审计日志** `cfgform-audit.log`；④ **仅当你主动点击**"保存为环境覆盖"或"将内置模板另存为 .cfgform"时，才写入/更新同目录的 `.cfgform` 边车（纯本地操作，仍不联网）。 |
| **发送到网络什么** | **无。完全离线，不发起任何网络请求，不含任何 LLM 调用。** |
| **绝不外传什么** | 一切。配置内容、密钥、文件路径都只在本机文件系统内流转。 |

### 用户侧 100% 离线声明

`configurator` 的全部 Rust 命令仅做本地文件 IO：`default_scan_dir` / `scan_dir` / `load_pair` / `preview_save` / `commit_save` / `read_audit_tail`。其中**没有任何一个发起网络请求**，源码不依赖任何 HTTP 客户端。你可以断网运行以自行验证。

---

## 2. `prep-tool`（开发者侧生成器）

| 项目 | 内容 |
| --- | --- |
| **读取什么** | ① 你选择的目标配置文件；② 项目 README（截断至约 6000 字符）；③ 至多 5 个相关源文件（文件名含 `config`/`settings`/`schema`，扩展名 `.ts/.py/.go/.rs/.json`，每个截断至约 3000 字符；自动跳过 `node_modules`/`.git`/`target`/`dist`/`build`/`.venv`/`__pycache__`）；④ 用于探测技术栈的标记文件（`package.json`/`tsconfig.json`/`pyproject.toml`/`requirements.txt`/`go.mod`/`Cargo.toml`）。 |
| **写入什么** | ① `<目标文件名>.cfgform`（追加式配对）；② 追加一段 `cfgform-audit.log` 留痕；③ **仅当你点击"保存接口设置（不含密钥）"时**，把非密钥的 Base URL/Model 写入操作系统应用配置目录的 `settings.json`（**绝不含密钥**，下次启动自动回填以预填界面）；④ **仅当你主动点击"记住密钥到本机 .env"（写前有安全确认）时**，才把密钥明文写入 exe 目录的 `.env`（`.env` 已被 `.gitignore` 忽略）。**绝不修改你的原配置文件。** |
| **发送到网络什么** | **唯一一次** HTTP 请求 → 见下文 §2.1。 |
| **绝不外传什么** | LLM 密钥（仅作为 `Authorization: Bearer` 头随该次请求发送，**不随 payload、不落盘、不入日志/边车**）。除了对你**自己配置**的端点发起的那一次生成请求外，不向任何第三方发送数据。 |

### 2.1 唯一的网络请求

| 维度 | 内容 |
| --- | --- |
| **由谁触发** | 开发者在界面点击"生成"时（`generate_metadata` 命令）。 |
| **目标** | `<你配置的 Base URL>/chat/completions`（OpenAI 兼容；**默认推荐 DeepSeek `https://api.deepseek.com`**，亦可改为 OpenAI、本地模型等任意兼容端点）。 |
| **方法/鉴权** | `POST`，`Authorization: Bearer <你的密钥>`。 |
| **请求体（payload）** | 一个 chat-completions 请求：`model`、`temperature: 0.2`、`response_format: {type: json_object}`，以及 messages——system 提示词 + user 内容。**user 内容包含**：目标文件格式、技术栈、由配置值推断出的基线 JSON Schema、README 摘录、所选源文件摘录。即：**你的配置结构与所选源码/文档会发送给你指定的 LLM**。 |
| **响应处理** | 取 `choices[0].message.content`，防御性解析出 `{schema, ui}`，合成 `.cfgform` 写入磁盘。 |

> 因此：是否联网、发给谁、发什么模型，**完全由开发者用自己的 Key/BaseURL/Model 决定**。用户侧对此全程无感知，也不会触发任何网络行为。

---

## 3. LLM 密钥：三级优先级与"绝不落盘"承诺

`prep-tool` 解析密钥的优先级（见 `resolve_key`）：

1. **界面临时输入**（最高）——仅存于内存，关闭即失，不留痕。
2. **系统环境变量**——`CFGFORM_LLM_API_KEY` / `CFGFORM_LLM_BASE_URL` / `CFGFORM_LLM_MODEL`（向后兼容旧名 `JSONFORM_LLM_*`）。
3. **程序安装目录的 `.env`** 文件（最低）。

> **非密钥项持久化**：Base URL / Model 可通过界面「保存接口设置（不含密钥）」按钮写入操作系统应用配置目录的 `settings.json`，启动时自动回填——`settings.json` **绝不包含密钥**。界面共三个按钮：「保存接口设置（不含密钥）」「记住密钥到本机 .env」（可选；写明文密钥到 exe 目录 `.env`，写前有安全警告）「清除已保存密钥」。

承诺：

- 密钥**绝不写入磁盘**（除非是你自己放进 exe 目录 `.env` 的那份，受你目录权限保护）。
- 密钥**绝不写入审计日志**，**绝不写入 `.cfgform`**，**绝不放进发给 LLM 的 payload 正文**（只作为 HTTP `Authorization` 头）。
- `.env` 已列入 `.gitignore`，避免误提交。

---

## 4. 审计日志格式样例

### 4.1 `configurator` 保存留痕（用户侧）

`commit_save` 向 `cfgform-audit.log` 追加（密钥字段只记"已修改"，不记值）：

```
==== 2026-06-20T10:15:30Z 保存配置 ====
目标文件：config.json（格式：json）
备份文件：config.json.20260620T101530Z.bak
  字段 mode：dev -> prod
  字段 port：8080 -> 443
  字段 https：false -> true
  字段 apiKey：已修改（密文，不记录值）
```

### 4.2 `prep-tool` 生成留痕（开发者侧）

`write_cfgform` 向 `cfgform-audit.log` 追加：

```
============================================================
[2026-06-20T10:15:30Z] prep-tool 生成边车元数据
  目标配置：config.json
  目标格式：json
  探测技术栈：node
  使用模型：deepseek-chat
  读取源文件：README.md, src/config.ts
  写出边车：config.json.cfgform
  说明：未修改原文件 config.json（原名原样）。本日志不记录任何密钥。
```

---

## 5. 备份策略

- 每次 `commit_save` 在写回前先 `fs::copy` 出一份 **`<目标名>.<UTC时间戳>.bak`**（例：`config.json.20260620T101530Z.bak`）。
- 备份文件**包含真实值（含密钥明文）**——它是你本机的文件，受你的目录权限保护；这是为了让你能完整回滚。请勿将含密钥的 `.bak` 提交到版本库（已在 `.gitignore` 忽略 `*.bak`）。
- 写入采用**原子替换**：先写隐藏临时文件 `.<name>.tmp`，再 `rename` 覆盖目标——即使中途断电也不会留下"写一半"的损坏文件。

---

## 6. `.env` 处理

- `prep-tool` 可从其**安装目录**的 `.env` 读取 LLM 配置（最低优先级），仅用于读取 `CFGFORM_LLM_*` / `JSONFORM_LLM_*`；也**仅在你主动点击"记住密钥到本机 .env"（写前有安全警告）时**，才把密钥明文写入该 `.env`。
- 该 `.env` 文件**已在 `.gitignore` 中忽略**，防止把密钥误提交到仓库。
- 当目标配置本身就是 `.env` 文件时（`format: env`），`configurator` 会把它当作普通配置读写；其中被标 `ui:secret` 的字段（如 `API_KEY`/`DB_PASSWORD`）在表单中掩码、在审计中不记值。

---
变更日志：2026-06-21 文档准确性校订——补充 prep-tool 新增的非密钥项持久化（`settings.json`，绝不含密钥）、可选的「记住密钥到本机 .env」写入与「清除已保存密钥」按钮；据此更新"写入什么"、密钥优先级与 `.env` 处理小节。
