use serde_json::{Map, Value};

pub fn normalize_format(format: &str) -> &str {
    match format {
        "json" | "env" | "toml" | "ini" | "yaml" | "compose" => format,
        "yml" => "yaml",
        _ => format,
    }
}

pub fn parse(format: &str, text: &str) -> Result<Value, String> {
    match normalize_format(format) {
        "json" => parse_json(text),
        "env" => Ok(parse_env(text)),
        "toml" => parse_toml(text),
        "ini" => Ok(parse_ini(text)),
        "yaml" | "compose" => parse_yaml(text),
        other => Err(format!("不支持的配置格式：{}", other)),
    }
}

pub fn serialize(format: &str, value: &Value, original: &str) -> Result<String, String> {
    match normalize_format(format) {
        "json" => serialize_json(value),
        "env" => serialize_env(value, original),
        "toml" => serialize_toml(value, original),
        "ini" => serialize_ini(value, original),
        "yaml" | "compose" => serialize_yaml(value, original),
        other => Err(format!("不支持的配置格式：{}", other)),
    }
}

fn scalar_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => String::new(),
        _ => v.to_string(),
    }
}

// ---------------- JSON ----------------

fn parse_json(text: &str) -> Result<Value, String> {
    if text.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    serde_json::from_str(text).map_err(|e| format!("JSON 解析失败：{}", e))
}

fn serialize_json(value: &Value) -> Result<String, String> {
    let mut s = serde_json::to_string_pretty(value).map_err(|e| format!("JSON 序列化失败：{}", e))?;
    s = s.replace("\r\n", "\n");
    if !s.ends_with('\n') {
        s.push('\n');
    }
    Ok(s)
}

// ---------------- ENV ----------------

fn strip_surrounding_quotes(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            let inner = &s[1..s.len() - 1];
            if first == b'"' {
                return inner.replace("\\\"", "\"").replace("\\n", "\n");
            }
            return inner.to_string();
        }
    }
    s.to_string()
}

struct EnvLine {
    export: bool,
    key: String,
    raw_value: String,
    quote: char,
}

fn parse_env_line(raw: &str) -> Option<EnvLine> {
    let trimmed = raw.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let (export, body) = match trimmed.strip_prefix("export ") {
        Some(rest) => (true, rest.trim_start()),
        None => (false, trimmed),
    };
    let eq = body.find('=')?;
    let key = body[..eq].trim().to_string();
    if key.is_empty() {
        return None;
    }
    let raw_value = body[eq + 1..].trim().to_string();
    let quote = if raw_value.starts_with('"') {
        '"'
    } else if raw_value.starts_with('\'') {
        '\''
    } else {
        '\0'
    };
    Some(EnvLine {
        export,
        key,
        raw_value,
        quote,
    })
}

fn parse_env(text: &str) -> Value {
    let mut map = Map::new();
    for raw in text.lines() {
        if let Some(line) = parse_env_line(raw) {
            map.insert(line.key, Value::String(strip_surrounding_quotes(&line.raw_value)));
        }
    }
    Value::Object(map)
}

fn render_env_value(new_value: &str, original_quote: char) -> String {
    if original_quote == '"' {
        return format!("\"{}\"", new_value.replace('"', "\\\"").replace('\n', "\\n"));
    }
    if original_quote == '\'' {
        return format!("'{}'", new_value);
    }
    if new_value.is_empty() {
        return String::new();
    }
    if new_value.contains(|c: char| c.is_whitespace())
        || new_value.contains('#')
        || new_value.contains('"')
    {
        return format!("\"{}\"", new_value.replace('"', "\\\""));
    }
    new_value.to_string()
}

fn serialize_env(value: &Value, original: &str) -> Result<String, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "ENV 配置必须是键值对象".to_string())?;
    let mut remaining: Vec<String> = obj.keys().cloned().collect();
    let mut out: Vec<String> = Vec::new();

    for raw in original.lines() {
        match parse_env_line(raw) {
            Some(line) => {
                if let Some(nv) = obj.get(&line.key) {
                    let new_str = scalar_to_string(nv);
                    let old_str = strip_surrounding_quotes(&line.raw_value);
                    if new_str == old_str {
                        out.push(raw.to_string());
                    } else {
                        let prefix = if line.export { "export " } else { "" };
                        out.push(format!(
                            "{}{}={}",
                            prefix,
                            line.key,
                            render_env_value(&new_str, line.quote)
                        ));
                    }
                    remaining.retain(|k| k != &line.key);
                }
                // key removed from data: drop line
            }
            None => out.push(raw.to_string()),
        }
    }

    for key in remaining {
        if let Some(v) = obj.get(&key) {
            let s = scalar_to_string(v);
            out.push(format!("{}={}", key, render_env_value(&s, '\0')));
        }
    }

    let mut text = out.join("\n");
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    Ok(text)
}

// ---------------- INI ----------------

fn parse_ini(text: &str) -> Value {
    let mut root = Map::new();
    let mut current: Option<String> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let sec = line[1..line.len() - 1].trim().to_string();
            root.entry(sec.clone()).or_insert_with(|| Value::Object(Map::new()));
            current = Some(sec);
            continue;
        }
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim().to_string();
            if key.is_empty() {
                continue;
            }
            let val = strip_surrounding_quotes(line[eq + 1..].trim());
            match &current {
                Some(sec) => {
                    if let Some(Value::Object(m)) = root.get_mut(sec) {
                        m.insert(key, Value::String(val));
                    }
                }
                None => {
                    root.insert(key, Value::String(val));
                }
            }
        }
    }
    Value::Object(root)
}

fn ini_lookup<'a>(obj: &'a Map<String, Value>, section: &str, key: &str) -> Option<&'a Value> {
    if section.is_empty() {
        let v = obj.get(key)?;
        if v.is_object() {
            return None;
        }
        Some(v)
    } else {
        match obj.get(section) {
            Some(Value::Object(m)) => m.get(key),
            _ => None,
        }
    }
}

fn serialize_ini(value: &Value, original: &str) -> Result<String, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "INI 配置必须是对象".to_string())?;

    let mut emitted: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();

    for raw in original.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            current = line[1..line.len() - 1].trim().to_string();
            out.push(raw.to_string());
            continue;
        }
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            out.push(raw.to_string());
            continue;
        }
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim().to_string();
            if let Some(nv) = ini_lookup(obj, &current, &key) {
                let old_str = strip_surrounding_quotes(line[eq + 1..].trim());
                let new_str = scalar_to_string(nv);
                if new_str == old_str {
                    out.push(raw.to_string());
                } else {
                    let indent_len = raw.len() - raw.trim_start().len();
                    out.push(format!("{}{}={}", &raw[..indent_len], key, new_str));
                }
                emitted.insert((current.clone(), key));
            }
            // removed key: drop
        } else {
            out.push(raw.to_string());
        }
    }

    // new root-level scalars: valid INI before any section, prepend at top
    let mut prepend: Vec<String> = Vec::new();
    for (k, v) in obj {
        if !v.is_object() && !emitted.contains(&(String::new(), k.clone())) {
            prepend.push(format!("{}={}", k, scalar_to_string(v)));
        }
    }

    // new section keys (existing or new sections): append at end
    let mut append: Vec<String> = Vec::new();
    for (sec, v) in obj {
        if let Value::Object(m) = v {
            let mut block: Vec<String> = Vec::new();
            for (k, kv) in m {
                if !emitted.contains(&(sec.clone(), k.clone())) {
                    block.push(format!("{}={}", k, scalar_to_string(kv)));
                }
            }
            if !block.is_empty() {
                if !append.is_empty() {
                    append.push(String::new());
                }
                append.push(format!("[{}]", sec));
                append.extend(block);
            }
        }
    }

    let mut all: Vec<String> = Vec::new();
    all.extend(prepend);
    all.extend(out);
    if !append.is_empty() {
        if all.last().map(|l| !l.is_empty()).unwrap_or(false) {
            all.push(String::new());
        }
        all.extend(append);
    }

    let mut text = all.join("\n");
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    Ok(text)
}

// ---------------- TOML ----------------

fn toml_value_to_json(v: &toml_edit::Value) -> Value {
    match v {
        toml_edit::Value::String(s) => Value::String(s.value().clone()),
        toml_edit::Value::Integer(i) => Value::from(*i.value()),
        toml_edit::Value::Float(f) => Value::from(*f.value()),
        toml_edit::Value::Boolean(b) => Value::from(*b.value()),
        toml_edit::Value::Datetime(d) => Value::String(d.value().to_string()),
        toml_edit::Value::Array(a) => Value::Array(a.iter().map(toml_value_to_json).collect()),
        toml_edit::Value::InlineTable(t) => {
            let mut m = Map::new();
            for (k, item) in t.iter() {
                m.insert(k.to_string(), toml_value_to_json(item));
            }
            Value::Object(m)
        }
    }
}

fn toml_table_to_json(t: &toml_edit::Table) -> Value {
    let mut m = Map::new();
    for (k, item) in t.iter() {
        m.insert(k.to_string(), toml_item_to_json(item));
    }
    Value::Object(m)
}

fn toml_item_to_json(item: &toml_edit::Item) -> Value {
    match item {
        toml_edit::Item::Value(v) => toml_value_to_json(v),
        toml_edit::Item::Table(t) => toml_table_to_json(t),
        toml_edit::Item::ArrayOfTables(arr) => {
            Value::Array(arr.iter().map(toml_table_to_json).collect())
        }
        toml_edit::Item::None => Value::Null,
    }
}

fn parse_toml(text: &str) -> Result<Value, String> {
    if text.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("TOML 解析失败：{}", e))?;
    Ok(toml_table_to_json(doc.as_table()))
}

fn json_to_toml_inline_value(v: &Value) -> toml_edit::Value {
    match v {
        Value::Null => toml_edit::Value::from(""),
        Value::Bool(b) => toml_edit::Value::from(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml_edit::Value::from(i)
            } else {
                toml_edit::Value::from(n.as_f64().unwrap_or(0.0))
            }
        }
        Value::String(s) => toml_edit::Value::from(s.clone()),
        Value::Array(a) => {
            let mut arr = toml_edit::Array::new();
            for e in a {
                arr.push(json_to_toml_inline_value(e));
            }
            toml_edit::Value::Array(arr)
        }
        Value::Object(o) => {
            let mut t = toml_edit::InlineTable::new();
            for (k, e) in o {
                t.insert(k, json_to_toml_inline_value(e));
            }
            toml_edit::Value::InlineTable(t)
        }
    }
}

fn json_scalar_to_toml_item(v: &Value) -> toml_edit::Item {
    toml_edit::Item::Value(json_to_toml_inline_value(v))
}

fn merge_json_into_table(table: &mut toml_edit::Table, obj: &Map<String, Value>) {
    let existing: Vec<String> = table.iter().map(|(k, _)| k.to_string()).collect();
    for k in existing {
        if !obj.contains_key(&k) {
            table.remove(&k);
        }
    }
    for (k, v) in obj {
        match v {
            Value::Object(child) => {
                let is_table = table.get(k).map(|i| i.is_table()).unwrap_or(false);
                if !is_table {
                    table.insert(k, toml_edit::Item::Table(toml_edit::Table::new()));
                }
                if let Some(toml_edit::Item::Table(t)) = table.get_mut(k) {
                    merge_json_into_table(t, child);
                }
            }
            _ => {
                let differs = match table.get(k) {
                    Some(item) => toml_item_to_json(item) != *v,
                    None => true,
                };
                if differs {
                    table.insert(k, json_scalar_to_toml_item(v));
                }
            }
        }
    }
}

fn serialize_toml(value: &Value, original: &str) -> Result<String, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "TOML 配置必须是对象".to_string())?;
    let mut doc = if original.trim().is_empty() {
        toml_edit::DocumentMut::new()
    } else {
        original
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| format!("原 TOML 文件解析失败：{}", e))?
    };
    merge_json_into_table(doc.as_table_mut(), obj);
    Ok(doc.to_string())
}

// ---------------- YAML / Compose ----------------
//
// 保真策略（外科手术式 + 安全网）：
//   1. 仅用 serde_yaml 做「解析」（文本 -> Value）。
//   2. 序列化时逐行扫描原文：注释行、空行、未改动的值行一律「原样输出」，
//      因此注释、键顺序、锚点(&a/*a)、缩进等全部保留。
//   3. 只有「叶子标量值确实改变」的那一行，才就地改写其值部分，
//      同时保留原缩进、键名、以及行尾内联 `# 注释`。
//   4. 删除的键 -> 丢弃该行（含其子树）；新增的键 -> 尽力按父级缩进追加到该块末尾。
//   5. 【安全网】手术结果会被重新解析校验：若与目标 Value 语义不一致
//      （通常是复杂结构性增删/类型变更等手术无法忠实表达的情况），
//      自动回退为 serde_yaml 全量序列化（数据正确，但该次会丢失注释）。
//
// 已知限制（回退场景，写前请看 Dry-run）：深层嵌套键的新增/删除、
// 标量<->对象/数组 的类型互换、数组长度结构性变化等，可能触发整文档回退。

fn parse_yaml(text: &str) -> Result<Value, String> {
    if text.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    serde_yaml::from_str(text).map_err(|e| format!("YAML 解析失败：{}", e))
}

fn serialize_yaml_full(value: &Value) -> Result<String, String> {
    let mut s = serde_yaml::to_string(value).map_err(|e| format!("YAML 序列化失败：{}", e))?;
    if !s.ends_with('\n') {
        s.push('\n');
    }
    Ok(s)
}

#[derive(Clone)]
enum YSeg {
    Key(String),
    Idx(usize),
}

fn yseg_path_key(path: &[YSeg]) -> String {
    let mut k = String::new();
    for s in path {
        match s {
            YSeg::Key(x) => {
                k.push('\u{1}');
                k.push_str(x);
            }
            YSeg::Idx(i) => {
                k.push('\u{2}');
                k.push_str(&i.to_string());
            }
        }
    }
    k
}

fn yaml_get<'a>(root: &'a Value, path: &[YSeg]) -> Option<&'a Value> {
    let mut cur = root;
    for seg in path {
        match seg {
            YSeg::Key(k) => cur = cur.as_object()?.get(k)?,
            YSeg::Idx(i) => cur = cur.as_array()?.get(*i)?,
        }
    }
    Some(cur)
}

fn yaml_indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

// Position (byte index) of an inline `#` comment, honoring quotes; None if absent.
fn yaml_inline_comment_pos(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut in_s = false;
    let mut in_d = false;
    let mut prev_ws = true;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'\'' if !in_d => in_s = !in_s,
            b'"' if !in_s => in_d = !in_d,
            b'#' if !in_s && !in_d && prev_ws => return Some(i),
            _ => {}
        }
        prev_ws = c == b' ' || c == b'\t';
        i += 1;
    }
    None
}

// For a line body (after leading indent) of a mapping entry, return the byte
// index just AFTER the separating colon (the `:` followed by space or EOL,
// outside quotes). None if this is not a `key:` line.
fn yaml_map_colon(body: &str) -> Option<usize> {
    let bytes = body.as_bytes();
    let mut in_s = false;
    let mut in_d = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'\'' if !in_d => in_s = !in_s,
            b'"' if !in_s => in_d = !in_d,
            b':' if !in_s && !in_d => {
                let next = bytes.get(i + 1).copied();
                if next == Some(b' ') || next.is_none() || next == Some(b'\t') {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

// Render a JSON scalar as a YAML scalar token (correctly quoted) via serde_yaml.
fn render_yaml_scalar(v: &Value) -> Option<String> {
    if v.is_object() || v.is_array() {
        return None;
    }
    let s = serde_yaml::to_string(v).ok()?;
    let token = s.trim_end_matches('\n');
    // Multi-line scalars cannot be edited inline surgically -> signal fallback.
    if token.contains('\n') {
        return None;
    }
    Some(token.to_string())
}

struct YFrame {
    indent: usize,
    path: Vec<YSeg>,
    is_seq_owner: bool, // value at path is an array
    seq_idx: usize,
    in_seq_elem: bool, // this frame represents a `- {map}` element
}

fn serialize_yaml(value: &Value, original: &str) -> Result<String, String> {
    if original.trim().is_empty() {
        return serialize_yaml_full(value);
    }
    // Surgical attempt; if anything is structurally unrepresentable we fall back.
    let surgical = surgical_yaml(value, original);
    if let Some(text) = surgical {
        if let Ok(reparsed) = parse_yaml(&text) {
            if &reparsed == value {
                return Ok(text);
            }
        }
    }
    serialize_yaml_full(value)
}

fn surgical_yaml(root: &Value, original: &str) -> Option<String> {
    let mut out: Vec<String> = Vec::new();
    let mut stack: Vec<YFrame> = Vec::new();
    // skip everything more-indented than this (removed subtree)
    let mut skip_above: Option<usize> = None;
    let mut covered: std::collections::HashSet<String> = std::collections::HashSet::new();

    let lines: Vec<&str> = original.lines().collect();
    for raw in &lines {
        let line = *raw;
        let indent = yaml_indent(line);
        let trimmed = line.trim_start_matches(' ');

        // handle removed-subtree skipping
        if let Some(si) = skip_above {
            if !trimmed.is_empty() && indent > si {
                continue; // still inside removed subtree
            }
            skip_above = None;
        }

        // comments / blanks / document markers: verbatim
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "---" || trimmed == "..." {
            out.push(line.to_string());
            continue;
        }

        // pop frames we've dedented out of
        while let Some(f) = stack.last() {
            let pop = if trimmed.starts_with('-') {
                // sequence item: also close a previous sibling seq element at same indent
                if f.in_seq_elem {
                    f.indent >= indent
                } else {
                    f.indent > indent
                }
            } else {
                f.indent >= indent
            };
            if pop {
                stack.pop();
            } else {
                break;
            }
        }

        if trimmed.starts_with('-') {
            // sequence item
            let owner = match stack.last() {
                Some(f) if f.is_seq_owner => f,
                _ => return None, // unexpected sequence context -> fall back
            };
            let owner_path = owner.path.clone();
            let idx = owner.seq_idx;
            if let Some(f) = stack.last_mut() {
                f.seq_idx += 1;
            }
            let mut elem_path = owner_path.clone();
            elem_path.push(YSeg::Idx(idx));
            covered.insert(yseg_path_key(&elem_path));

            // dash payload
            let after = &trimmed[1..]; // drop '-'
            let pad = after.len() - after.trim_start_matches(' ').len();
            let payload = after.trim_start_matches(' ');
            let dash_prefix_len = indent + 1 + pad; // up to start of payload

            if payload.is_empty() {
                // `-` alone (block element follows on next lines): treat as seq elem map frame
                stack.push(YFrame {
                    indent,
                    path: elem_path,
                    is_seq_owner: false,
                    seq_idx: 0,
                    in_seq_elem: true,
                });
                out.push(line.to_string());
                continue;
            }

            // is payload a `key: ...` (map element) or a scalar?
            if let Some(colon_end) = yaml_map_colon(payload) {
                // `- key: value` map element; push elem frame and emit, then treat the key
                let new_elem = yaml_get(root, &elem_path);
                if new_elem.map(|v| v.is_object()).unwrap_or(false) {
                    // emit the dash line possibly editing the inline key value
                    let key_raw = &payload[..colon_end - 1];
                    let key = yaml_unquote_key(key_raw.trim());
                    let mut child_path = elem_path.clone();
                    child_path.push(YSeg::Key(key.clone()));
                    covered.insert(yseg_path_key(&child_path));
                    let rest = &payload[colon_end..];
                    let new_line = edit_value_line(
                        line,
                        dash_prefix_len + colon_end,
                        rest,
                        root,
                        &child_path,
                    )?;
                    out.push(new_line);
                    // push the seq element frame so deeper keys attach to it
                    stack.push(YFrame {
                        indent,
                        path: elem_path,
                        is_seq_owner: false,
                        seq_idx: 0,
                        in_seq_elem: true,
                    });
                    continue;
                } else {
                    return None; // structural change
                }
            } else {
                // scalar sequence item `- value`
                let new_v = yaml_get(root, &elem_path)?;
                if new_v.is_object() || new_v.is_array() {
                    return None;
                }
                // value region is the whole payload (+ trailing comment)
                let new_line = edit_value_line(line, dash_prefix_len, payload, root, &elem_path)?;
                out.push(new_line);
                continue;
            }
        }

        // mapping entry `key:` or `key: value`
        let body = trimmed;
        let colon_end = match yaml_map_colon(body) {
            Some(c) => c,
            None => {
                // unrecognized line; keep verbatim (could be a continuation)
                out.push(line.to_string());
                continue;
            }
        };
        let key_raw = &body[..colon_end - 1];
        let key = yaml_unquote_key(key_raw.trim());

        let parent_path = stack.last().map(|f| f.path.clone()).unwrap_or_default();
        let mut path = parent_path.clone();
        path.push(YSeg::Key(key.clone()));
        let path_key = yseg_path_key(&path);

        let new_v = yaml_get(root, &path);
        if new_v.is_none() {
            // removed key: drop this line and its subtree
            skip_above = Some(indent);
            continue;
        }
        let new_v = new_v.unwrap();
        covered.insert(path_key);

        let rest = &body[colon_end..]; // includes leading space(s) + value + comment
        let value_part = rest.trim_start_matches(' ');
        // strip inline comment to inspect whether a scalar value is present
        let has_scalar = match yaml_inline_comment_pos(value_part) {
            Some(p) => !value_part[..p].trim().is_empty(),
            None => !value_part.trim().is_empty(),
        };

        if has_scalar {
            // leaf scalar line
            if new_v.is_object() || new_v.is_array() {
                return None; // scalar -> container, structural
            }
            let prefix_len = indent + colon_end;
            let new_line = edit_value_line(line, prefix_len, rest, root, &path)?;
            out.push(new_line);
        } else {
            // parent (object or array). emit verbatim and push frame.
            if !new_v.is_object() && !new_v.is_array() {
                return None; // container -> scalar, structural
            }
            out.push(line.to_string());
            stack.push(YFrame {
                indent,
                path,
                is_seq_owner: new_v.is_array(),
                seq_idx: 0,
                in_seq_elem: false,
            });
        }
    }

    // additions: any top-level key in root not covered -> append at end (best effort).
    // Deeper additions are left for the verification fallback.
    if let Some(obj) = root.as_object() {
        for (k, v) in obj {
            let p = vec![YSeg::Key(k.clone())];
            if !covered.contains(&yseg_path_key(&p)) {
                emit_yaml_addition(&mut out, 0, k, v)?;
            }
        }
    }

    let mut text = out.join("\n");
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    Some(text)
}

fn yaml_unquote_key(k: &str) -> String {
    let b = k.as_bytes();
    if b.len() >= 2 {
        let f = b[0];
        let l = b[b.len() - 1];
        if (f == b'"' && l == b'"') || (f == b'\'' && l == b'\'') {
            return k[1..k.len() - 1].to_string();
        }
    }
    k.to_string()
}

// Rewrite (if changed) the value portion of a line. `value_region` is the slice
// of the line starting at byte index `region_start` (everything after the
// `:`/`- ` separator), including leading spaces, the value, and any inline comment.
fn edit_value_line(
    line: &str,
    region_start: usize,
    value_region: &str,
    root: &Value,
    path: &[YSeg],
) -> Option<String> {
    let new_v = yaml_get(root, path)?;
    let lead = value_region.len() - value_region.trim_start_matches(' ').len();
    let body = &value_region[lead..];
    let (value_text, _comment) = match yaml_inline_comment_pos(body) {
        Some(p) => (body[..p].trim_end(), &body[p..]),
        None => (body.trim_end(), ""),
    };
    // original parsed scalar
    let orig_scalar: Value = serde_yaml::from_str(value_text).unwrap_or(Value::Null);
    if &orig_scalar == new_v {
        return Some(line.to_string()); // unchanged -> verbatim
    }
    let rendered = render_yaml_scalar(new_v)?;
    // rebuild: prefix (up to & incl separator) + original leading spaces + new value + trailing(ws+comment)
    let prefix = &line[..region_start];
    let leading_spaces = &value_region[..lead];
    let value_end_in_body = value_text.len();
    let trailing = &body[value_end_in_body..]; // trailing ws + inline comment
    Some(format!("{}{}{}{}", prefix, leading_spaces, rendered, trailing))
}

// Append a new key (and its value subtree) as YAML lines at the given indent.
fn emit_yaml_addition(
    out: &mut Vec<String>,
    indent: usize,
    key: &str,
    value: &Value,
) -> Option<String> {
    let pad = " ".repeat(indent);
    match value {
        Value::Object(_) | Value::Array(_) => {
            // serialize subtree via serde_yaml then re-indent under the key
            let sub = serde_yaml::to_string(value).ok()?;
            out.push(format!("{}{}:", pad, key));
            for l in sub.lines() {
                if l == "---" || l.trim().is_empty() {
                    continue;
                }
                out.push(format!("{}  {}", pad, l));
            }
        }
        _ => {
            let token = render_yaml_scalar(value)?;
            out.push(format!("{}{}: {}", pad, key, token));
        }
    }
    Some(String::new())
}

// ---------------- line diff (for Dry-run preview) ----------------

pub fn line_diff(old: &str, new: &str) -> Vec<String> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let n = a.len();
    let m = b.len();
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    let mut j = 0;
    while i < n && j < m {
        if a[i] == b[j] {
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            out.push(format!("删除  {}", a[i]));
            i += 1;
        } else {
            out.push(format!("新增  {}", b[j]));
            j += 1;
        }
    }
    while i < n {
        out.push(format!("删除  {}", a[i]));
        i += 1;
    }
    while j < m {
        out.push(format!("新增  {}", b[j]));
        j += 1;
    }
    if out.is_empty() {
        out.push("（与现文件无差异）".to_string());
    }
    out
}
