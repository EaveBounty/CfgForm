mod adapters;

use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const CFGFORM_EXT: &str = ".cfgform";
const JSONFORM_EXT: &str = ".jsonform";
const AUDIT_LOG: &str = "cfgform-audit.log";

const ORPHAN_EXTS: &[&str] = &[
    ".json", ".env", ".toml", ".yaml", ".yml", ".ini", ".conf", ".properties",
];

// ---------- built-in schema library (compile-time embedded) ----------
// 内置精选模板，路径相对 src-tauri/src 指向仓库 schemas/ 目录。
const BUILTIN_PACKAGE_JSON: &str = include_str!("../../../schemas/package.json.cfgform");
const BUILTIN_TSCONFIG: &str = include_str!("../../../schemas/tsconfig.json.cfgform");
const BUILTIN_COMPOSE: &str = include_str!("../../../schemas/docker-compose.yml.cfgform");

// 目标文件名 -> 内置 .cfgform 模板原文。
fn builtin_template(target_name: &str) -> Option<&'static str> {
    match target_name.to_ascii_lowercase().as_str() {
        "package.json" => Some(BUILTIN_PACKAGE_JSON),
        "tsconfig.json" => Some(BUILTIN_TSCONFIG),
        "docker-compose.yml" | "docker-compose.yaml" | "compose.yml" => Some(BUILTIN_COMPOSE),
        _ => None,
    }
}

#[derive(Serialize)]
struct PairInfo {
    cfgform_path: String,
    target_path: String,
    target_name: String,
    title: String,
    format: String,
    target_exists: bool,
    error: Option<String>,
    builtin: bool,
    source: String,
}

#[derive(Serialize)]
struct LoadResult {
    title: String,
    target_name: String,
    format: String,
    schema: Value,
    ui: Value,
    data: Value,
    profiles: Value,
}

#[derive(Serialize)]
struct PreviewResult {
    new_text: String,
    diff_lines: Vec<String>,
}

#[derive(Serialize)]
struct SaveResult {
    backup_path: String,
    changes: Vec<String>,
}

// ---------- sidecar interpretation ----------

struct Sidecar {
    target_name: String,
    title: String,
    format: String,
    schema: Value,
    ui: Value,
    profiles: Value,
}

fn detect_format(filename: &str, declared: Option<&str>) -> String {
    if let Some(f) = declared {
        if !f.is_empty() {
            return adapters::normalize_format(f).to_string();
        }
    }
    let lower = filename.to_ascii_lowercase();
    if lower == ".env" || lower.starts_with(".env.") || lower.ends_with(".env") {
        return "env".to_string();
    }
    if lower.ends_with(".toml") {
        return "toml".to_string();
    }
    if lower.ends_with(".ini") || lower.ends_with(".conf") || lower.ends_with(".properties") {
        return "ini".to_string();
    }
    if lower == "docker-compose.yml" || lower == "docker-compose.yaml" || lower == "compose.yml"
    {
        return "compose".to_string();
    }
    if lower.ends_with(".yaml") || lower.ends_with(".yml") {
        return "yaml".to_string();
    }
    "json".to_string()
}

fn interpret_sidecar(form_name: &str, v: &Value, legacy: bool) -> Sidecar {
    let base = if legacy {
        form_name.trim_end_matches(JSONFORM_EXT).to_string()
    } else {
        form_name.trim_end_matches(CFGFORM_EXT).to_string()
    };
    let default_target = if legacy {
        format!("{}.json", base)
    } else {
        base.clone()
    };
    let target_name = v
        .get("target")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .unwrap_or(default_target);
    let title = v
        .get("title")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| target_name.clone());
    let format = if legacy {
        "json".to_string()
    } else {
        let declared = v.get("format").and_then(|f| f.as_str());
        detect_format(&target_name, declared)
    };
    let schema = v
        .get("schema")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));
    let ui = v
        .get("ui")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));
    let profiles = v.get("profiles").cloned().unwrap_or(Value::Null);
    Sidecar {
        target_name,
        title,
        format,
        schema,
        ui,
        profiles,
    }
}

// ---------- commands ----------

#[tauri::command]
fn default_scan_dir() -> Result<String, String> {
    #[cfg(debug_assertions)]
    {
        if let Ok(cwd) = std::env::current_dir() {
            return Ok(cwd.to_string_lossy().to_string());
        }
    }
    let exe = std::env::current_exe().map_err(|e| format!("无法获取程序路径：{}", e))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "无法获取程序所在目录".to_string())?;
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
fn scan_dir(dir: String) -> Result<Vec<PairInfo>, String> {
    let dir_path = Path::new(&dir);
    let entries = fs::read_dir(dir_path).map_err(|e| format!("无法读取目录 {}：{}", dir, e))?;

    let mut pairs: Vec<PairInfo> = Vec::new();
    let mut targets_with_form: HashSet<String> = HashSet::new();
    let mut orphan_candidates: Vec<PathBuf> = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        let legacy = name.ends_with(JSONFORM_EXT);
        if name.ends_with(CFGFORM_EXT) || legacy {
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    pairs.push(PairInfo {
                        cfgform_path: path.to_string_lossy().to_string(),
                        target_path: String::new(),
                        target_name: name.clone(),
                        title: name.clone(),
                        format: String::new(),
                        target_exists: false,
                        error: Some(format!("无法读取表单文件：{}", e)),
                        builtin: false,
                        source: String::new(),
                    });
                    continue;
                }
            };
            match serde_json::from_str::<Value>(&content) {
                Ok(v) => {
                    let sc = interpret_sidecar(&name, &v, legacy);
                    let target_path = dir_path.join(&sc.target_name);
                    let target_exists = target_path.is_file();
                    targets_with_form.insert(sc.target_name.clone());
                    let error = if target_exists {
                        None
                    } else {
                        Some(format!("目标配置文件 {} 不存在", sc.target_name))
                    };
                    pairs.push(PairInfo {
                        cfgform_path: path.to_string_lossy().to_string(),
                        target_path: target_path.to_string_lossy().to_string(),
                        target_name: sc.target_name,
                        title: sc.title,
                        format: sc.format,
                        target_exists,
                        error,
                        builtin: false,
                        source: "边车".to_string(),
                    });
                }
                Err(e) => {
                    pairs.push(PairInfo {
                        cfgform_path: path.to_string_lossy().to_string(),
                        target_path: String::new(),
                        target_name: name.clone(),
                        title: name.clone(),
                        format: String::new(),
                        target_exists: false,
                        error: Some(format!("表单文件解析失败：{}", e)),
                        builtin: false,
                        source: String::new(),
                    });
                }
            }
        } else if name.ends_with(".bak") || name == AUDIT_LOG {
            continue;
        } else if ORPHAN_EXTS.iter().any(|ext| name.to_ascii_lowercase().ends_with(ext))
            || name.eq_ignore_ascii_case(".env")
        {
            orphan_candidates.push(path.clone());
        }
    }

    for op in orphan_candidates {
        let name = match op.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if targets_with_form.contains(&name) {
            continue;
        }
        // 无自带边车，但命中内置 schema 库 -> 作为内置配对呈现，可直接编辑。
        if let Some(tpl) = builtin_template(&name) {
            let (title, format) = match serde_json::from_str::<Value>(tpl) {
                Ok(v) => {
                    let title = v
                        .get("title")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| name.clone());
                    let declared = v.get("format").and_then(|f| f.as_str());
                    (title, detect_format(&name, declared))
                }
                Err(_) => (name.clone(), detect_format(&name, None)),
            };
            pairs.push(PairInfo {
                cfgform_path: String::new(),
                target_path: op.to_string_lossy().to_string(),
                target_name: name.clone(),
                title,
                format,
                target_exists: true,
                error: None,
                builtin: true,
                source: "内置库".to_string(),
            });
            continue;
        }
        pairs.push(PairInfo {
            cfgform_path: String::new(),
            target_path: op.to_string_lossy().to_string(),
            target_name: name.clone(),
            title: name.clone(),
            format: detect_format(&name, None),
            target_exists: true,
            error: Some("缺少表单说明文件，请联系作者用准备工具（prep-tool）生成".to_string()),
            builtin: false,
            source: String::new(),
        });
    }

    pairs.sort_by(|a, b| {
        (a.cfgform_path.is_empty(), &a.target_name)
            .cmp(&(b.cfgform_path.is_empty(), &b.target_name))
    });

    Ok(pairs)
}

#[tauri::command]
fn load_pair(cfgform_path: String) -> Result<LoadResult, String> {
    let cf_path = Path::new(&cfgform_path);
    let content = fs::read_to_string(cf_path).map_err(|e| format!("无法读取表单文件：{}", e))?;
    let v: Value =
        serde_json::from_str(&content).map_err(|e| format!("表单文件解析失败：{}", e))?;

    let dir = cf_path
        .parent()
        .ok_or_else(|| "无法定位表单文件所在目录".to_string())?;
    let form_name = cf_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let legacy = form_name.ends_with(JSONFORM_EXT);
    let sc = interpret_sidecar(form_name, &v, legacy);

    let target_path = dir.join(&sc.target_name);
    if !target_path.is_file() {
        return Err(format!("目标配置文件 {} 不存在", sc.target_name));
    }
    let target_content = fs::read_to_string(&target_path)
        .map_err(|e| format!("无法读取目标配置文件 {}：{}", sc.target_name, e))?;
    let data = adapters::parse(&sc.format, &target_content)
        .map_err(|e| format!("目标配置文件 {} {}", sc.target_name, e))?;

    Ok(LoadResult {
        title: sc.title,
        target_name: sc.target_name,
        format: sc.format,
        schema: sc.schema,
        ui: sc.ui,
        data,
        profiles: sc.profiles,
    })
}

#[tauri::command]
fn load_builtin(target_path: String) -> Result<LoadResult, String> {
    let tpath = Path::new(&target_path);
    let name = tpath
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "无效的目标文件名".to_string())?
        .to_string();
    let tpl = builtin_template(&name)
        .ok_or_else(|| format!("{} 不在内置 schema 库中", name))?;
    let v: Value = serde_json::from_str(tpl)
        .map_err(|e| format!("内置模板解析失败：{}", e))?;
    let sc = interpret_sidecar(&format!("{}{}", name, CFGFORM_EXT), &v, false);

    if !tpath.is_file() {
        return Err(format!("目标配置文件 {} 不存在", name));
    }
    let target_content = fs::read_to_string(tpath)
        .map_err(|e| format!("无法读取目标配置文件 {}：{}", name, e))?;
    let data = adapters::parse(&sc.format, &target_content)
        .map_err(|e| format!("目标配置文件 {} {}", name, e))?;

    Ok(LoadResult {
        title: sc.title,
        target_name: sc.target_name,
        format: sc.format,
        schema: sc.schema,
        ui: sc.ui,
        data,
        profiles: Value::Null,
    })
}

#[tauri::command]
fn preview_save(
    target_path: String,
    format: String,
    data: Value,
) -> Result<PreviewResult, String> {
    let tpath = Path::new(&target_path);
    let original = if tpath.is_file() {
        fs::read_to_string(tpath).map_err(|e| format!("无法读取原配置文件：{}", e))?
    } else {
        String::new()
    };
    let new_text = adapters::serialize(&format, &data, &original)?;
    let diff_lines = adapters::line_diff(&original, &new_text);
    Ok(PreviewResult {
        new_text,
        diff_lines,
    })
}

#[tauri::command]
fn commit_save(
    target_path: String,
    format: String,
    data: Value,
    secret_paths: Vec<String>,
) -> Result<SaveResult, String> {
    let tpath = Path::new(&target_path);
    let dir = tpath
        .parent()
        .ok_or_else(|| "无法定位目标文件所在目录".to_string())?;
    let file_name = tpath
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "无效的目标文件名".to_string())?
        .to_string();

    let original = if tpath.is_file() {
        fs::read_to_string(tpath).map_err(|e| format!("无法读取原配置文件：{}", e))?
    } else {
        String::new()
    };
    let old: Value = if original.trim().is_empty() {
        Value::Null
    } else {
        adapters::parse(&format, &original).map_err(|e| format!("原配置文件 {}", e))?
    };

    let secret_set: HashSet<String> = secret_paths.into_iter().collect();
    let mut raw_changes: Vec<Change> = Vec::new();
    diff_value("", &old, &data, &mut raw_changes);
    let changes = format_changes(&raw_changes, &secret_set);

    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let (backup_path_str, backup_name) = if tpath.is_file() {
        let backup_name = format!("{}.{}.bak", file_name, ts);
        let backup_path = dir.join(&backup_name);
        fs::copy(tpath, &backup_path).map_err(|e| format!("备份失败：{}", e))?;
        (backup_path.to_string_lossy().to_string(), backup_name)
    } else {
        (String::new(), String::new())
    };

    let new_text = adapters::serialize(&format, &data, &original)?;
    let tmp_path = dir.join(format!(".{}.tmp", file_name));
    fs::write(&tmp_path, new_text.as_bytes())
        .map_err(|e| format!("写入临时文件失败：{}", e))?;
    fs::rename(&tmp_path, tpath).map_err(|e| format!("替换目标文件失败：{}", e))?;

    let log_path = dir.join(AUDIT_LOG);
    let mut block = String::new();
    block.push_str(&format!(
        "==== {} 保存配置 ====\n",
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
    ));
    block.push_str(&format!("目标文件：{}（格式：{}）\n", file_name, format));
    if !backup_name.is_empty() {
        block.push_str(&format!("备份文件：{}\n", backup_name));
    }
    if changes.is_empty() {
        block.push_str("（无字段变更）\n");
    } else {
        for c in &changes {
            block.push_str(&format!("  {}\n", c));
        }
    }
    block.push('\n');
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("无法打开审计日志：{}", e))?;
    f.write_all(block.as_bytes())
        .map_err(|e| format!("写入审计日志失败：{}", e))?;

    Ok(SaveResult {
        backup_path: backup_path_str,
        changes,
    })
}

#[tauri::command]
fn read_audit_tail(dir: String, max_lines: usize) -> Result<String, String> {
    let log_path = Path::new(&dir).join(AUDIT_LOG);
    if !log_path.is_file() {
        return Ok(String::new());
    }
    let content =
        fs::read_to_string(&log_path).map_err(|e| format!("无法读取审计日志：{}", e))?;
    let lines: Vec<&str> = content.lines().collect();
    let start = if lines.len() > max_lines {
        lines.len() - max_lines
    } else {
        0
    };
    Ok(lines[start..].join("\n"))
}

fn append_audit(dir: &Path, block: &str) -> Result<(), String> {
    let log_path = dir.join(AUDIT_LOG);
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("无法打开审计日志：{}", e))?;
    f.write_all(block.as_bytes())
        .map_err(|e| format!("写入审计日志失败：{}", e))
}

// 将当前修改保存为某 profile 环境的覆盖，写回边车 profiles.overrides[active]。
// 非破坏：保留边车其它字段与 profiles.active/list 不变。
#[tauri::command]
fn save_profile_overrides(
    cfgform_path: String,
    active: String,
    overrides_value: Value,
) -> Result<(), String> {
    let cf_path = Path::new(&cfgform_path);
    if !cf_path.is_file() {
        return Err(format!("边车文件不存在：{}", cfgform_path));
    }
    let dir = cf_path
        .parent()
        .ok_or_else(|| "无法定位边车所在目录".to_string())?;
    let content = fs::read_to_string(cf_path).map_err(|e| format!("无法读取边车文件：{}", e))?;
    let mut root: Value =
        serde_json::from_str(&content).map_err(|e| format!("边车文件解析失败：{}", e))?;

    if !root.is_object() {
        return Err("边车文件根节点必须是 JSON 对象".to_string());
    }
    // 确保 profiles 对象存在
    if !root.get("profiles").map(|p| p.is_object()).unwrap_or(false) {
        root.as_object_mut()
            .unwrap()
            .insert("profiles".to_string(), Value::Object(Default::default()));
    }
    let profiles = root.get_mut("profiles").unwrap().as_object_mut().unwrap();
    // 确保 overrides 对象存在
    if !profiles
        .get("overrides")
        .map(|p| p.is_object())
        .unwrap_or(false)
    {
        profiles.insert("overrides".to_string(), Value::Object(Default::default()));
    }
    let overrides = profiles
        .get_mut("overrides")
        .unwrap()
        .as_object_mut()
        .unwrap();
    overrides.insert(active.clone(), overrides_value);

    let mut out = serde_json::to_string_pretty(&root)
        .map_err(|e| format!("边车序列化失败：{}", e))?;
    out = out.replace("\r\n", "\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    // 原子写入边车文件
    let file_name = cf_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "无效的边车文件名".to_string())?;
    let tmp_path = dir.join(format!(".{}.tmp", file_name));
    fs::write(&tmp_path, out.as_bytes()).map_err(|e| format!("写入临时文件失败：{}", e))?;
    fs::rename(&tmp_path, cf_path).map_err(|e| format!("替换边车文件失败：{}", e))?;

    let block = format!(
        "==== {} 更新边车 profiles.overrides[{}] ====\n边车文件：{}\n\n",
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
        active,
        file_name
    );
    append_audit(dir, &block)?;
    Ok(())
}

// 可选便捷操作：将内置模板另存为目标文件旁的 .cfgform 边车。
#[tauri::command]
fn save_builtin_sidecar(target_path: String) -> Result<String, String> {
    let tpath = Path::new(&target_path);
    let dir = tpath
        .parent()
        .ok_or_else(|| "无法定位目标文件所在目录".to_string())?;
    let name = tpath
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "无效的目标文件名".to_string())?
        .to_string();
    let tpl = builtin_template(&name)
        .ok_or_else(|| format!("{} 不在内置 schema 库中", name))?;
    let sidecar_name = format!("{}{}", name, CFGFORM_EXT);
    let sidecar_path = dir.join(&sidecar_name);
    if sidecar_path.exists() {
        return Err(format!("{} 已存在，未覆盖", sidecar_name));
    }
    let mut out = tpl.replace("\r\n", "\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    fs::write(&sidecar_path, out.as_bytes()).map_err(|e| format!("写入边车失败：{}", e))?;
    let block = format!(
        "==== {} 写出内置边车 ====\n来源：内置 schema 库\n边车文件：{}\n\n",
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
        sidecar_name
    );
    append_audit(dir, &block)?;
    Ok(sidecar_path.to_string_lossy().to_string())
}

// ---------- structured diff ----------

enum ChangeKind {
    Changed,
    Added,
    Removed,
}

struct Change {
    path: String,
    kind: ChangeKind,
    old: Value,
    new: Value,
}

fn fmt_val(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "（空）".to_string(),
        _ => v.to_string(),
    }
}

fn join_path(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_string()
    } else {
        format!("{}.{}", parent, key)
    }
}

fn diff_value(path: &str, old: &Value, new: &Value, out: &mut Vec<Change>) {
    match (old, new) {
        (Value::Object(o), Value::Object(n)) => {
            let mut keys: Vec<&String> = o.keys().chain(n.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let child = join_path(path, k);
                match (o.get(k), n.get(k)) {
                    (Some(ov), Some(nv)) => diff_value(&child, ov, nv, out),
                    (Some(ov), None) => out.push(Change {
                        path: child,
                        kind: ChangeKind::Removed,
                        old: ov.clone(),
                        new: Value::Null,
                    }),
                    (None, Some(nv)) => out.push(Change {
                        path: child,
                        kind: ChangeKind::Added,
                        old: Value::Null,
                        new: nv.clone(),
                    }),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(o), Value::Array(n)) => {
            let max = o.len().max(n.len());
            for i in 0..max {
                let child = format!("{}[{}]", path, i);
                match (o.get(i), n.get(i)) {
                    (Some(ov), Some(nv)) => diff_value(&child, ov, nv, out),
                    (Some(ov), None) => out.push(Change {
                        path: child,
                        kind: ChangeKind::Removed,
                        old: ov.clone(),
                        new: Value::Null,
                    }),
                    (None, Some(nv)) => out.push(Change {
                        path: child,
                        kind: ChangeKind::Added,
                        old: Value::Null,
                        new: nv.clone(),
                    }),
                    (None, None) => {}
                }
            }
        }
        _ => {
            if old != new {
                out.push(Change {
                    path: path.to_string(),
                    kind: ChangeKind::Changed,
                    old: old.clone(),
                    new: new.clone(),
                });
            }
        }
    }
}

fn format_changes(changes: &[Change], secret_set: &HashSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    for c in changes {
        let label_path = if c.path.is_empty() {
            "（整体）".to_string()
        } else {
            c.path.clone()
        };
        if secret_set.contains(&c.path) {
            out.push(format!("字段 {}：已修改（密文，不记录值）", label_path));
            continue;
        }
        match c.kind {
            ChangeKind::Changed => out.push(format!(
                "字段 {}：{} -> {}",
                label_path,
                fmt_val(&c.old),
                fmt_val(&c.new)
            )),
            ChangeKind::Added => {
                out.push(format!("新增 {}：{}", label_path, fmt_val(&c.new)))
            }
            ChangeKind::Removed => {
                out.push(format!("删除 {}：{}", label_path, fmt_val(&c.old)))
            }
        }
    }
    out
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            default_scan_dir,
            scan_dir,
            load_pair,
            load_builtin,
            preview_save,
            commit_save,
            read_audit_tail,
            save_profile_overrides,
            save_builtin_sidecar
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
