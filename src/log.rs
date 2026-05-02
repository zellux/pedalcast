use std::time::{SystemTime, UNIX_EPOCH};

pub fn info(target: &str, message: &str, fields: &[(&str, String)]) {
    write("INFO", target, message, fields);
}

pub fn warn(target: &str, message: &str, fields: &[(&str, String)]) {
    write("WARN", target, message, fields);
}

pub fn error(target: &str, message: &str, fields: &[(&str, String)]) {
    write("ERROR", target, message, fields);
}

fn write(level: &str, target: &str, message: &str, fields: &[(&str, String)]) {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();

    let mut line = format!("{level} ts_ms={millis} {target} {message}");
    for (key, value) in fields {
        line.push(' ');
        line.push_str(key);
        line.push('=');
        line.push_str(&quote_if_needed(value));
    }
    println!("{line}");
}

fn quote_if_needed(value: &str) -> String {
    if value.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ':' | '/')
    }) {
        value.to_string()
    } else {
        format!("{value:?}")
    }
}
