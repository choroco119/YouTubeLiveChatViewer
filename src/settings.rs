use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NarratorProfile {
    pub speed: u32,
    pub pitch: u32,
    pub volume: u32,
    pub alpha: u32,
    pub intonation: u32,
    pub emotions: HashMap<String, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserNote {
    pub name: String,
    pub note: String,
}



#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SeEntry {
    pub pattern: String,
    pub file_path: String,
    pub volume: f32,
}

impl NarratorProfile {
    pub fn default_values() -> Self {
        Self {
            speed: 50,
            pitch: 50,
            volume: 50,
            alpha: 50,
            intonation: 50,
            emotions: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub video_id: String,
    pub narrator: String,
    pub profiles: HashMap<String, NarratorProfile>,
    pub window_width: Option<f32>,
    pub window_height: Option<f32>,
    pub tts_enabled: bool,
    pub skip_threshold: u32,
    #[serde(skip)]
    pub user_notes: HashMap<String, UserNote>,
    #[serde(default)]
    pub ng_words: Vec<String>,
    #[serde(default)]
    pub ng_users: Vec<String>,
    #[serde(default = "default_max_read_length")]
    pub max_read_length: u32,
    #[serde(default = "default_read_more_text")]
    pub read_more_text: String,
    #[serde(default = "default_ng_replacement_text")]
    pub ng_replacement_text: String,
    #[serde(default)]
    pub se_entries: Vec<SeEntry>,
    #[serde(default)]
    pub gemini_api_key: String,
    #[serde(default)]
    pub gemini_enabled: bool,
    #[serde(default = "default_gemini_interval_secs")]
    pub gemini_interval_secs: u32,
    #[serde(default = "default_gemini_system_prompt")]
    pub gemini_system_prompt: String,
    #[serde(default = "default_gemini_model")]
    pub gemini_model: String,
    #[serde(default)]
    pub obs_server_enabled: bool,
    #[serde(default = "default_obs_server_port")]
    pub obs_server_port: u16,
    #[serde(default = "default_gemini_name")]
    pub gemini_name: String,
    #[serde(default)]
    pub log_dir: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            video_id: String::new(),
            narrator: String::new(),
            profiles: HashMap::new(),
            window_width: None,
            window_height: None,
            tts_enabled: false,
            skip_threshold: 0,
            user_notes: HashMap::new(),
            ng_words: Vec::new(),
            ng_users: Vec::new(),
            max_read_length: default_max_read_length(),
            read_more_text: default_read_more_text(),
            ng_replacement_text: default_ng_replacement_text(),
            se_entries: Vec::new(),
            gemini_api_key: String::new(),
            gemini_enabled: false,
            gemini_interval_secs: default_gemini_interval_secs(),
            gemini_system_prompt: default_gemini_system_prompt(),
            gemini_model: default_gemini_model(),
            obs_server_enabled: false,
            obs_server_port: default_obs_server_port(),
            gemini_name: default_gemini_name(),
            log_dir: String::new(),
        }
    }
}

fn default_gemini_name() -> String {
    "Gemini Assistant".to_string()
}

fn default_obs_server_port() -> u16 {
    3000
}

fn default_max_read_length() -> u32 {
    100
}

fn default_read_more_text() -> String {
    "以下略".to_string()
}

fn default_ng_replacement_text() -> String {
    "不適切な表現が含まれています".to_string()
}

fn default_gemini_interval_secs() -> u32 {
    30
}

fn default_gemini_system_prompt() -> String {
    "あなたは配信のAIアシスタントです。リスナーからの複数のコメントに対して、配信を盛り上げるように短く100文字以内で回答してください。".to_string()
}

fn default_gemini_model() -> String {
    "gemini-2.5-flash".to_string()
}

fn config_path() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    exe_dir.join("settings.json")
}

fn notes_path() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    exe_dir.join("user_notes.json")
}

pub fn load() -> Settings {
    let path = config_path();
    let mut settings: Settings = if let Ok(data) = fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        Settings::default()
    };

    // ユーザーメモを別ファイルから読み込む
    let n_path = notes_path();
    if let Ok(data) = fs::read_to_string(&n_path) {
        if let Ok(notes) = serde_json::from_str(&data) {
            settings.user_notes = notes;
        }
    }

    settings
}

pub fn save(settings: &Settings) {
    // メイン設定の保存
    let path = config_path();
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(path, json);
    }

    // ユーザーメモの保存
    let n_path = notes_path();
    if let Ok(json) = serde_json::to_string_pretty(&settings.user_notes) {
        let _ = fs::write(n_path, json);
    }
}
