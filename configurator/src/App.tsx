import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import Form, { type IChangeEvent } from "@rjsf/core";
import { customizeValidator } from "@rjsf/validator-ajv8";
import type {
  RJSFSchema,
  UiSchema,
  RJSFValidationError,
  WidgetProps,
} from "@rjsf/utils";
import "./App.css";

interface PairInfo {
  cfgform_path: string;
  target_path: string;
  target_name: string;
  title: string;
  format: string;
  target_exists: boolean;
  error: string | null;
  builtin: boolean;
  source: string;
}

interface LoadResult {
  title: string;
  target_name: string;
  format: string;
  schema: RJSFSchema;
  ui: UiSchema;
  data: unknown;
  profiles: unknown;
}

interface PreviewResult {
  new_text: string;
  diff_lines: string[];
}

interface SaveResult {
  backup_path: string;
  changes: string[];
}

interface Toast {
  type: "success" | "error";
  text: string;
}

interface FriendlyError {
  field: string;
  reason: string;
}

interface Profiles {
  active?: string;
  list?: string[];
  overrides?: Record<string, Record<string, unknown>>;
}

const validator = customizeValidator();

const FORMAT_BADGES: Record<string, string> = {
  json: "JSON",
  env: "ENV",
  toml: "TOML",
  yaml: "YAML",
  ini: "INI",
  compose: "Compose",
};

function formatBadge(format: string): string {
  return FORMAT_BADGES[format] ?? format.toUpperCase();
}

function translateErrors(errors: RJSFValidationError[]): FriendlyError[] {
  return errors.map((err) => {
    const name = err.name ?? "";
    let reason: string;
    switch (name) {
      case "required":
        reason = "缺少必填项";
        break;
      case "type":
        reason = "类型不正确";
        break;
      case "minimum":
      case "maximum":
      case "exclusiveMinimum":
      case "exclusiveMaximum":
      case "minLength":
      case "maxLength":
      case "minItems":
      case "maxItems":
        reason = "超出允许范围";
        break;
      case "pattern":
      case "format":
        reason = "格式不正确";
        break;
      case "enum":
      case "const":
        reason = "不是允许的选项";
        break;
      case "if":
      case "then":
      case "else":
      case "dependencies":
      case "dependentRequired":
        reason = "不满足条件约束";
        break;
      default:
        reason = err.message ?? "不符合要求";
    }

    let field = err.property ? err.property.replace(/^\./, "") : "";
    if (name === "required") {
      const missing = (err.params as { missingProperty?: string } | undefined)
        ?.missingProperty;
      if (missing) {
        field = field ? `${field}.${missing}` : missing;
      }
    }
    if (!field) {
      field = "（根级）";
    }
    return { field, reason };
  });
}

function basename(p: string): string {
  const parts = p.split(/[\\/]/);
  return parts[parts.length - 1] || p;
}

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function collectUiFlags(
  ui: unknown,
  base = "",
  secret: string[] = [],
  readonly: string[] = [],
): { secret: string[]; readonly: string[] } {
  if (!isPlainObject(ui)) return { secret, readonly };
  if (base && ui["ui:secret"] === true) secret.push(base);
  if (base && ui["ui:readOnly"] === true) readonly.push(base);
  for (const k of Object.keys(ui)) {
    if (k.startsWith("ui:")) continue;
    const child = ui[k];
    if (isPlainObject(child)) {
      collectUiFlags(child, base ? `${base}.${k}` : k, secret, readonly);
    }
  }
  return { secret, readonly };
}

function getUiNode(
  ui: Record<string, unknown>,
  path: string,
): Record<string, unknown> {
  const segs = path.split(".");
  let node: Record<string, unknown> = ui;
  for (const seg of segs) {
    if (!isPlainObject(node[seg])) {
      node[seg] = {};
    }
    node = node[seg] as Record<string, unknown>;
  }
  return node;
}

function buildUiSchema(
  ui: UiSchema,
  secretPaths: string[],
  readonlyPaths: string[],
): UiSchema {
  const clone = JSON.parse(JSON.stringify(ui ?? {})) as Record<string, unknown>;
  for (const p of secretPaths) {
    const node = getUiNode(clone, p);
    if (!node["ui:widget"]) node["ui:widget"] = "secretText";
  }
  for (const p of readonlyPaths) {
    const node = getUiNode(clone, p);
    node["ui:disabled"] = true;
    const existing = typeof node["ui:help"] === "string" ? node["ui:help"] : "";
    const lock = "🔒 作者锁定，不可修改";
    node["ui:help"] = existing ? `${existing}（${lock}）` : lock;
  }
  return clone as UiSchema;
}

function collectDefaults(
  schema: unknown,
  base = "",
  out: Record<string, unknown> = {},
): Record<string, unknown> {
  if (!isPlainObject(schema)) return out;
  const props = schema.properties;
  if (isPlainObject(props)) {
    for (const [k, sub] of Object.entries(props)) {
      const path = base ? `${base}.${k}` : k;
      if (isPlainObject(sub)) {
        if ("default" in sub) out[path] = sub.default;
        if (isPlainObject(sub.properties)) collectDefaults(sub, path, out);
      }
    }
  }
  return out;
}

function getAtPath(obj: unknown, path: string): unknown {
  const segs = path.split(".");
  let cur: unknown = obj;
  for (const seg of segs) {
    if (!isPlainObject(cur)) return undefined;
    cur = cur[seg];
  }
  return cur;
}

function setAtPath(obj: unknown, path: string, value: unknown): unknown {
  const segs = path.split(".");
  const root: Record<string, unknown> = isPlainObject(obj) ? { ...obj } : {};
  let cur = root;
  for (let i = 0; i < segs.length - 1; i++) {
    const seg = segs[i];
    const next = cur[seg];
    cur[seg] = isPlainObject(next) ? { ...next } : {};
    cur = cur[seg] as Record<string, unknown>;
  }
  cur[segs[segs.length - 1]] = value;
  return root;
}

function deepEqual(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

// Accept either dot-path (a.b.c) or JSON Pointer (/a/b/c) override keys.
function normalizeOverrideKey(key: string): string {
  if (key.startsWith("/")) {
    return key
      .slice(1)
      .split("/")
      .map((s) => s.replace(/~1/g, "/").replace(/~0/g, "~"))
      .join(".");
  }
  return key;
}

// Effective data = base target data with overrides[active] applied on top.
function applyOverrides(
  base: unknown,
  overrides: Record<string, unknown> | undefined,
): unknown {
  if (!overrides) return base;
  let next = base;
  for (const [k, v] of Object.entries(overrides)) {
    next = setAtPath(next, normalizeOverrideKey(k), v);
  }
  return next;
}

// Flatten leaf differences (current vs base) into a dot-path -> value map.
// Objects recurse; arrays/scalars are treated as leaves (whole-value override).
function flattenDiff(
  base: unknown,
  current: unknown,
  prefix = "",
  out: Record<string, unknown> = {},
): Record<string, unknown> {
  if (isPlainObject(current)) {
    const baseObj = isPlainObject(base) ? base : {};
    for (const [k, cur] of Object.entries(current)) {
      const path = prefix ? `${prefix}.${k}` : k;
      flattenDiff(baseObj[k], cur, path, out);
    }
    return out;
  }
  if (current !== undefined && !deepEqual(base, current)) {
    out[prefix] = current;
  }
  return out;
}

function maskText(text: string, secrets: string[], reveal: boolean): string {
  if (reveal) return text;
  let out = text;
  for (const s of secrets) {
    if (s) out = out.split(s).join("••••••");
  }
  return out;
}

function SecretWidget(props: WidgetProps) {
  const { id, value, onChange, disabled, readonly, placeholder, autofocus } =
    props;
  const [show, setShow] = useState(false);
  return (
    <div className="secret-field">
      <input
        id={id}
        type={show ? "text" : "password"}
        className="secret-input"
        value={value ?? ""}
        disabled={disabled || readonly}
        placeholder={placeholder}
        autoFocus={autofocus}
        autoComplete="off"
        onChange={(e) =>
          onChange(e.target.value === "" ? undefined : e.target.value)
        }
      />
      <button
        type="button"
        className="btn btn-secondary secret-toggle"
        onClick={() => setShow((s) => !s)}
      >
        {show ? "隐藏" : "显示"}
      </button>
    </div>
  );
}

const WIDGETS = { secretText: SecretWidget };

function App() {
  const [scanDir, setScanDir] = useState<string>("");
  const [pairs, setPairs] = useState<PairInfo[]>([]);
  const [selectedIdx, setSelectedIdx] = useState<number>(-1);

  const [current, setCurrent] = useState<LoadResult | null>(null);
  const [formData, setFormData] = useState<unknown>(undefined);
  const [baseData, setBaseData] = useState<unknown>(undefined);
  const [errors, setErrors] = useState<RJSFValidationError[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);

  const [secretPaths, setSecretPaths] = useState<string[]>([]);
  const [uiSchema, setUiSchema] = useState<UiSchema>({});
  const [defaults, setDefaults] = useState<Record<string, unknown>>({});
  const [selectedProfile, setSelectedProfile] = useState<string>("");

  const [toast, setToast] = useState<Toast | null>(null);
  const [lastChanges, setLastChanges] = useState<string[]>([]);
  const [saving, setSaving] = useState<boolean>(false);

  const [preview, setPreview] = useState<PreviewResult | null>(null);
  const [previewOpen, setPreviewOpen] = useState<boolean>(false);
  const [revealSecrets, setRevealSecrets] = useState<boolean>(false);

  const [auditOpen, setAuditOpen] = useState<boolean>(false);
  const [auditText, setAuditText] = useState<string>("");

  const selectedPair = selectedIdx >= 0 ? pairs[selectedIdx] : undefined;
  const isOrphan =
    !!selectedPair && selectedPair.cfgform_path === "" && !selectedPair.builtin;
  const isBuiltin = !!selectedPair && selectedPair.builtin;
  const friendlyErrors = useMemo(() => translateErrors(errors), [errors]);

  const profiles: Profiles | null =
    current && isPlainObject(current.profiles)
      ? (current.profiles as Profiles)
      : null;

  const secretValues = useMemo(() => {
    return secretPaths
      .map((p) => getAtPath(formData, p))
      .filter((v): v is string => typeof v === "string" && v.length > 0);
  }, [secretPaths, formData]);

  const changedDefaults = useMemo(() => {
    const out: { path: string; def: unknown; cur: unknown }[] = [];
    for (const [path, def] of Object.entries(defaults)) {
      const cur = getAtPath(formData, path);
      if (cur !== undefined && !deepEqual(cur, def)) {
        out.push({ path, def, cur });
      }
    }
    return out;
  }, [defaults, formData]);

  const refreshAudit = useCallback(async (dir: string) => {
    if (!dir) return;
    try {
      const text = await invoke<string>("read_audit_tail", {
        dir,
        maxLines: 200,
      });
      setAuditText(text);
    } catch {
      setAuditText("");
    }
  }, []);

  const loadPair = useCallback(async (pair: PairInfo) => {
    setLoadError(null);
    setCurrent(null);
    setFormData(undefined);
    setBaseData(undefined);
    setErrors([]);
    setLastChanges([]);
    setSecretPaths([]);
    setUiSchema({});
    setDefaults({});
    setSelectedProfile("");

    if (!pair.builtin && pair.cfgform_path === "") {
      return;
    }
    if (!pair.target_exists || pair.error) {
      setLoadError(pair.error ?? "无法加载此配置");
      return;
    }
    try {
      const res = pair.builtin
        ? await invoke<LoadResult>("load_builtin", {
            targetPath: pair.target_path,
          })
        : await invoke<LoadResult>("load_pair", {
            cfgformPath: pair.cfgform_path,
          });
      setCurrent(res);
      setBaseData(res.data);
      const flags = collectUiFlags(res.ui);
      setSecretPaths(flags.secret);
      setUiSchema(buildUiSchema(res.ui, flags.secret, flags.readonly));
      setDefaults(collectDefaults(res.schema));

      let effective: unknown = res.data;
      if (isPlainObject(res.profiles)) {
        const p = res.profiles as Profiles;
        const active = p.active ?? (p.list && p.list[0]) ?? "";
        setSelectedProfile(active);
        effective = applyOverrides(res.data, p.overrides?.[active]);
      }
      setFormData(effective);
      const result = validator.validateFormData(effective, res.schema);
      setErrors(result.errors);
    } catch (e) {
      setLoadError(String(e));
    }
  }, []);

  const scan = useCallback(
    async (dir: string) => {
      try {
        const found = await invoke<PairInfo[]>("scan_dir", { dir });
        setPairs(found);
        const firstForm = found.findIndex((p) => p.cfgform_path !== "");
        const idx = firstForm >= 0 ? firstForm : found.length > 0 ? 0 : -1;
        setSelectedIdx(idx);
        if (idx >= 0) {
          await loadPair(found[idx]);
        } else {
          setCurrent(null);
          setLoadError(null);
        }
        await refreshAudit(dir);
      } catch (e) {
        setPairs([]);
        setSelectedIdx(-1);
        setToast({ type: "error", text: `扫描目录失败：${String(e)}` });
      }
    },
    [loadPair, refreshAudit],
  );

  useEffect(() => {
    (async () => {
      try {
        const dir = await invoke<string>("default_scan_dir");
        setScanDir(dir);
        await scan(dir);
      } catch (e) {
        setToast({ type: "error", text: `无法确定扫描目录：${String(e)}` });
      }
    })();
  }, [scan]);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 6000);
    return () => clearTimeout(t);
  }, [toast]);

  const chooseFolder = useCallback(async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      setScanDir(selected);
      await scan(selected);
    }
  }, [scan]);

  const onSelect = useCallback(
    async (idx: number) => {
      setSelectedIdx(idx);
      await loadPair(pairs[idx]);
    },
    [pairs, loadPair],
  );

  const onFormChange = useCallback((e: IChangeEvent) => {
    setFormData(e.formData);
    setErrors(e.errors ?? []);
  }, []);

  const revalidate = useCallback(
    (data: unknown) => {
      if (!current) return;
      const result = validator.validateFormData(data, current.schema);
      setErrors(result.errors);
    },
    [current],
  );

  const resetField = useCallback(
    (path: string, def: unknown) => {
      const next = setAtPath(formData, path, def);
      setFormData(next);
      revalidate(next);
    },
    [formData, revalidate],
  );

  const applyProfile = useCallback(
    (name: string) => {
      setSelectedProfile(name);
      const next = applyOverrides(baseData, profiles?.overrides?.[name]);
      setFormData(next);
      revalidate(next);
    },
    [profiles, baseData, revalidate],
  );

  const onSaveProfileOverride = useCallback(async () => {
    if (!current || !selectedPair || selectedPair.cfgform_path === "") return;
    const active = selectedProfile;
    if (!active) {
      setToast({ type: "error", text: "未选择目标环境" });
      return;
    }
    const diff = flattenDiff(baseData, formData);
    setSaving(true);
    try {
      await invoke("save_profile_overrides", {
        cfgformPath: selectedPair.cfgform_path,
        active,
        overridesValue: diff,
      });
      setCurrent((prev) => {
        if (!prev || !isPlainObject(prev.profiles)) return prev;
        const p = JSON.parse(JSON.stringify(prev.profiles)) as Profiles;
        p.overrides = p.overrides ?? {};
        p.overrides[active] = diff as Record<string, unknown>;
        return { ...prev, profiles: p };
      });
      const n = Object.keys(diff).length;
      setToast({
        type: "success",
        text: `已写入【${active}】环境覆盖（${n} 项差异）`,
      });
      await refreshAudit(scanDir);
    } catch (e) {
      setToast({ type: "error", text: `保存环境覆盖失败：${String(e)}` });
    } finally {
      setSaving(false);
    }
  }, [
    current,
    selectedPair,
    selectedProfile,
    baseData,
    formData,
    refreshAudit,
    scanDir,
  ]);

  const onSaveBuiltinSidecar = useCallback(async () => {
    if (!selectedPair || !selectedPair.builtin) return;
    setSaving(true);
    try {
      const p = await invoke<string>("save_builtin_sidecar", {
        targetPath: selectedPair.target_path,
      });
      setToast({ type: "success", text: `已写出边车：${basename(p)}` });
      await scan(scanDir);
    } catch (e) {
      setToast({ type: "error", text: `写出边车失败：${String(e)}` });
    } finally {
      setSaving(false);
    }
  }, [selectedPair, scan, scanDir]);

  const onSave = useCallback(async () => {
    if (!current || !selectedPair) return;
    const result = validator.validateFormData(formData, current.schema);
    if (result.errors.length > 0) {
      setErrors(result.errors);
      setToast({
        type: "error",
        text: "存在校验错误，已阻止保存，请先修正下方红色提示项",
      });
      return;
    }
    setSaving(true);
    try {
      const res = await invoke<PreviewResult>("preview_save", {
        targetPath: selectedPair.target_path,
        format: current.format,
        data: formData,
      });
      setPreview(res);
      setRevealSecrets(false);
      setPreviewOpen(true);
    } catch (e) {
      setToast({ type: "error", text: `生成预览失败：${String(e)}` });
    } finally {
      setSaving(false);
    }
  }, [current, selectedPair, formData]);

  const onConfirmWrite = useCallback(async () => {
    if (!current || !selectedPair) return;
    setSaving(true);
    try {
      const res = await invoke<SaveResult>("commit_save", {
        targetPath: selectedPair.target_path,
        format: current.format,
        data: formData,
        secretPaths,
      });
      setLastChanges(res.changes);
      const backupName = res.backup_path ? basename(res.backup_path) : "无";
      setToast({ type: "success", text: `已保存（备份：${backupName}）` });
      setPreviewOpen(false);
      setPreview(null);
      await refreshAudit(scanDir);
    } catch (e) {
      setToast({ type: "error", text: `保存失败：${String(e)}` });
    } finally {
      setSaving(false);
    }
  }, [current, selectedPair, formData, secretPaths, refreshAudit, scanDir]);

  return (
    <div className="app">
      <header className="app-header">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true">
            {"{ }"}
          </span>
          <h1>通用配置器</h1>
        </div>
        <div className="folder-bar">
          <span className="folder-label">扫描目录</span>
          <code className="folder-path" title={scanDir}>
            {scanDir || "（未确定）"}
          </code>
          <button className="btn btn-secondary" onClick={chooseFolder}>
            选择文件夹
          </button>
        </div>
      </header>

      <div className="layout">
        <aside className="sidebar">
          <div className="sidebar-title">已发现的配置</div>
          {pairs.length === 0 ? (
            <p className="muted">未在此目录找到任何配置文件。</p>
          ) : (
            <ul className="pair-list">
              {pairs.map((p, idx) => {
                const orphan = p.cfgform_path === "" && !p.builtin;
                const warn = orphan || !p.target_exists || !!p.error;
                return (
                  <li key={`${p.target_path}-${idx}`}>
                    <button
                      className={`pair-item ${idx === selectedIdx ? "active" : ""}`}
                      onClick={() => onSelect(idx)}
                    >
                      <span className="pair-row">
                        <span className="pair-title">{p.title}</span>
                        {p.format && (
                          <span className="badge">{formatBadge(p.format)}</span>
                        )}
                      </span>
                      <span className="pair-file">{p.target_name}</span>
                      {p.builtin && (
                        <span className="chip chip-builtin">内置库</span>
                      )}
                      {warn && (
                        <span className="chip chip-warn">
                          {orphan
                            ? "缺少表单"
                            : !p.target_exists
                              ? "目标缺失"
                              : "解析异常"}
                        </span>
                      )}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </aside>

        <main className="content">
          {!selectedPair && (
            <div className="placeholder">请选择左侧的一个配置项进行编辑。</div>
          )}

          {selectedPair && isOrphan && (
            <div className="notice notice-warn">
              <h2>缺少表单说明文件</h2>
              <p>
                配置文件 <code>{selectedPair.target_name}</code>{" "}
                没有配套的表单说明文件（<code>.cfgform</code>）。
              </p>
              <p>
                本工具不会自行生成表单，请联系作者使用准备工具（prep-tool）生成后再编辑。
              </p>
            </div>
          )}

          {selectedPair && !isOrphan && loadError && (
            <div className="notice notice-error">
              <h2>无法加载此配置</h2>
              <p>{loadError}</p>
            </div>
          )}

          {selectedPair && !isOrphan && !loadError && current && (
            <>
              <div className="form-head">
                <h2>{current.title}</h2>
                <span className="badge badge-lg">
                  {formatBadge(current.format)}
                </span>
                <span className="form-target">
                  写回文件：{current.target_name}
                </span>
              </div>

              {isBuiltin && (
                <div className="builtin-banner">
                  <span className="builtin-badge">内置库</span>
                  <span>
                    使用内置 schema 库（项目未随附 <code>.cfgform</code>）。保存将正常写回{" "}
                    <code>{current.target_name}</code> 本身。
                  </span>
                  <button
                    type="button"
                    className="btn btn-secondary btn-mini"
                    onClick={onSaveBuiltinSidecar}
                    disabled={saving}
                  >
                    将内置模板另存为本目录的 .cfgform
                  </button>
                </div>
              )}

              {profiles && profiles.list && profiles.list.length > 0 && (
                <div className="profile-bar">
                  <span className="profile-label">环境</span>
                  <select
                    className="profile-select"
                    value={selectedProfile}
                    onChange={(e) => applyProfile(e.target.value)}
                  >
                    {profiles.list.map((name) => (
                      <option key={name} value={name}>
                        {name}
                      </option>
                    ))}
                  </select>
                  {!isBuiltin &&
                    selectedPair &&
                    selectedPair.cfgform_path !== "" && (
                      <button
                        type="button"
                        className="btn btn-secondary btn-mini profile-save"
                        onClick={onSaveProfileOverride}
                        disabled={saving}
                      >
                        将当前修改保存为【{selectedProfile}】环境的覆盖
                      </button>
                    )}
                </div>
              )}

              {friendlyErrors.length > 0 && (
                <div className="error-panel" role="alert">
                  <div className="error-panel-title">
                    发现 {friendlyErrors.length} 处需要修正的问题
                  </div>
                  <ul>
                    {friendlyErrors.map((fe, i) => (
                      <li key={i}>
                        <span className="error-field">{fe.field}</span>
                        <span className="error-reason">{fe.reason}</span>
                      </li>
                    ))}
                  </ul>
                </div>
              )}

              <div className="form-wrap">
                <Form
                  schema={current.schema}
                  uiSchema={uiSchema}
                  formData={formData}
                  validator={validator}
                  widgets={WIDGETS}
                  liveValidate
                  showErrorList={false}
                  onChange={onFormChange}
                  onError={(errs) => setErrors(errs)}
                >
                  <div className="form-actions">
                    <button
                      type="button"
                      className="btn btn-primary"
                      onClick={onSave}
                      disabled={saving || friendlyErrors.length > 0}
                    >
                      {saving ? "处理中…" : "保存…"}
                    </button>
                    {friendlyErrors.length > 0 && (
                      <span className="hint-error">
                        请先修正上方问题后再保存
                      </span>
                    )}
                  </div>
                </Form>
              </div>

              {changedDefaults.length > 0 && (
                <div className="defaults-panel">
                  <div className="defaults-title">
                    与默认值不同的字段（共 {changedDefaults.length} 项）
                  </div>
                  <ul>
                    {changedDefaults.map((d) => (
                      <li key={d.path}>
                        <span className="defaults-field">{d.path}</span>
                        <span className="defaults-meta">
                          当前 {JSON.stringify(d.cur)} ｜ 默认{" "}
                          {JSON.stringify(d.def)}
                        </span>
                        <button
                          type="button"
                          className="btn btn-secondary btn-mini"
                          onClick={() => resetField(d.path, d.def)}
                        >
                          重置为默认
                        </button>
                      </li>
                    ))}
                  </ul>
                </div>
              )}

              {lastChanges.length > 0 && (
                <div className="changes-panel">
                  <div className="changes-title">
                    本次变更（共 {lastChanges.length} 项）
                  </div>
                  <ul>
                    {lastChanges.map((c, i) => (
                      <li key={i}>{c}</li>
                    ))}
                  </ul>
                </div>
              )}
            </>
          )}

          <section className="audit">
            <button
              className="audit-toggle"
              onClick={() => setAuditOpen((v) => !v)}
              aria-expanded={auditOpen}
            >
              <span className={`caret ${auditOpen ? "open" : ""}`}>▶</span>
              操作记录
            </button>
            {auditOpen && (
              <pre className="audit-body">
                {auditText.trim() ? auditText : "暂无操作记录。"}
              </pre>
            )}
          </section>
        </main>
      </div>

      {previewOpen && preview && current && (
        <div className="modal-overlay" role="dialog" aria-modal="true">
          <div className="modal">
            <div className="modal-head">
              <h2>保存预览（Dry-run）</h2>
              <span className="modal-sub">
                目标：{current.target_name}（{formatBadge(current.format)}）
              </span>
            </div>

            <div className="modal-section-title">即将写入的文件内容</div>
            <pre className="preview-text">
              {maskText(preview.new_text, secretValues, revealSecrets)}
            </pre>

            <div className="modal-section-title">变更对比</div>
            <pre className="diff-text">
              {preview.diff_lines
                .map((l) => maskText(l, secretValues, revealSecrets))
                .join("\n")}
            </pre>

            {secretValues.length > 0 && (
              <label className="reveal-row">
                <input
                  type="checkbox"
                  checked={revealSecrets}
                  onChange={(e) => setRevealSecrets(e.target.checked)}
                />
                <span>
                  显示密文（⚠️ 警告：将明文展示密钥/密码，请确认周围无人窥屏）
                </span>
              </label>
            )}

            <div className="modal-actions">
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => {
                  setPreviewOpen(false);
                  setPreview(null);
                }}
                disabled={saving}
              >
                取消
              </button>
              <button
                type="button"
                className="btn btn-primary"
                onClick={onConfirmWrite}
                disabled={saving}
              >
                {saving ? "写入中…" : "确认写入"}
              </button>
            </div>
          </div>
        </div>
      )}

      {toast && (
        <div className={`toast toast-${toast.type}`} role="status">
          {toast.text}
        </div>
      )}
    </div>
  );
}

export default App;
