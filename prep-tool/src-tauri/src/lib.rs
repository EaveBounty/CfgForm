use chrono::SecondsFormat;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use tauri::Manager;

// ===== 数据结构 =====

#[derive(Serialize)]
struct StackInfo {
    stack: String,
    markers: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct SourceFile {
    path: String,
    content: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct SourcesBundle {
    readme: String,
    readme_path: String,
    files: Vec<SourceFile>,
}

#[derive(Serialize)]
struct LlmConfigStatus {
    base_url: String,
    model: String,
    key_source: String,
    has_key: bool,
}

#[derive(Deserialize)]
struct GenArgs {
    #[allow(dead_code)]
    dir: String,
    config_path: String,
    format: String,
    stack: String,
    base_schema: Value,
    sources: SourcesBundle,
    base_url: String,
    model: String,
    api_key: Option<String>,
    title: String,
}

#[derive(Serialize)]
struct GenResult {
    cfgform: Value,
    llm_raw_excerpt: String,
}

#[derive(Serialize)]
struct WriteResult {
    cfgform_path: String,
    log_path: String,
    actions: Vec<String>,
}

// ===== 工具函数 =====

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{}…(已截断)", head)
    }
}

fn strip_bom(s: &str) -> &str {
    s.trim_start_matches('\u{feff}')
}

fn read_dotenv(exe_dir: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let p = exe_dir.join(".env");
    if let Ok(content) = fs::read_to_string(&p) {
        for raw in content.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(idx) = line.find('=') {
                let k = line[..idx].trim().to_string();
                let mut v = line[idx + 1..].trim().to_string();
                if v.len() >= 2
                    && ((v.starts_with('"') && v.ends_with('"'))
                        || (v.starts_with('\'') && v.ends_with('\'')))
                {
                    v = v[1..v.len() - 1].to_string();
                }
                if !k.is_empty() {
                    map.insert(k, v);
                }
            }
        }
    }
    map
}

fn current_exe_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_path_buf()))
}

/// 同时兼容新旧两套环境变量名：优先 CFGFORM_LLM_*，回退 JSONFORM_LLM_*。
fn env_pair(suffix: &str) -> Option<String> {
    for prefix in ["CFGFORM_LLM_", "JSONFORM_LLM_"] {
        if let Ok(v) = std::env::var(format!("{}{}", prefix, suffix)) {
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

fn dotenv_pair(dotenv: &HashMap<String, String>, suffix: &str) -> Option<String> {
    for prefix in ["CFGFORM_LLM_", "JSONFORM_LLM_"] {
        if let Some(v) = dotenv.get(&format!("{}{}", prefix, suffix)) {
            if !v.is_empty() {
                return Some(v.clone());
            }
        }
    }
    None
}

/// 按优先级解析真实密钥：界面临时 > 环境变量 > .env。仅在调用 LLM 时内部使用，不返回给前端。
fn resolve_key(ui_key: &Option<String>) -> Option<(String, &'static str)> {
    if let Some(k) = ui_key {
        if !k.trim().is_empty() {
            return Some((k.trim().to_string(), "ui"));
        }
    }
    if let Some(v) = env_pair("API_KEY") {
        return Some((v, "env"));
    }
    if let Some(dir) = current_exe_dir() {
        let dotenv = read_dotenv(&dir);
        if let Some(v) = dotenv_pair(&dotenv, "API_KEY") {
            return Some((v, "dotenv"));
        }
    }
    None
}

/// 防御性地从 LLM 文本输出中提取 JSON 对象（容忍 ```json 围栏与前后多余文字）。
fn extract_json_object(s: &str) -> Option<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(s.trim()) {
        if v.is_object() {
            return Some(v);
        }
    }
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end > start {
        let sub = &s[start..=end];
        if let Ok(v) = serde_json::from_str::<Value>(sub) {
            if v.is_object() {
                return Some(v);
            }
        }
    }
    None
}

// ===== 格式适配器：文本 → 规范数据树（serde_json::Value） =====
//
// prep-tool 仅需 PARSE 用于构建基线数据树；用户侧 configurator 负责无损 serialize 回写。

fn unquote(v: &str) -> String {
    let v = v.trim();
    if v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"'))
            || (v.starts_with('\'') && v.ends_with('\'')))
    {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

/// .env / KV 行级解析：保留键顺序无关，统一产出字符串值对象。
fn parse_env_text(text: &str) -> Value {
    let mut map = serde_json::Map::new();
    for raw in text.lines() {
        let mut line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("export ") {
            line = rest.trim();
        }
        if let Some(idx) = line.find('=') {
            let k = line[..idx].trim().to_string();
            let v = unquote(&line[idx + 1..]);
            if !k.is_empty() {
                map.insert(k, Value::String(v));
            }
        }
    }
    Value::Object(map)
}

/// .ini / .properties / .conf 解析：section + KV，无 section 的键归并到顶层。
fn parse_ini_text(text: &str) -> Value {
    let mut root = serde_json::Map::new();
    let mut current: Option<String> = None;
    let mut sections: HashMap<String, serde_json::Map<String, Value>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let name = line[1..line.len() - 1].trim().to_string();
            if !sections.contains_key(&name) {
                sections.insert(name.clone(), serde_json::Map::new());
                order.push(name.clone());
            }
            current = Some(name);
            continue;
        }
        let sep = line.find('=').or_else(|| line.find(':'));
        if let Some(idx) = sep {
            let k = line[..idx].trim().to_string();
            let v = unquote(&line[idx + 1..]);
            if k.is_empty() {
                continue;
            }
            match &current {
                Some(sec) => {
                    sections.get_mut(sec).unwrap().insert(k, Value::String(v));
                }
                None => {
                    root.insert(k, Value::String(v));
                }
            }
        }
    }
    for name in order {
        if let Some(m) = sections.remove(&name) {
            root.insert(name, Value::Object(m));
        }
    }
    Value::Object(root)
}

fn toml_value_to_json(v: &toml_edit::Value) -> Value {
    match v {
        toml_edit::Value::String(s) => Value::String(s.value().clone()),
        toml_edit::Value::Integer(i) => json!(*i.value()),
        toml_edit::Value::Float(f) => json!(*f.value()),
        toml_edit::Value::Boolean(b) => json!(*b.value()),
        toml_edit::Value::Datetime(d) => Value::String(d.value().to_string()),
        toml_edit::Value::Array(a) => {
            Value::Array(a.iter().map(toml_value_to_json).collect())
        }
        toml_edit::Value::InlineTable(t) => {
            let mut m = serde_json::Map::new();
            for (k, val) in t.iter() {
                m.insert(k.to_string(), toml_value_to_json(val));
            }
            Value::Object(m)
        }
    }
}

fn toml_item_to_json(item: &toml_edit::Item) -> Value {
    match item {
        toml_edit::Item::None => Value::Null,
        toml_edit::Item::Value(v) => toml_value_to_json(v),
        toml_edit::Item::Table(t) => {
            let mut m = serde_json::Map::new();
            for (k, v) in t.iter() {
                m.insert(k.to_string(), toml_item_to_json(v));
            }
            Value::Object(m)
        }
        toml_edit::Item::ArrayOfTables(a) => {
            let mut arr = Vec::new();
            for t in a.iter() {
                let mut m = serde_json::Map::new();
                for (k, v) in t.iter() {
                    m.insert(k.to_string(), toml_item_to_json(v));
                }
                arr.push(Value::Object(m));
            }
            Value::Array(arr)
        }
    }
}

fn parse_toml_text(text: &str) -> Result<Value, String> {
    let doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("TOML 解析失败：{}", e))?;
    let mut m = serde_json::Map::new();
    for (k, v) in doc.as_table().iter() {
        m.insert(k.to_string(), toml_item_to_json(v));
    }
    Ok(Value::Object(m))
}

fn parse_yaml_text(text: &str) -> Result<Value, String> {
    serde_yaml::from_str::<Value>(text).map_err(|e| format!("YAML 解析失败：{}", e))
}

/// 统一适配器入口：format → 规范数据树。compose 复用 yaml。
fn parse_config(format: &str, text: &str) -> Result<Value, String> {
    let text = strip_bom(text);
    match format {
        "json" => serde_json::from_str::<Value>(text)
            .map_err(|e| format!("JSON 解析失败：{}", e)),
        "env" => Ok(parse_env_text(text)),
        "toml" => parse_toml_text(text),
        "yaml" | "compose" => parse_yaml_text(text),
        "ini" => Ok(parse_ini_text(text)),
        other => Err(format!("不支持的格式：{}", other)),
    }
}

/// 由文件名/扩展名判定格式；无法判定时再嗅探内容；仍不确定则回退 json。
fn detect_format_impl(path: &str, content: &str) -> String {
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let lower = name.to_lowercase();

    // compose 优先（其扩展名通常是 .yml/.yaml，但语义为 compose）
    if lower.starts_with("docker-compose") || lower.starts_with("compose.") || lower == "compose.yml"
        || lower == "compose.yaml"
    {
        return "compose".to_string();
    }
    // .env / 以 .env 开头（.env.local 等）
    if lower == ".env" || lower.starts_with(".env") {
        return "env".to_string();
    }
    let ext = Path::new(&lower)
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();
    match ext.as_str() {
        "json" => return "json".to_string(),
        "toml" => return "toml".to_string(),
        "yml" | "yaml" => return "yaml".to_string(),
        "env" => return "env".to_string(),
        "ini" | "properties" | "conf" => return "ini".to_string(),
        _ => {}
    }

    // 内容嗅探（尽力而为）
    let body = strip_bom(content).trim_start();
    if body.starts_with('{') || body.starts_with('[') {
        return "json".to_string();
    }
    if body.starts_with("---") {
        return "yaml".to_string();
    }
    let has_section = body
        .lines()
        .any(|l| {
            let t = l.trim();
            t.starts_with('[') && t.ends_with(']') && t.len() > 2
        });
    if has_section {
        return "ini".to_string();
    }
    if body.lines().any(|l| {
        let t = l.trim();
        !t.is_empty() && !t.starts_with('#') && t.contains('=')
    }) {
        return "env".to_string();
    }

    "json".to_string()
}

// ===== 密钥启发式 =====

fn key_is_secret(key: &str) -> bool {
    let k = key.to_lowercase();
    // 对应正则 /key|token|secret|password|passwd|pwd|dsn|credential|api[-_]?key|private/i
    // ("key" 已覆盖 apikey/api_key/api-key)
    const NEEDLES: [&str; 8] = [
        "key", "token", "secret", "password", "passwd", "pwd", "dsn", "credential",
    ];
    if k.contains("private") {
        return true;
    }
    NEEDLES.iter().any(|n| k.contains(n))
}

fn walk_secrets(data: &Value, prefix: &str, out: &mut Vec<String>) {
    if let Value::Object(map) = data {
        for (k, v) in map {
            let path = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{}.{}", prefix, k)
            };
            if key_is_secret(k) {
                out.push(path.clone());
            }
            walk_secrets(v, &path, out);
        }
    }
    // 数组内的对象键较少出现密钥，按需扩展；此处保持 ui 路径映射简单可靠。
}

#[tauri::command]
fn suggest_secrets(data: Value) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    walk_secrets(&data, "", &mut out);
    out.sort();
    out.dedup();
    Ok(out)
}

/// 把点路径 a.b.c 写入 uiSchema，并在叶子节点设置 ui:secret=true。
fn set_ui_secret(ui: &mut Value, path: &str) {
    if !ui.is_object() {
        *ui = json!({});
    }
    let parts: Vec<&str> = path.split('.').collect();
    let mut node = ui;
    for (i, part) in parts.iter().enumerate() {
        let obj = node.as_object_mut().unwrap();
        let entry = obj.entry((*part).to_string()).or_insert_with(|| json!({}));
        if !entry.is_object() {
            *entry = json!({});
        }
        if i == parts.len() - 1 {
            entry
                .as_object_mut()
                .unwrap()
                .insert("ui:secret".to_string(), Value::Bool(true));
        }
        node = entry;
    }
}

// ===== 命令 =====

#[tauri::command]
fn exe_dir() -> Result<String, String> {
    let dir = current_exe_dir().ok_or_else(|| "无法获取可执行文件所在目录".to_string())?;
    Ok(dir.to_string_lossy().to_string())
}

// ===== LLM 配置持久化（非密钥项存 settings.json；密钥仅可选写 .env） =====

#[derive(Serialize, Deserialize, Default)]
struct SavedSettings {
    base_url: String,
    model: String,
}

fn settings_file(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("无法获取应用配置目录：{}", e))?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败：{}", e))?;
    }
    Ok(dir.join("settings.json"))
}

/// 读取已保存的非密钥设置用于启动回填；缺失时回退环境变量/.env/默认值。永不含密钥。
#[tauri::command]
fn load_settings(app: tauri::AppHandle) -> Result<SavedSettings, String> {
    let mut s = SavedSettings::default();
    if let Ok(p) = settings_file(&app) {
        if let Ok(content) = fs::read_to_string(&p) {
            if let Ok(parsed) = serde_json::from_str::<SavedSettings>(&content) {
                s = parsed;
            }
        }
    }
    let dotenv = current_exe_dir().map(|d| read_dotenv(&d)).unwrap_or_default();
    if s.base_url.trim().is_empty() {
        s.base_url = env_pair("BASE_URL")
            .or_else(|| dotenv_pair(&dotenv, "BASE_URL"))
            .unwrap_or_else(|| "https://api.deepseek.com".to_string());
    }
    if s.model.trim().is_empty() {
        s.model = env_pair("MODEL")
            .or_else(|| dotenv_pair(&dotenv, "MODEL"))
            .unwrap_or_else(|| "deepseek-chat".to_string());
    }
    Ok(s)
}

/// 仅保存非密钥项（Base URL / Model）到 settings.json，绝不写入密钥。
#[tauri::command]
fn save_settings(app: tauri::AppHandle, base_url: String, model: String) -> Result<(), String> {
    let p = settings_file(&app)?;
    let s = SavedSettings {
        base_url: base_url.trim().to_string(),
        model: model.trim().to_string(),
    };
    let txt = serde_json::to_string_pretty(&s).map_err(|e| format!("序列化设置失败：{}", e))?;
    fs::write(&p, format!("{}\n", txt)).map_err(|e| format!("写入设置失败：{}", e))?;
    Ok(())
}

/// 在 .env 中 upsert 指定键，保留其它行。
fn upsert_dotenv_lines(dir: &Path, updates: &[(&str, &str)]) -> Result<std::path::PathBuf, String> {
    let p = dir.join(".env");
    let existing = fs::read_to_string(&p).unwrap_or_default();
    let mut lines: Vec<String> = existing.lines().map(|l| l.to_string()).collect();
    for (key, val) in updates {
        let prefix = format!("{}=", key);
        let newline = format!("{}={}", key, val);
        if let Some(slot) = lines.iter_mut().find(|l| l.trim_start().starts_with(&prefix)) {
            *slot = newline;
        } else {
            lines.push(newline);
        }
    }
    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    fs::write(&p, out).map_err(|e| format!("写入 .env 失败：{}", e))?;
    Ok(p)
}

/// 显式选择：把密钥（及当前 Base URL/Model）以明文写入 exe 目录 .env，长期保存、免重输。
#[tauri::command]
fn save_key_to_dotenv(api_key: String, base_url: String, model: String) -> Result<String, String> {
    let dir = current_exe_dir().ok_or_else(|| "无法获取可执行文件所在目录".to_string())?;
    let key = api_key.trim();
    if key.is_empty() {
        return Err("密钥为空，无需保存。".to_string());
    }
    let burl = base_url.trim();
    let mdl = model.trim();
    let mut updates: Vec<(&str, &str)> = vec![("CFGFORM_LLM_API_KEY", key)];
    if !burl.is_empty() {
        updates.push(("CFGFORM_LLM_BASE_URL", burl));
    }
    if !mdl.is_empty() {
        updates.push(("CFGFORM_LLM_MODEL", mdl));
    }
    let p = upsert_dotenv_lines(&dir, &updates)?;
    Ok(p.to_string_lossy().to_string())
}

/// 从 exe 目录 .env 移除已保存的密钥行（保留 Base URL/Model）。
#[tauri::command]
fn clear_dotenv_key() -> Result<(), String> {
    let dir = current_exe_dir().ok_or_else(|| "无法获取可执行文件所在目录".to_string())?;
    let p = dir.join(".env");
    if let Ok(content) = fs::read_to_string(&p) {
        let kept: Vec<&str> = content
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("CFGFORM_LLM_API_KEY=") && !t.starts_with("JSONFORM_LLM_API_KEY=")
            })
            .collect();
        let mut out = kept.join("\n");
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        fs::write(&p, out).map_err(|e| format!("更新 .env 失败：{}", e))?;
    }
    Ok(())
}

#[tauri::command]
fn detect_format(path: String, content: String) -> Result<String, String> {
    Ok(detect_format_impl(&path, &content))
}

#[tauri::command]
fn read_target_as_value(path: String, format: String) -> Result<Value, String> {
    let content =
        fs::read_to_string(&path).map_err(|e| format!("读取文件失败：{} （{}）", path, e))?;
    parse_config(&format, &content)
}

#[tauri::command]
fn detect_stack(dir: String) -> Result<StackInfo, String> {
    let base = Path::new(&dir);
    if !base.is_dir() {
        return Err(format!("目录不存在或不可访问：{}", dir));
    }

    let exists = |name: &str| base.join(name).exists();
    let has_ts_files = || -> bool {
        if let Ok(rd) = fs::read_dir(base) {
            for e in rd.flatten() {
                if let Some(ext) = e.path().extension() {
                    if ext == "ts" {
                        return true;
                    }
                }
            }
        }
        false
    };

    let mut markers: Vec<String> = Vec::new();
    for m in [
        "tsconfig.json",
        "package.json",
        "pyproject.toml",
        "requirements.txt",
        "go.mod",
        "Cargo.toml",
    ] {
        if exists(m) {
            markers.push(m.to_string());
        }
    }
    let has_ts = has_ts_files();
    if has_ts {
        markers.push("*.ts".to_string());
    }

    // Python 额外探测 pydantic / BaseModel 留痕
    let mut pydantic = false;
    for f in ["requirements.txt", "pyproject.toml"] {
        let p = base.join(f);
        if let Ok(c) = fs::read_to_string(&p) {
            let lc = c.to_lowercase();
            if lc.contains("pydantic") || c.contains("BaseModel") {
                pydantic = true;
            }
        }
    }
    if pydantic {
        markers.push("pydantic".to_string());
    }

    let stack = if exists("package.json") || exists("tsconfig.json") || has_ts {
        "node"
    } else if exists("pyproject.toml") || exists("requirements.txt") {
        "python"
    } else if exists("go.mod") {
        "go"
    } else if exists("Cargo.toml") {
        "rust"
    } else {
        "generic"
    };

    Ok(StackInfo {
        stack: stack.to_string(),
        markers,
    })
}

fn infer_node(v: &Value) -> Value {
    match v {
        Value::Null => json!({ "type": "null" }),
        Value::Bool(_) => json!({ "type": "boolean" }),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                json!({ "type": "integer" })
            } else {
                json!({ "type": "number" })
            }
        }
        Value::String(_) => json!({ "type": "string" }),
        Value::Array(arr) => {
            let mut node = json!({ "type": "array" });
            if let Some(first) = arr.first() {
                node["items"] = infer_node(first);
            }
            node
        }
        Value::Object(map) => {
            let mut props = serde_json::Map::new();
            let mut required: Vec<Value> = Vec::new();
            for (k, val) in map {
                props.insert(k.clone(), infer_node(val));
                required.push(Value::String(k.clone()));
            }
            let mut node = json!({ "type": "object" });
            node["properties"] = Value::Object(props);
            if !required.is_empty() {
                node["required"] = Value::Array(required);
            }
            node
        }
    }
}

#[tauri::command]
fn infer_schema(data: Value) -> Result<Value, String> {
    let mut schema = infer_node(&data);
    if let Value::Object(ref mut m) = schema {
        m.insert(
            "$schema".to_string(),
            Value::String("http://json-schema.org/draft-07/schema#".to_string()),
        );
    } else {
        // 顶层不是对象时也包一层，确保是合法 draft-07 文档
        schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": schema.get("type").cloned().unwrap_or(json!("string"))
        });
    }
    Ok(schema)
}

fn collect_sources(base: &Path, dir: &Path, depth: usize, out: &mut Vec<SourceFile>) {
    if out.len() >= 5 || depth > 3 {
        return;
    }
    let exts = ["ts", "py", "go", "rs", "json"];
    let keywords = ["config", "settings", "schema"];
    let skip_dirs = [
        "node_modules",
        ".git",
        "target",
        "dist",
        "build",
        ".venv",
        "__pycache__",
    ];

    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());

    // 先文件后目录
    for e in &entries {
        if out.len() >= 5 {
            return;
        }
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_lowercase();
        let ext_ok = p
            .extension()
            .map(|x| exts.contains(&x.to_string_lossy().as_ref()))
            .unwrap_or(false);
        let kw_ok = keywords.iter().any(|k| name.contains(k));
        if ext_ok && kw_ok {
            if let Ok(c) = fs::read_to_string(&p) {
                let rel = p
                    .strip_prefix(base)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push(SourceFile {
                    path: rel,
                    content: truncate_chars(&c, 3000),
                });
            }
        }
    }
    for e in &entries {
        if out.len() >= 5 {
            return;
        }
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let dname = e.file_name().to_string_lossy().to_string();
        if skip_dirs.contains(&dname.as_str()) {
            continue;
        }
        collect_sources(base, &p, depth + 1, out);
    }
}

#[tauri::command]
fn gather_sources(dir: String) -> Result<SourcesBundle, String> {
    let base = Path::new(&dir);
    if !base.is_dir() {
        return Err(format!("目录不存在或不可访问：{}", dir));
    }

    let mut readme = String::new();
    let mut readme_path = String::new();
    for name in ["README.md", "README", "readme.md", "Readme.md", "README.txt"] {
        let p = base.join(name);
        if p.is_file() {
            if let Ok(c) = fs::read_to_string(&p) {
                readme = truncate_chars(&c, 6000);
                readme_path = name.to_string();
                break;
            }
        }
    }

    let mut files: Vec<SourceFile> = Vec::new();
    collect_sources(base, base, 0, &mut files);
    files.truncate(5);

    Ok(SourcesBundle {
        readme,
        readme_path,
        files,
    })
}

#[tauri::command]
fn resolve_llm_config(
    ui_base_url: String,
    ui_model: String,
    ui_has_key: bool,
) -> Result<LlmConfigStatus, String> {
    let dotenv = current_exe_dir().map(|d| read_dotenv(&d)).unwrap_or_default();

    let base_url = if !ui_base_url.trim().is_empty() {
        ui_base_url.trim().to_string()
    } else if let Some(v) = env_pair("BASE_URL") {
        v
    } else if let Some(v) = dotenv_pair(&dotenv, "BASE_URL") {
        v
    } else {
        "https://api.deepseek.com".to_string()
    };

    let model = if !ui_model.trim().is_empty() {
        ui_model.trim().to_string()
    } else if let Some(v) = env_pair("MODEL") {
        v
    } else if let Some(v) = dotenv_pair(&dotenv, "MODEL") {
        v
    } else {
        String::new()
    };

    let env_key = env_pair("API_KEY").is_some();
    let dotenv_key = dotenv_pair(&dotenv, "API_KEY").is_some();

    let (key_source, has_key) = if ui_has_key {
        ("ui", true)
    } else if env_key {
        ("env", true)
    } else if dotenv_key {
        ("dotenv", true)
    } else {
        ("none", false)
    };

    Ok(LlmConfigStatus {
        base_url,
        model,
        key_source: key_source.to_string(),
        has_key,
    })
}

#[tauri::command]
async fn generate_metadata(args: GenArgs) -> Result<GenResult, String> {
    let key = match resolve_key(&args.api_key) {
        Some((k, _)) => k,
        None => {
            return Err("未配置 LLM 密钥：请在界面输入临时密钥，或设置环境变量 CFGFORM_LLM_API_KEY（兼容 JSONFORM_LLM_API_KEY），或在程序目录放置 .env 文件后重试。".to_string());
        }
    };

    let base = args.base_url.trim().trim_end_matches('/').to_string();
    if base.is_empty() {
        return Err("Base URL 不能为空，请填写 OpenAI 兼容接口地址（推荐 DeepSeek：https://api.deepseek.com）。".to_string());
    }
    let model = if args.model.trim().is_empty() {
        "deepseek-chat".to_string()
    } else {
        args.model.trim().to_string()
    };
    let url = format!("{}/chat/completions", base);

    let target_name = Path::new(&args.config_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "config.json".to_string());
    let format = if args.format.trim().is_empty() {
        "json".to_string()
    } else {
        args.format.trim().to_string()
    };
    let title = if args.title.trim().is_empty() {
        target_name.clone()
    } else {
        args.title.trim().to_string()
    };

    // 拼装源码上下文
    let mut src_ctx = String::new();
    if !args.sources.readme.is_empty() {
        src_ctx.push_str(&format!(
            "### README（{}）\n{}\n\n",
            if args.sources.readme_path.is_empty() {
                "README"
            } else {
                &args.sources.readme_path
            },
            args.sources.readme
        ));
    }
    for f in &args.sources.files {
        src_ctx.push_str(&format!("### 源文件 {}\n{}\n\n", f.path, f.content));
    }
    if src_ctx.is_empty() {
        src_ctx.push_str("（未找到 README 或相关源文件）\n");
    }

    let base_schema_str =
        serde_json::to_string_pretty(&args.base_schema).unwrap_or_else(|_| "{}".to_string());

    let system = "你是一名资深配置专家，负责为各种格式（json/env/toml/yaml/ini/compose）的配置文件生成与格式无关的可视化表单元数据（基于解析后的规范数据树）。\
你将得到：目标文件格式、技术栈、由配置值推断出的基线 JSON Schema、以及项目的 README 与关键源文件。\
你必须严格只返回一个 JSON 对象，形如 {\"schema\":{...},\"ui\":{...}}，不要包含任何解释或 Markdown 围栏。\
schema 必须是合法的 JSON Schema draft-07，结构需与基线/原配置保持一致；在可推断时补充 description（中文）、enum、minimum、maximum、pattern、format，并在能推断字段间约束时给出条件校验 if/then/else（例如 mode=prod 时某字段必填）。\
ui 必须是 RJSF uiSchema：可用 ui:widget、ui:placeholder、ui:help（中文）、ui:enumNames（与 schema.enum 一一对应的中文业务含义）、ui:order；对疑似密钥字段（key/token/secret/password/dsn/credential/private 等）设置 \"ui:secret\": true；对作者明显不应暴露给最终用户修改的字段（如内部版本号）设置 \"ui:readOnly\": true。";

    let user = format!(
        "目标文件格式：{}\n技术栈：{}\n目标配置文件名：{}\n\n## 基线 JSON Schema（由配置值推断，类型可信，结构请保持一致）\n{}\n\n## 项目上下文\n{}\n请基于以上信息返回严格 JSON：{{\"schema\":{{...}},\"ui\":{{...}}}}。",
        format, args.stack, target_name, base_schema_str, src_ctx
    );

    let body = json!({
        "model": model,
        "temperature": 0.2,
        "response_format": { "type": "json_object" },
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(&key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("调用 LLM 失败（网络/连接错误）：{}", e))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取 LLM 响应失败：{}", e))?;
    if !status.is_success() {
        return Err(format!(
            "LLM 服务返回错误（HTTP {}）：{}",
            status.as_u16(),
            truncate_chars(&text, 300)
        ));
    }

    let parsed: Value = serde_json::from_str(&text)
        .map_err(|e| format!("LLM 响应不是合法 JSON（可能不是 OpenAI 兼容接口）：{}", e))?;
    let content = parsed["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "LLM 响应缺少 choices[0].message.content 字段。".to_string())?;
    let excerpt = truncate_chars(content, 500);

    // 防御性解析：失败则回退基线 schema + 空 ui，保证流程不中断。
    let model_json = extract_json_object(content).unwrap_or_else(|| json!({}));
    let schema = model_json
        .get("schema")
        .cloned()
        .filter(|v| v.is_object())
        .unwrap_or_else(|| args.base_schema.clone());
    let mut ui = model_json
        .get("ui")
        .cloned()
        .filter(|v| v.is_object())
        .unwrap_or_else(|| json!({}));

    // 合并启发式密钥建议：即便 LLM 漏标，也强制 ui:secret=true。
    let mut secret_paths = Vec::new();
    walk_secrets(&args.base_schema_data_or_self(), "", &mut secret_paths);
    for p in &secret_paths {
        set_ui_secret(&mut ui, p);
    }

    let mut sources_list: Vec<Value> = Vec::new();
    if !args.sources.readme_path.is_empty() {
        sources_list.push(Value::String(args.sources.readme_path.clone()));
    }
    for f in &args.sources.files {
        sources_list.push(Value::String(f.path.clone()));
    }

    let now = chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let cfgform = json!({
        "$cfgform": "2.0",
        "target": target_name,
        "format": format,
        "title": title,
        "schema": schema,
        "ui": ui,
        "meta": {
            "generatedBy": "prep-tool/2.0.0",
            "generatedAt": now,
            "stackDetected": args.stack,
            "llm": {
                "used": true,
                "model": model,
                "note": "类型由原文件推断 + 源码/README 经 LLM 精化；约束、枚举含义、帮助文字、密钥与只读标记由 LLM 生成（密钥字段另经启发式强制标注），请作者复核。"
            },
            "sources": sources_list
        }
    });

    Ok(GenResult {
        cfgform,
        llm_raw_excerpt: excerpt,
    })
}

// 让 generate_metadata 能基于「基线 schema 的 properties 键」推断密钥路径。
// 由于密钥启发式作用于「字段名」，而 schema 的属性键即字段名，这里把 schema 的 properties
// 视作数据树键来源；若无 properties 则退回 base_schema 本身。
impl GenArgs {
    fn base_schema_data_or_self(&self) -> Value {
        fn schema_to_keytree(schema: &Value) -> Value {
            match schema.get("type").and_then(|t| t.as_str()) {
                Some("object") => {
                    let mut m = serde_json::Map::new();
                    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
                        for (k, v) in props {
                            m.insert(k.clone(), schema_to_keytree(v));
                        }
                    }
                    Value::Object(m)
                }
                Some("array") => {
                    if let Some(items) = schema.get("items") {
                        json!([schema_to_keytree(items)])
                    } else {
                        json!([])
                    }
                }
                _ => Value::Null,
            }
        }
        schema_to_keytree(&self.base_schema)
    }
}

#[tauri::command]
fn write_cfgform(
    dir: String,
    target_path: String,
    cfgform_value: Value,
) -> Result<WriteResult, String> {
    let base = Path::new(&dir);
    if !base.is_dir() {
        return Err(format!("目录不存在或不可访问：{}", dir));
    }
    let target_p = Path::new(&target_path);
    let target_name = target_p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| "无法解析目标文件名。".to_string())?;

    // v2.0：追加式配对——目标完整文件名 + ".cfgform"（绝不替换 stem）。
    let sidecar_name = format!("{}.cfgform", target_name);
    let sidecar = base.join(&sidecar_name);

    // UTF-8 无 BOM，\n 行尾（serde_json pretty 使用 \n），保证末尾换行
    let mut pretty = serde_json::to_string_pretty(&cfgform_value)
        .map_err(|e| format!("序列化 .cfgform 失败：{}", e))?;
    pretty.push('\n');
    fs::write(&sidecar, pretty.as_bytes())
        .map_err(|e| format!("写入 {} 失败：{}", sidecar_name, e))?;

    // 留痕信息
    let format = cfgform_value["format"].as_str().unwrap_or("unknown");
    let stack = cfgform_value["meta"]["stackDetected"]
        .as_str()
        .unwrap_or("unknown");
    let model = cfgform_value["meta"]["llm"]["model"]
        .as_str()
        .unwrap_or("（未使用）");
    let sources_join = cfgform_value["meta"]["sources"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let sources_display = if sources_join.is_empty() {
        "（无）".to_string()
    } else {
        sources_join.clone()
    };

    let actions = vec![
        format!("已写入边车 {}", sidecar_name),
        format!("目标格式 {}", format),
        format!("探测技术栈 {}", stack),
        format!("使用模型 {}", model),
        format!("读取源文件 {}", sources_display),
        format!("未修改原文件 {}（原名原样）", target_name),
    ];

    // 审计日志（格式无关固定名）
    let log_path = base.join("cfgform-audit.log");
    let now = chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let block = format!(
        "============================================================\n\
[{ts}] prep-tool 生成边车元数据\n\
  目标配置：{tgt}\n\
  目标格式：{fmt}\n\
  探测技术栈：{stack}\n\
  使用模型：{model}\n\
  读取源文件：{srcs}\n\
  写出边车：{sidecar}\n\
  说明：未修改原文件 {tgt}（原名原样）。本日志不记录任何密钥。\n",
        ts = now,
        tgt = target_name,
        fmt = format,
        stack = stack,
        model = model,
        srcs = sources_display,
        sidecar = sidecar_name
    );
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("打开审计日志失败：{}", e))?;
    f.write_all(block.as_bytes())
        .map_err(|e| format!("写入审计日志失败：{}", e))?;

    Ok(WriteResult {
        cfgform_path: sidecar.to_string_lossy().to_string(),
        log_path: log_path.to_string_lossy().to_string(),
        actions,
    })
}

#[tauri::command]
fn read_json_file(path: String) -> Result<Value, String> {
    let content =
        fs::read_to_string(&path).map_err(|e| format!("读取文件失败：{} （{}）", path, e))?;
    let content = strip_bom(&content);
    serde_json::from_str(content).map_err(|e| format!("文件不是合法 JSON：{}", e))
}

#[tauri::command]
fn read_text_file(path: String) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|e| format!("读取文件失败：{} （{}）", path, e))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            exe_dir,
            load_settings,
            save_settings,
            save_key_to_dotenv,
            clear_dotenv_key,
            detect_format,
            read_target_as_value,
            detect_stack,
            infer_schema,
            gather_sources,
            suggest_secrets,
            resolve_llm_config,
            generate_metadata,
            write_cfgform,
            read_json_file,
            read_text_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
