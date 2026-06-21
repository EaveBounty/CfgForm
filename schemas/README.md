> 🌐 **中文** ｜ [English](README.en.md)

# 内置 Schema 库（schemas）

本目录收录**常见配置文件的精选 `.cfgform` 模板**。它们都是手工策划、合法的 v2.0 边车，开箱即可让 `configurator` 把这些常见文件渲染成带中文说明的可视化表单——**无需运行 `prep-tool`、无需 LLM**。

## 当前条目

| 模板 | 适用目标文件 | format | 说明 |
| --- | --- | --- | --- |
| `package.json.cfgform` | `package.json` | `json` | npm 包清单：name/version/scripts/dependencies 等常见字段 + 中文提示 |
| `tsconfig.json.cfgform` | `tsconfig.json` | `json` | TypeScript 编译配置：compilerOptions 常见项 + 枚举与提示 |
| `docker-compose.yml.cfgform` | `docker-compose.yml` | `compose` | 通用 Compose：services（任意服务名）/ volumes / networks 模板 |

## 如何使用（手动复制 / 定制）

> 提示：`package.json` / `tsconfig.json` / `docker-compose.yml` 这三类文件已由 `configurator` **自动匹配内置模板**（见下文「自动匹配」），通常无需手动复制；以下步骤适用于**其它文件名**或你想**随附并定制**一份边车的情形。

1. 选一个与你的文件匹配的模板，例如 `package.json.cfgform`。
2. **复制到你的目标文件旁边的同一目录**，并**改名为 `<你的文件名>.cfgform`**（追加式配对）：

   ```
   你的项目/
   ├─ package.json
   └─ package.json.cfgform   ← 由本目录的 package.json.cfgform 复制并改名而来
   ```

   > 命名规则是「目标完整文件名 + `.cfgform`」。例如目标是 `tsconfig.app.json`，则边车应为 `tsconfig.app.json.cfgform`，并把模板内的 `"target"` 改成 `"tsconfig.app.json"`。

3. 按需微调：修改 `target`（务必与实际文件名一致）、增删字段、为含密码/令牌的字段补 `"ui:secret": true`、为不希望用户改的字段补 `"ui:readOnly": true`。
4. 用 `configurator` 打开该目录即可看到表单。

## 设计约定

- 这些模板**适度宽松**（`additionalProperties: true`、`required` 最小化），以便适配各种真实文件而不误报。
- `meta.generatedBy` 标为 `curated/cfgform-schemas`、`meta.llm.used: false`，表示人工策划、未经 LLM。
- 字段类型遵循对应格式的规范数据树：`json` 保留原生类型；`compose` 走 YAML 解析。

## 自动匹配（已实现）

> **自动匹配已在 `configurator` 中落地。** `package.json`、`tsconfig.json`、`docker-compose.yml`（含 `docker-compose.yaml`/`compose.yml`）这三类常见文件的模板已**编译期内置**进 `configurator` 二进制（`include_str!`）。当目录中存在这些目标文件却缺少自带 `.cfgform`/`.jsonform` 时，`configurator` 会**自动套用内置模板**渲染表单（顶部横幅标注"使用内置 schema 库"），无需手动复制改名；保存照常写回目标文件且不自动写边车，另提供可选的"将内置模板另存为本目录的 .cfgform"按钮。
>
> 本目录的 `.cfgform` 文件与内置模板一致，主要作为**可复制、可定制的模板**：用于上述三类文件之外的文件名（如 `tsconfig.app.json`），或当你想在项目中随附并微调一份边车时。此时按上面的"手动复制"流程使用。

欢迎贡献更多条目，提交规范见 [`../CONTRIBUTING.md`](../CONTRIBUTING.md) 第 5 节。
