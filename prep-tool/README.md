> 🌐 **中文** ｜ [English](README.en.md)

# prep-tool（开发者侧 · v2.0.0）

为多格式配置文件生成 `.cfgform` 边车元数据（schema + ui + meta）。**只生成边车，绝不修改原文件。**
用户侧由 `configurator` 消费边车进行安全编辑。规范见 `../spec/cfgform-spec.md`（唯一真相源）。

## 支持的格式（PARSE → 规范数据树）

| format    | 触发                                   | 解析方式            |
| --------- | -------------------------------------- | ------------------- |
| `json`    | `*.json`                               | serde_json          |
| `env`     | `.env` / `*.env`                       | 自研行级 KV         |
| `toml`    | `*.toml`                               | toml_edit           |
| `yaml`    | `*.yml` / `*.yaml`                     | serde_yaml          |
| `ini`     | `*.ini` / `*.properties` / `*.conf`    | 自研 section/KV     |
| `compose` | `docker-compose.*` / `compose.*`       | yaml 适配器         |

`detect_format` 先按文件名/扩展名判定，失败时嗅探内容，仍不确定回退 `json`。

## 四步向导

1. 选择项目 → 探测格式（`detect_format`）+ 技术栈（`detect_stack`）→ 解析为规范数据树 → 推断基线 Schema。
2. 配置 LLM（OpenAI 兼容）。**默认接口 = DeepSeek `https://api.deepseek.com`（推荐）**，默认模型 `deepseek-chat`（若有更强的 DeepSeek `pro`/`V` 系列模型，建议填其 id；可编辑）。凭据优先级：**界面临时输入（仅内存）> 环境变量 `CFGFORM_LLM_*`（兼容旧 `JSONFORM_LLM_*`）> 程序目录 `.env`**；密钥默认仅存内存。详见下方「LLM 配置持久化」。
3. 生成：基线 Schema + README/源码经 LLM 精化（约束、`if/then/else` 条件校验、中文帮助、`ui:enumNames`、
   `ui:secret`、`ui:readOnly`），并合并启发式密钥建议（`suggest_secrets`）。实时 RJSF 预览，密钥字段默认掩码。
4. 写入：在项目目录写出 `<目标文件名>.cfgform`（**追加式配对**），追加 `cfgform-audit.log` 中文留痕。

## LLM 配置持久化

- **接口设置（非密钥）持久化**：Base URL 与 Model 保存到操作系统**应用配置目录**下的 `settings.json`（`app_config_dir()`），下次启动**自动回填**预填到界面；**绝不保存 API Key**。
- **API Key 解析优先级**：界面临时输入（仅内存）> 环境变量 `CFGFORM_LLM_API_KEY` / `CFGFORM_LLM_BASE_URL` / `CFGFORM_LLM_MODEL`（兼容旧 `JSONFORM_LLM_*`）> 程序（exe）目录 `.env`。
- 界面按钮：
  - **「保存接口设置（不含密钥）」**：把当前 Base URL/Model 写入 `settings.json`（不含密钥）。
  - **「记住密钥到本机 .env」**：可选项；经警示后把密钥（连同 Base URL/Model）以**明文**写入 exe 目录 `.env`，便于长期免重输。
  - **「清除已保存密钥」**：从 exe 目录 `.env` 移除已保存的密钥行（保留 Base URL/Model）。
- `.env` 已在 `.gitignore` 中，不会被提交。

## 安全红线

- 原文件原名原样，绝不修改。
- LLM 密钥绝不落盘 / 入日志 / 入 `.cfgform`；`.env` 已在 `.gitignore`。
- 环境变量名：优先 `CFGFORM_LLM_{API_KEY,BASE_URL,MODEL}`，兼容旧 `JSONFORM_LLM_*`。

## 开发

```sh
npm install
npm run build         # tsc + vite，0 TS 错误
npm run tauri dev     # 启动桌面应用
```

Rust 后端校验（src-tauri）：`cargo check`。

---

变更日志：
- 2026-06-20 v2.0.0：从仅 JSON 升级为多格式（json/env/toml/yaml/ini/compose）；后缀 `.jsonform`→`.cfgform`（追加式配对）；
  新增格式适配器、`detect_format`、`suggest_secrets` 密钥启发式、`ui:secret`/`ui:readOnly`/条件校验提示词、密钥掩码预览；
  命令 `write_jsonform`→`write_cfgform`，审计日志 `jsonform-audit.log`→`cfgform-audit.log`。
