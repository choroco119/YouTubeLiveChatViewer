use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictEntry {
    pub pattern: String,
    pub replacement: String,
    pub is_regex: bool,
}

fn dict_path() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    exe_dir.join("dictionary.json")
}

pub fn load_dictionary() -> Vec<DictEntry> {
    let path = dict_path();
    if let Ok(data) = fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        Vec::new()
    }
}

pub fn save_dictionary(entries: &[DictEntry]) {
    let path = dict_path();
    if let Ok(json) = serde_json::to_string_pretty(entries) {
        let _ = fs::write(path, json);
    }
}

pub fn apply_dictionary(text: &str, entries: &[DictEntry]) -> String {
    let mut result = text.to_string();
    for entry in entries {
        if entry.pattern.is_empty() {
            continue;
        }
        if entry.is_regex {
            if let Ok(re) = regex::Regex::new(&entry.pattern) {
                result = re.replace_all(&result, entry.replacement.as_str()).into_owned();
            }
        } else {
            result = result.replace(&entry.pattern, &entry.replacement);
        }
    }
    result
}
