import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import Form, { type IChangeEvent } from "@rjsf/core";
import validator from "@rjsf/validator-ajv8";
import type { RJSFSchema, UiSchema } from "@rjsf/utils";
import "./App.css";

interface StackInfo {
  stack: string;
  markers: string[];
}
interface SourceFile {
  path: string;
  content: string;
}
interface SourcesBundle {
  readme: string;
  readme_path: string;
  files: SourceFile[];
}
interface LlmConfigStatus {
  base_url: string;
  model: string;
  key_source: string;
  has_key: boolean;
}
interface CfgForm {
  $cfgform: string;
  target: string;
  format: string;
  title: string;
  schema: RJSFSchema;
  ui: UiSchema;
  meta: Record<string, unknown>;
}
interface GenResult {
  cfgform: CfgForm;
  llm_raw_excerpt: string;
}
interface WriteResult {
  cfgform_path: string;
  log_path: string;
  actions: string[];
}

const STEPS = ["选择项目", "配置 LLM", "生成预览", "写入项目"];

const KEY_SOURCE_LABEL: Record<string, string> = {
  ui: "界面临时输入（仅内存）",
  env: "系统环境变量",
  dotenv: "程序目录 .env 文件",
  none: "未检测到密钥",
};

const FORMAT_LABEL: Record<string, string> = {
  json: "JSON",
  env: ".env / KV",
  toml: "TOML",
  yaml: "YAML",
  ini: "INI / properties",
  compose: "Docker Compose（YAML）",
};

const CONFIG_EXTENSIONS = [
  "json",
  "env",
  "toml",
  "yml",
  "yaml",
  "ini",
  "properties",
  "conf",
];

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/// 预览用：把所有 ui:secret 字段额外标为 password 控件以默认掩码显示。
function maskSecrets(ui: UiSchema): UiSchema {
  const clone: Record<string, unknown> = isPlainObject(ui)
    ? JSON.parse(JSON.stringify(ui))
    : {};
  const walk = (node: Record<string, unknown>) => {
    if (node["ui:secret"] === true && !node["ui:widget"]) {
      node["ui:widget"] = "password";
    }
    for (const k of Object.keys(node)) {
      const v = node[k];
      if (isPlainObject(v)) walk(v);
    }
  };
  walk(clone);
  return clone as UiSchema;
}

function App() {
  const [step, setStep] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  // Step 1
  const [projectDir, setProjectDir] = useState("");
  const [configPath, setConfigPath] = useState("");
  const [format, setFormat] = useState("");
  const [stackInfo, setStackInfo] = useState<StackInfo | null>(null);
  const [configData, setConfigData] = useState<unknown>(null);
  const [baseSchema, setBaseSchema] = useState<RJSFSchema | null>(null);
  const [sources, setSources] = useState<SourcesBundle | null>(null);
  const [title, setTitle] = useState("");

  // Step 2
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [llmStatus, setLlmStatus] = useState<LlmConfigStatus | null>(null);
  const [settingsMsg, setSettingsMsg] = useState("");

  // Step 3
  const [genResult, setGenResult] = useState<GenResult | null>(null);
  const [formData, setFormData] = useState<unknown>(null);

  // Step 4
  const [writeResult, setWriteResult] = useState<WriteResult | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const s = await invoke<{ base_url: string; model: string }>(
          "load_settings",
        );
        setBaseUrl((prev) => (prev.trim() ? prev : s.base_url));
        setModel((prev) => (prev.trim() ? prev : s.model));
      } catch {
        /* 忽略：回退默认值 */
      }
    })();
  }, []);

  useEffect(() => {
    if (step !== 1) return;
    let cancelled = false;
    (async () => {
      try {
        const status = await invoke<LlmConfigStatus>("resolve_llm_config", {
          uiBaseUrl: baseUrl,
          uiModel: model,
          uiHasKey: apiKey.trim().length > 0,
        });
        if (!cancelled) setLlmStatus(status);
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [step, baseUrl, model, apiKey]);

  async function pickProjectDir() {
    setError("");
    try {
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected !== "string") return;
      setProjectDir(selected);
      setConfigPath("");
      setFormat("");
      setConfigData(null);
      setBaseSchema(null);
      setSources(null);
      setStackInfo(null);
      setBusy(true);
      const info = await invoke<StackInfo>("detect_stack", { dir: selected });
      setStackInfo(info);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function pickConfigFile() {
    setError("");
    if (!projectDir) {
      setError("请先选择项目文件夹。");
      return;
    }
    try {
      const selected = await open({
        directory: false,
        multiple: false,
        defaultPath: projectDir,
        filters: [
          { name: "配置文件", extensions: CONFIG_EXTENSIONS },
          { name: "全部文件", extensions: ["*"] },
        ],
      });
      if (typeof selected !== "string") return;
      setConfigPath(selected);
      setBusy(true);

      const content = await invoke<string>("read_text_file", { path: selected });
      const fmt = await invoke<string>("detect_format", {
        path: selected,
        content,
      });
      setFormat(fmt);

      const data = await invoke<unknown>("read_target_as_value", {
        path: selected,
        format: fmt,
      });
      setConfigData(data);
      setFormData(data);

      const schema = await invoke<RJSFSchema>("infer_schema", { data });
      setBaseSchema(schema);

      const src = await invoke<SourcesBundle>("gather_sources", {
        dir: projectDir,
      });
      setSources(src);

      const name =
        selected.replace(/\\/g, "/").split("/").pop() ?? "config";
      if (!title) setTitle(name);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function generate() {
    setError("");
    if (!stackInfo || !baseSchema || !sources) {
      setError("请先在步骤 1 选择项目与配置文件。");
      return;
    }
    setBusy(true);
    setGenResult(null);
    try {
      const res = await invoke<GenResult>("generate_metadata", {
        args: {
          dir: projectDir,
          config_path: configPath,
          format,
          stack: stackInfo.stack,
          base_schema: baseSchema,
          sources,
          base_url: baseUrl,
          model,
          api_key: apiKey.trim() ? apiKey : null,
          title,
        },
      });
      setGenResult(res);
      if (configData != null) setFormData(configData);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function writeForm() {
    setError("");
    if (!genResult) {
      setError("请先在步骤 3 生成元数据。");
      return;
    }
    setBusy(true);
    try {
      const res = await invoke<WriteResult>("write_cfgform", {
        dir: projectDir,
        targetPath: configPath,
        cfgformValue: genResult.cfgform,
      });
      setWriteResult(res);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const configName =
    configPath.replace(/\\/g, "/").split("/").pop() ?? "（未选择）";

  const canNext =
    step === 0
      ? Boolean(projectDir && configPath && baseSchema)
      : step === 1
        ? true
        : step === 2
          ? Boolean(genResult)
          : false;

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <span className="logo-dot" />
          <div>
            <h1>prep-tool</h1>
            <p className="subtitle">
              为多格式配置（JSON/.env/TOML/YAML/INI/Compose）生成 .cfgform 边车元数据
            </p>
          </div>
        </div>
        <span className="version">v2.0.0</span>
      </header>

      <nav className="stepper">
        {STEPS.map((label, i) => (
          <div
            key={label}
            className={
              "step" +
              (i === step ? " active" : "") +
              (i < step ? " done" : "")
            }
          >
            <span className="step-index">{i + 1}</span>
            <span className="step-label">{label}</span>
          </div>
        ))}
      </nav>

      {error && (
        <div className="banner error">
          <strong>出错了：</strong>
          {error}
        </div>
      )}

      <main className="content">
        {step === 0 && (
          <section className="card">
            <h2>步骤 1 · 选择项目</h2>
            <p className="hint">
              选择项目文件夹，并指定要生成边车的配置文件（支持
              JSON/.env/TOML/YAML/INI/Compose）。
            </p>
            <div className="field-row">
              <button className="btn primary" onClick={pickProjectDir} disabled={busy}>
                选择项目文件夹
              </button>
              <span className="path-display">{projectDir || "尚未选择"}</span>
            </div>
            <div className="field-row">
              <button
                className="btn"
                onClick={pickConfigFile}
                disabled={busy || !projectDir}
              >
                选择配置文件
              </button>
              <span className="path-display">{configPath || "尚未选择"}</span>
            </div>

            {(stackInfo || format) && (
              <div className="info-block">
                {format && (
                  <div className="kv">
                    <span className="k">检测到格式</span>
                    <span className="v badge ok">
                      {FORMAT_LABEL[format] ?? format}
                    </span>
                  </div>
                )}
                {stackInfo && (
                  <>
                    <div className="kv">
                      <span className="k">探测到技术栈</span>
                      <span className="v badge">{stackInfo.stack}</span>
                    </div>
                    <div className="kv">
                      <span className="k">命中标记文件</span>
                      <span className="v">
                        {stackInfo.markers.length
                          ? stackInfo.markers.map((m) => (
                              <code key={m} className="chip">
                                {m}
                              </code>
                            ))
                          : "（无）"}
                      </span>
                    </div>
                  </>
                )}
              </div>
            )}

            {baseSchema && (
              <details className="collapsible">
                <summary>查看推断出的基线 Schema（draft-07）</summary>
                <pre className="code">{JSON.stringify(baseSchema, null, 2)}</pre>
              </details>
            )}
            {sources && (
              <div className="info-block">
                <div className="kv">
                  <span className="k">将参考的文件</span>
                  <span className="v">
                    {[
                      ...(sources.readme_path ? [sources.readme_path] : []),
                      ...sources.files.map((f) => f.path),
                    ].map((p) => (
                      <code key={p} className="chip">
                        {p}
                      </code>
                    )) || "（无）"}
                  </span>
                </div>
              </div>
            )}
            <div className="field-row">
              <label className="inline-label">表单标题</label>
              <input
                className="input"
                value={title}
                onChange={(e) => setTitle(e.currentTarget.value)}
                placeholder="用于 .cfgform 的 title"
              />
            </div>
          </section>
        )}

        {step === 1 && (
          <section className="card">
            <h2>步骤 2 · 配置 LLM</h2>
            <div className="notice">
              凭据优先级：<b>界面临时输入</b> &gt; <b>环境变量</b> &gt;{" "}
              <b>程序目录 .env</b>。 密钥仅保存在内存中，
              <b>绝不写入磁盘、日志或 .cfgform</b>。
            </div>
            <div className="field-col">
              <label className="inline-label">Base URL（OpenAI 兼容）</label>
              <input
                className="input"
                value={baseUrl}
                onChange={(e) => setBaseUrl(e.currentTarget.value)}
                placeholder="https://api.deepseek.com（DeepSeek，推荐）"
              />
            </div>
            <div className="field-col">
              <label className="inline-label">Model</label>
              <input
                className="input"
                value={model}
                onChange={(e) => setModel(e.currentTarget.value)}
                placeholder="推荐 deepseek-chat；如有更强的 pro / V 系列模型可填其 id"
              />
            </div>
            <div className="field-col">
              <label className="inline-label">临时密钥（仅本次内存，不保存）</label>
              <input
                className="input"
                type="password"
                value={apiKey}
                onChange={(e) => setApiKey(e.currentTarget.value)}
                placeholder="留空则使用环境变量 / .env"
                autoComplete="off"
              />
            </div>

            {llmStatus && (
              <div className="info-block">
                <div className="kv">
                  <span className="k">当前生效 Base URL</span>
                  <span className="v">{llmStatus.base_url}</span>
                </div>
                <div className="kv">
                  <span className="k">当前生效 Model</span>
                  <span className="v">
                    {llmStatus.model || "（未设置，将用 deepseek-chat）"}
                  </span>
                </div>
                <div className="kv">
                  <span className="k">密钥来源</span>
                  <span
                    className={"v badge " + (llmStatus.has_key ? "ok" : "warn")}
                  >
                    {KEY_SOURCE_LABEL[llmStatus.key_source] ??
                      llmStatus.key_source}
                  </span>
                </div>
              </div>
            )}

            <div
              className="field-row"
              style={{ flexWrap: "wrap", gap: 8, marginTop: 12 }}
            >
              <button
                className="btn"
                onClick={async () => {
                  setError("");
                  setSettingsMsg("");
                  try {
                    await invoke("save_settings", { baseUrl, model });
                    setSettingsMsg(
                      "已保存接口设置（不含密钥），下次启动自动填充。",
                    );
                  } catch (e) {
                    setError(String(e));
                  }
                }}
              >
                保存接口设置（不含密钥）
              </button>
              <button
                className="btn"
                disabled={apiKey.trim().length === 0}
                onClick={async () => {
                  setError("");
                  setSettingsMsg("");
                  const ok = window.confirm(
                    "将把密钥以明文写入程序目录的 .env 文件，长期保存、免重输。请确保该目录安全（.env 已被 .gitignore 忽略）。是否继续？",
                  );
                  if (!ok) return;
                  try {
                    const p = await invoke<string>("save_key_to_dotenv", {
                      apiKey,
                      baseUrl,
                      model,
                    });
                    setSettingsMsg("密钥已写入：" + p);
                  } catch (e) {
                    setError(String(e));
                  }
                }}
              >
                记住密钥到本机 .env
              </button>
              <button
                className="btn"
                onClick={async () => {
                  setError("");
                  setSettingsMsg("");
                  try {
                    await invoke("clear_dotenv_key");
                    setSettingsMsg("已清除 .env 中保存的密钥（保留接口设置）。");
                  } catch (e) {
                    setError(String(e));
                  }
                }}
              >
                清除已保存密钥
              </button>
            </div>
            {settingsMsg && (
              <div className="info-block" style={{ marginTop: 8 }}>
                {settingsMsg}
              </div>
            )}
          </section>
        )}

        {step === 2 && (
          <section className="card">
            <h2>步骤 3 · 生成并预览</h2>
            <p className="hint">
              本工具将由配置值推断结构（格式：
              <b>{FORMAT_LABEL[format] ?? format ?? "未知"}</b>
              ），并调用 LLM 读取 README /
              源码精化类型、补充约束、条件校验、中文帮助、密钥与只读标记。
            </p>
            <div className="field-row">
              <button className="btn primary" onClick={generate} disabled={busy}>
                {busy ? "生成中…" : "生成边车元数据"}
              </button>
              {busy && <span className="spinner" aria-hidden />}
            </div>

            {genResult && (
              <>
                <details className="collapsible" open>
                  <summary>生成的 Schema（折叠查看）</summary>
                  <pre className="code">
                    {JSON.stringify(genResult.cfgform.schema, null, 2)}
                  </pre>
                </details>
                <details className="collapsible">
                  <summary>生成的 uiSchema（含 ui:secret / ui:readOnly）</summary>
                  <pre className="code">
                    {JSON.stringify(genResult.cfgform.ui, null, 2)}
                  </pre>
                </details>

                <h3>实时表单预览（密钥字段默认掩码）</h3>
                <div className="rjsf-preview">
                  <Form
                    schema={genResult.cfgform.schema}
                    uiSchema={maskSecrets(genResult.cfgform.ui)}
                    validator={validator}
                    formData={formData}
                    liveValidate
                    onChange={(e: IChangeEvent) => setFormData(e.formData)}
                  >
                    <></>
                  </Form>
                </div>

                <details className="collapsible">
                  <summary>LLM 原始输出摘录（透明留痕，前 ~500 字）</summary>
                  <pre className="code">{genResult.llm_raw_excerpt}</pre>
                </details>
              </>
            )}
          </section>
        )}

        {step === 3 && (
          <section className="card">
            <h2>步骤 4 · 写入项目</h2>
            <p className="hint">
              将 <code>{configName}.cfgform</code>（追加式配对）写入项目目录，
              <b>不会修改原 {configName}</b>，并追加人话审计日志。
            </p>
            <div className="field-row">
              <button
                className="btn primary"
                onClick={writeForm}
                disabled={busy || !genResult}
              >
                {busy ? "写入中…" : "写入 .cfgform 到项目"}
              </button>
            </div>

            {writeResult && (
              <div className="info-block success">
                <h3>已完成，变更明细：</h3>
                <ul className="trace">
                  {writeResult.actions.map((a, i) => (
                    <li key={i}>{a}</li>
                  ))}
                </ul>
                <div className="kv">
                  <span className="k">边车文件</span>
                  <span className="v">
                    <code>{writeResult.cfgform_path}</code>
                  </span>
                </div>
                <div className="kv">
                  <span className="k">审计日志</span>
                  <span className="v">
                    <code>{writeResult.log_path}</code>
                  </span>
                </div>
                <p className="emphasis">
                  原 {configName} 未被修改（原名原样）；密钥未写入任何文件或日志。
                </p>
              </div>
            )}
          </section>
        )}
      </main>

      <footer className="navbar">
        <button
          className="btn"
          onClick={() => setStep((s) => Math.max(0, s - 1))}
          disabled={step === 0 || busy}
        >
          上一步
        </button>
        <div className="grow" />
        {step < STEPS.length - 1 && (
          <button
            className="btn primary"
            onClick={() => setStep((s) => Math.min(STEPS.length - 1, s + 1))}
            disabled={!canNext || busy}
          >
            下一步
          </button>
        )}
      </footer>
    </div>
  );
}

export default App;
