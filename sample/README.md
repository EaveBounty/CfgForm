> 🌐 **中文** ｜ [English](README.en.md)

# 可运行示例（sample）

本目录包含 **4 种格式的真实成对样例**，每对都是「目标配置文件 + 合法的 `.cfgform` v2.0 边车」。用 `configurator` 打开本目录即可立即看到表单、实时校验、密钥掩码、只读锁定与条件校验等全部特性。

## 目录内容

| 目标文件 | 边车文件 | format | 演示重点 |
| --- | --- | --- | --- |
| `config.json` | `config.json.cfgform` | `json` | 枚举 / 数值范围 / 必填 / 默认值 / `ui:secret`（apiKey）/ **条件校验：`mode=prod` 时 `https` 必填且须为 true** |
| `.env` | `.env.cfgform` | `env` | 全字符串值 / `ui:secret` 掩码（`API_KEY`、`DB_PASSWORD`）/ 枚举单选 / 行级保留 |
| `app.toml` | `app.toml.cfgform` | `toml` | 嵌套表 / `ui:readOnly` 作者锁定（`version`）/ `ui:secret`（`database.url`）/ 注释与顺序无损保留 |
| `docker-compose.yml` | `docker-compose.yml.cfgform` | `compose` | 嵌套服务 / 端口与卷数组 / `ui:secret`（`db` 的 `POSTGRES_PASSWORD`）/ YAML 改值外科手术式保留注释/键序/锚点（仅深层结构性增删才回退整文档规整化） |

## 如何用 configurator 打开本目录验证

`configurator` 在**开发模式**下默认扫描"当前工作目录"，在**生产构建**下扫描"可执行文件所在目录"。任选其一：

### 方式 A：开发模式指向本目录（最快）

在 `configurator` 目录启动开发模式后，它会扫描其启动时的工作目录。最简单的做法是先把工作目录切到 `sample/`，再启动：

```pwsh
# 从仓库根目录
cd sample
npm --prefix ..\configurator install
npm --prefix ..\configurator run tauri dev
```

> 说明：`default_scan_dir` 在 debug 构建下返回 `current_dir()`。若你的启动脚本固定了工作目录，可在应用内的"目录"输入处手动改为本 `sample/` 的绝对路径后重新扫描。

### 方式 B：把构建好的程序放进本目录（贴近真实用户场景）

```pwsh
# 先构建用户侧编辑器
npm --prefix ..\configurator run tauri build
# 将产物（src-tauri/target/release/bundle/ 下的可执行文件）复制到本 sample/ 目录后双击运行
```

程序启动后会扫描同目录的 4 个 `.cfgform`，并把每个渲染成一个表单。

## 你应当看到的效果

- 4 个配置以**可视化表单**呈现，必填项带红星，字段下方有中文 `ui:help` 提示。
- `config.json` 把 `mode` 改为 **生产（prod）** 后，`https` 立即变为必填并要求为开启状态（条件校验生效）。
- `apiKey`、`.env` 的 `API_KEY`/`DB_PASSWORD`、`app.toml` 的 `database.url`、compose 的 `POSTGRES_PASSWORD` 均以 **`••••••` 掩码**显示，可点击显示/隐藏。
- `app.toml` 的 `version` 字段**置灰只读**并标注"作者锁定"。
- 点击保存会先进入 **Dry-run 预览**（展示将写入的原文 + 行级 diff，密钥默认掩码），确认后才会备份 + 原子写回，并在本目录生成 `cfgform-audit.log` 与 `*.bak`。

> 这些 `.bak` 与 `cfgform-audit.log` 是运行产物，已在仓库根 `.gitignore` 中忽略。
