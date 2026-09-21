//! Model-facing text. The CLI and private socket retain JSON.
use serde_json::Value;

pub const TEXT_BUDGET: usize = 8192;
const TRUNCATED: &str = "\n[output truncated; changes are not cancelled]";

pub fn prefix(text: &str, bytes: usize) -> &str {
    let mut end = bytes.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

pub fn bounded(text: &str) -> String {
    if text.len() <= TEXT_BUDGET {
        text.to_owned()
    } else {
        format!(
            "{}{}",
            prefix(text, TEXT_BUDGET - TRUNCATED.len()),
            TRUNCATED
        )
    }
}

// Bare identifiers remain distinguishable from JSON scalars and the missing-cell dash.
// Everything else is JSON-escaped, including numeric strings and reserved literals.
fn identifier(text: &str) -> String {
    let mut bytes = text.bytes();
    let simple = bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        && !matches!(text, "null" | "true" | "false" | "NaN" | "Infinity");
    if simple {
        text.into()
    } else {
        Value::String(text.into()).to_string()
    }
}

fn cell(value: &Value) -> String {
    match value {
        Value::String(text) => identifier(text),
        // Never route integers through f64: IDs and counts above 2^53 must stay exact.
        _ => value.to_string(),
    }
}

fn inspection_cell(value: &Value) -> String {
    match value {
        Value::Number(n) if n.is_f64() => n
            .as_f64()
            .map(|n| n.to_string())
            .unwrap_or_else(|| n.to_string()),
        Value::Array(a) => {
            let head = a
                .iter()
                .take(4)
                .map(|v| match v {
                    Value::Number(_) => inspection_cell(v),
                    _ => v.to_string(),
                })
                .collect::<Vec<_>>()
                .join(",");
            if a.len() > 4 {
                format!("[{head},…(+{})]", a.len() - 4)
            } else {
                format!("[{head}]")
            }
        }
        _ => cell(value),
    }
}

fn contract(name: &str, value: &Value) -> String {
    let permission = match value.get("permission") {
        Some(Value::Null) => "read",
        Some(Value::String(permission)) => permission,
        _ => "unknown",
    };
    let mut text = format!(
        "{name} [{permission}]: {}",
        value["summary"].as_str().unwrap_or("")
    );
    if let Some(args) = value["args"].as_object() {
        let required = value["required"].as_array();
        for (key, hint) in args {
            let optional = required.is_some_and(|required| !required.iter().any(|v| v == key));
            let hint = hint.as_str().unwrap_or("");
            let hint = if optional {
                hint.strip_prefix("optional ").unwrap_or(hint)
            } else {
                hint
            };
            text.push_str(&format!(
                "\n{key}{}: {}",
                if optional { "?" } else { "" },
                hint
            ));
        }
    }
    text
}

fn discovery(args: &Value, value: &Value) -> String {
    if value.get("args").is_some() {
        return bounded(&contract(
            args["operation"].as_str().unwrap_or("operation"),
            value,
        ));
    }
    let Some(entries) = value.as_object() else {
        return bounded(&value.to_string());
    };
    let mut text = String::new();
    let mut remaining = Vec::new();
    for (name, value) in entries {
        let line = if let Some(summary) = value.as_str() {
            format!("{name}: {summary}")
        } else {
            contract(name, value)
        };
        // Reserve space for the complete list of unreturned operation names.
        if remaining.is_empty() && text.len() + line.len() + 512 < TEXT_BUDGET {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&line);
        } else {
            remaining.push(name.as_str());
        }
    }
    if !remaining.is_empty() {
        text.push_str(&format!("\nremaining: {}", remaining.join(",")));
    }
    bounded(&text)
}

fn inspection(args: &Value, value: &Value) -> String {
    let rows = value["objects"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let offset = args["offset"].as_u64().unwrap_or(0);
    let total = value["total"].as_u64().unwrap_or(0);
    let mut text = format!("scene={} total={total}", cell(&value["scene"]));
    if args["context"] == true {
        text.push_str(&format!(
            " blender={} permissions={}",
            value["blender"].as_str().unwrap_or("?"),
            value["permissions"]
                .as_array()
                .map(|a| a.iter().map(cell).collect::<Vec<_>>().join(","))
                .unwrap_or_default()
        ));
    }
    text.push('\n');
    if let Some(missing) = value["missing"].as_array().filter(|a| !a.is_empty()) {
        text.push_str(&format!(
            "missing={} {}\n",
            missing.len(),
            inspection_cell(&value["missing"])
        ));
    }
    let mut columns = vec!["name"];
    for row in rows {
        if let Some(object) = row.as_object() {
            for key in object.keys() {
                if !columns.contains(&key.as_str()) {
                    columns.push(key);
                }
            }
        }
    }
    text.push_str(&columns.join("\t"));
    let mut returned = 0;
    for row in rows {
        let line = columns
            .iter()
            .map(|key| {
                row.get(*key)
                    .map(inspection_cell)
                    .unwrap_or_else(|| "-".into())
            })
            .collect::<Vec<_>>()
            .join("\t");
        if text.len() + line.len() + 160 > TEXT_BUDGET {
            break;
        }
        text.push('\n');
        text.push_str(&line);
        returned += 1;
    }
    let next = offset + returned;
    if next < total {
        text.push_str(&format!("\nreturned={returned} next_offset={next}"));
        if returned == 0 && !rows.is_empty() {
            text.push_str("\n[row too large; select fewer fields]");
        }
    }
    bounded(&text)
}

/// Reserve the truncation marker while adding complete metadata lines and table rows.
/// Only explicitly labelled text previews may end mid-value.
#[derive(Default)]
struct Text {
    text: String,
    truncated: bool,
}

impl Text {
    fn line(&mut self, line: &str) -> bool {
        let separator = usize::from(!self.text.is_empty());
        if self.text.len() + separator + line.len() > TEXT_BUDGET - TRUNCATED.len() {
            self.truncated = true;
            return false;
        }
        if separator != 0 {
            self.text.push('\n');
        }
        self.text.push_str(line);
        true
    }

    fn preview(&mut self, label: &str, value: &str) {
        let line = format!("{label}: {}", Value::String(value.into()));
        if !self.line(&line) {
            let remaining = (TEXT_BUDGET - TRUNCATED.len()).saturating_sub(self.text.len() + 1);
            // The final marker explicitly identifies this UTF-8-safe text prefix as incomplete.
            if remaining > label.len() + 3 {
                self.line(prefix(&line, remaining));
            }
        }
    }

    fn value(&mut self, label: &str, value: &Value) {
        if let Some((rows, columns)) = table(value) {
            let header = format!(
                "{label} rows={}:\n{}",
                rows.len(),
                columns
                    .iter()
                    .map(|c| identifier(c))
                    .collect::<Vec<_>>()
                    .join("\t")
            );
            if !self.line(&header) {
                return;
            }
            for row in rows {
                let line = columns
                    .iter()
                    .map(|key| cell(&row[*key]))
                    .collect::<Vec<_>>()
                    .join("\t");
                if !self.line(&line) {
                    break;
                }
            }
        } else if !self.line(&format!("{label}: {value}")) {
            // Preserve complete identifiers and valid JSON instead of cutting inside a value.
            self.line(&format!("{label}: [omitted; return fewer fields/items]"));
        }
    }

    fn finish(mut self) -> String {
        if self.truncated {
            self.text.push_str(TRUNCATED);
        }
        self.text
    }
}

fn table(value: &Value) -> Option<(&[Value], Vec<&str>)> {
    let rows = value.as_array()?;
    if rows.len() < 2 {
        return None;
    }
    let first = rows[0].as_object()?;
    if first.is_empty() {
        return None;
    }
    let columns: Vec<_> = first.keys().map(String::as_str).collect();
    rows.iter()
        .all(|row| {
            row.as_object().is_some_and(|object| {
                object.len() == columns.len()
                    && columns.iter().all(|key| {
                        object
                            .get(*key)
                            .is_some_and(|v| !v.is_array() && !v.is_object())
                    })
            })
        })
        .then_some((rows, columns))
}

fn execution(args: &Value, value: &Value) -> String {
    if value.get("validated").is_some() {
        return format!(
            "validated={} executed=0 scope=syntax/permissions",
            value["validated"]
        );
    }
    let mut text = Text::default();
    if value["ok"] == false {
        text.line(&format!(
            "error failed_index={} completed={} partial=true",
            value["failed_index"], value["completed"]
        ));
        text.line(prefix(
            value["error"].as_str().unwrap_or("Operation failed"),
            2048,
        ));
    } else {
        text.line("ok");
    }
    let results = value["results"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let batch = args["steps"].as_array().is_some_and(|s| s.len() > 1)
        || results.iter().any(|r| r["index"].as_u64().unwrap_or(0) > 0)
        || value["failed_index"].as_u64().unwrap_or(0) > 0;
    let label = |result: &Value, field: &str| {
        if batch {
            format!("step={} {field}", result["index"])
        } else {
            field.into()
        }
    };
    // New handles and files precede optional payloads so large stdout cannot hide them.
    for result in results {
        let index = result["index"].as_u64().unwrap_or(0) as usize;
        let step = args["steps"].get(index).unwrap_or(args);
        if let Some(script) = result["result"]["script"].as_str()
            && step.get("script").is_none()
        {
            text.line(&format!("{}={script}", label(result, "script")));
        }
    }
    if let Some(files) = value["files"].as_array() {
        for file in files {
            text.line(&format!("file={}", cell(&file["file"])));
        }
    }
    for result in results {
        let data = &result["result"];
        if data.get("script").is_some() {
            if let Some(value) = data.get("value") {
                text.value(&label(result, "result"), value);
            }
            if let Some(preview) = data["result_preview"].as_str() {
                text.preview(
                    &label(
                        result,
                        &format!("result truncated chars={}", data["result_chars"]),
                    ),
                    preview,
                );
            }
            if let Some(stdout) = data["stdout"].as_str() {
                text.preview(
                    &label(
                        result,
                        if data["stdout_truncated"] == true {
                            "stdout truncated"
                        } else {
                            "stdout"
                        },
                    ),
                    stdout,
                );
            }
        } else {
            text.value(&label(result, "result"), data);
        }
    }
    text.finish()
}

pub fn result(method: &str, args: &Value, value: &Value) -> String {
    match method {
        "inspect" => inspection(args, value),
        "discover" => discovery(args, value),
        "execute" => execution(args, value),
        _ => bounded(&value.to_string()),
    }
}
