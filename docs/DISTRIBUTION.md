> 🌐 **中文** ｜ [English](DISTRIBUTION.en.md)

# 发布与多平台分发（DISTRIBUTION）

> 本文回答："要宣发时到底打包什么、各平台怎么出包、用户拿到什么"。配套自动化见仓库根目录 [`.github/workflows/release.yml`](../.github/workflows/release.yml)。

## 1. 你要发布的是什么

这是一个**单仓库、两个独立应用**的项目，分别面向不同人群、分别发版：

| 应用 | 面向 | 分发物 | 是否含 LLM/联网 |
| --- | --- | --- | --- |
| `configurator` | **最终用户**（不懂代码） | 安装器或独立 exe | 否，100% 离线 |
| `prep-tool` | **开发者/作者** | 安装器或独立 exe | 是（开发者自带 Key） |

**不要把这些打进应用安装包**：`sample/`、`schemas/`、源码、`node_modules`、`产品经理Agent.md`——它们是仓库内的演示/模板/文档，跟着 GitHub 仓库走即可，应用本体只需可执行文件。

> `schemas/` 内置模板已**编译进 `configurator` 二进制**（`include_str!`），用户无需额外文件即可对 `package.json`/`tsconfig.json`/`docker-compose.yml` 自动出表单。

## 2. 单 exe 还是安装器？（关键）

| 形态 | 优点 | 注意 |
| --- | --- | --- |
| **独立 exe**（`tauri build --no-bundle`） | 单文件、免安装、可直接放到配置目录旁双击 | **依赖系统 WebView2 运行时**；Win10/11 通常已预装，老系统可能缺失 → 白屏 |
| **NSIS/MSI 安装器**（`tauri build`） | **自动检测并引导安装 WebView2**、注册开始菜单/卸载项、更专业 | 体积略大；**首次构建 NSIS 安装器时 Tauri 会从 GitHub 下载 NSIS 工具链（需联网）**，MSI 同理需 WiX |

**结论**：正式宣发**优先发安装器**（解决 WebView2 这个"平台支撑文件"问题）；同时可附带独立 exe 供高级用户绿色使用。

> ⚠️ 诚实说明：**本项目当前的构建验证只产出了独立 exe**（`configurator.exe` ≈ 10.0 MB、`prep-tool.exe` ≈ 12.5 MB，经 `tauri build --no-bundle`，依赖系统 WebView2）。安装器是**已记录在案的能力**（本地 `tauri build` 或经下文 §4 的 GitHub Actions CI 出包），**尚未在本项目验证中实际产出**。

## 3. 各平台产物（必须在对应系统上构建）

**Tauri 不能从 Windows 交叉编译 macOS/Linux 包**——每个平台的安装包必须在该平台（或其 CI runner）上构建。

| 平台 | 产物 | 构建前置 |
| --- | --- | --- |
| **Windows** | `*-setup.exe`（NSIS）、`*.msi`（WiX） | Rust(MSVC) + WebView2；产物在 `src-tauri/target/release/bundle/{nsis,msi}/` |
| **macOS** | `*.dmg`、`*.app` | 在 macOS 上构建；上架/免告警需 Apple 证书签名 + 公证（notarize） |
| **Linux** | `*.deb`、`*.rpm`、`*.AppImage` | 在 Linux 上构建；需 `libwebkit2gtk-4.1-dev` 等系统依赖（见 CI） |

## 4. 推荐做法：一条命令出三平台（GitHub Actions）

仓库已附 [`.github/workflows/release.yml`](../.github/workflows/release.yml)（基于官方 `tauri-apps/tauri-action`，矩阵 = 两应用 × 三平台）。用法：

```bash
git tag v1.0.0
git push origin v1.0.0
```

推送标签后，CI 在 Windows/macOS/Linux 三种 runner 上各自为两个应用出包，并自动上传到一个**草稿版 GitHub Release**。你审阅后点击发布即可。无需本地装齐三套环境。

## 5. 本地构建（Windows）

```pwsh
# VS Developer 环境内（规避 cygwin link.exe）：
$vs = & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -latest -property installationPath
Import-Module (Join-Path $vs "Common7\Tools\Microsoft.VisualStudio.DevShell.dll")
Enter-VsDevShell -VsInstallPath $vs -DevCmdArguments "-arch=x64" -SkipAutomaticLocation
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

cd configurator           # 或 prep-tool
npx tauri build                    # NSIS + MSI 安装器（+ exe）
npx tauri build --bundles nsis     # 仅 NSIS 安装器
npx tauri build --no-bundle        # 仅独立 exe
```

> ⚠️ 切勿用 `cargo build` 直接出生产版（不会内嵌前端 → 白屏 / localhost 拒绝连接）。

> 📡 **首次构建 NSIS 安装器需联网**：Tauri 的 NSIS bundler 会在首次构建时从 GitHub 下载 NSIS 工具链（MSI 则需 WiX）；离线环境会失败。独立 exe（`--no-bundle`）无此网络依赖。

## 6. 代码签名（避免安全告警，可选但建议）

- **Windows**：未签名的 exe 会触发 SmartScreen "未知发布者" 警告。购买代码签名证书后，在应用的 `tauri.conf.json` 配置 `bundle.windows.certificateThumbprint`（或在 CI 用证书）。
- **macOS**：未签名/未公证的 app 会被 Gatekeeper 拦截。需 Apple Developer 证书签名 + `notarytool` 公证；CI 中通过 `APPLE_*` 机密注入（见 `release.yml` 注释）。

## 7. LLM 配置持久化（prep-tool）

为避免每次启动重配：

- **非密钥项（Base URL / Model）**：在界面点"保存接口设置"后写入用户配置目录的 `settings.json`，**下次启动自动回填**（其中绝不含密钥）。
- **密钥**：按优先级 **界面临时输入 > 环境变量 `CFGFORM_LLM_API_KEY` > 程序目录 `.env`**。可在界面点"记住密钥到本机 .env"（有安全确认）长期保存免重输，或点"清除已保存密钥"移除，或设置系统环境变量。`.env` 已被 `.gitignore` 忽略。
- 默认推荐 **DeepSeek**（`https://api.deepseek.com`，OpenAI 兼容；默认模型 `deepseek-chat`，若有更强的 DeepSeek "pro" 模型可填入，字段可编辑）。

## 8. 用户侧使用方式

1. 下载并运行 `configurator` 安装器（或绿色 exe）。
2. 把它放到（或安装后指向）含目标配置文件 + `.cfgform` 边车的目录。
3. 双击运行 → 自动扫描 → 表单编辑 → Dry-run 预览 → 保存（自动备份）。

## 9. 发布清单（建议）

- [ ] 版本号同步（各应用 `package.json` 与 `src-tauri/tauri.conf.json` 的 `version`）。
- [ ] `LICENSE` 与 README 末尾 `<Your Name>` 已替换为你的署名。
- [ ] 为安装包生成校验和（如 `Get-FileHash *.exe -Algorithm SHA256`）。
- [ ] Release 说明附上：支持平台、WebView2 提示、变更日志。

---
变更日志：2026-06-20 新增分发文档，明确两应用分发物、单 exe vs 安装器、三平台出包（含 GitHub Actions 自动化）、签名与 LLM 持久化说明。

变更日志：2026-06-21 文档准确性校订——补充"首次构建 NSIS 安装器需从 GitHub 下载工具链（需联网）"的诚实告示；明确本项目当前仅实测独立 exe（10.0/12.5 MB），安装器为已具备但尚未产出的能力；补充 DeepSeek 默认模型 `deepseek-chat` 与「清除已保存密钥」按钮。
