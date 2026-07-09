
use crate::cevio::{CevioCommand, CevioParams, get_narrators, get_emotions, start_cevio_thread};
use crate::dictionary::{DictEntry, load_dictionary, save_dictionary};
use crate::settings::{self, NarratorProfile, Settings, UserNote};
use crate::text_filter::clean_text;
use crate::youtube::{ChatEvent, ChatMessage, start_chat_monitor};
use egui::*;
use std::collections::HashMap;
use std::sync::Arc;
use crate::web_server;
use tokio::sync::mpsc;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use std::io::BufReader;
use std::fs::File;

// コメント表示用の色設定
const COLOR_OWNER: Color32 = Color32::from_rgb(255, 230, 100);     // 明るい金
const COLOR_MODERATOR: Color32 = Color32::from_rgb(130, 220, 255); // 明るい水色
const COLOR_MEMBER: Color32 = Color32::from_rgb(100, 255, 180);   // 明るい緑
const COLOR_NORMAL: Color32 = Color32::from_rgb(255, 255, 255);   // 純白に変更
const COLOR_TIME: Color32 = Color32::from_rgb(160, 160, 160);    // 灰(少し明るく)

#[derive(Clone, Debug)]
pub struct GeminiResponse {
    pub timestamp: chrono::DateTime<chrono::Local>,
    pub prompt_summary: String, // 要約元となったコメントのプレビュー
    pub response: String,
    pub is_error: bool,
}

pub struct GeminiTaskResult {
    pub result: Result<String, String>,
    pub prompt_summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Voice,
    Ai,
    Obs,
    Filter,
    SoundEffect,
    Other,
}

pub struct App {
    // 接続設定
    video_id_input: String,
    is_connected: bool,

    // チャット
    chat_tx: Option<tokio::task::JoinHandle<()>>,
    chat_event_rx: Option<mpsc::Receiver<ChatEvent>>,
    messages: Vec<ChatMessage>,
    status_text: String,
    auto_scroll: bool,

    // CeVIO AI
    tts_enabled: bool,
    narrators: Vec<String>,
    selected_narrator: String,
    emotions: Vec<String>,
    cevio_tx: Option<mpsc::Sender<CevioCommand>>,

    // 音声パラメータ
    speed: u32,
    pitch: u32,
    volume: u32,
    alpha: u32,
    intonation: u32,
    emotion_values: HashMap<String, u32>,
    skip_threshold: u32,

    // 辞書ウィンドウ
    show_dict_window: bool,
    dict_entries: Vec<DictEntry>,
    dict_pattern_input: String,
    dict_replacement_input: String,
    dict_is_regex: bool,

    // 設定
    settings: Settings,
    left_panel_width: f32,

    // ログ・メモ機能
    video_title: String,
    editing_user_note: Option<(String, String)>, // (author_id, name)
    note_input: String,
    show_user_management_window: bool,
    user_search_query: String,

    // NG設定用
    ng_word_input: String,
    ng_user_input: String,

    // 効果音設定用
    se_pattern_input: String,
    se_file_input: String,
    se_volume_input: f32,
    audio_output: Option<(OutputStream, OutputStreamHandle)>,

    // Gemini連携用
    gemini_responses: Vec<GeminiResponse>,
    gemini_tx: mpsc::Sender<GeminiTaskResult>,
    gemini_rx: mpsc::Receiver<GeminiTaskResult>,
    last_gemini_sent_comment_id: Option<String>,
    last_gemini_sent_time: Option<std::time::Instant>,
    is_waiting_gemini: bool,
    gemini_scroll_to_bottom: bool,

    // OBS Web Server用
    web_server_state: Arc<std::sync::Mutex<web_server::WebServerState>>,
    web_server_shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    last_web_server_port: u16,
    last_web_server_enabled: bool,

    // 表示タブの管理
    active_tab: SettingsTab,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // ダークテーマ設定
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = Self::dark_theme();
        
        // ボタンとウィジェットのサイズを調整
        style.spacing.button_padding = vec2(12.0, 6.0);
        style.spacing.interact_size.y = 32.0;            // 最小の高さをボタンと合わせる
        
        cc.egui_ctx.set_style(style);

        // 日本語フォント読み込み
        Self::load_japanese_font(&cc.egui_ctx);

        let settings = settings::load();
        let dict_entries = load_dictionary();

        // CeVIOナレーター取得（バックグラウンドスレッドで実行済み前提）
        let narrators = get_narrators();
        let selected_narrator = if narrators.contains(&settings.narrator) {
            settings.narrator.clone()
        } else {
            narrators.first().cloned().unwrap_or_default()
        };

        let emotions = if !selected_narrator.is_empty() {
            get_emotions(&selected_narrator)
        } else {
            vec![]
        };

        let profile = settings
            .profiles
            .get(&selected_narrator)
            .cloned()
            .unwrap_or_else(NarratorProfile::default_values);

        let mut emotion_values = HashMap::new();
        for e in &emotions {
            let v = profile.emotions.get(e).copied().unwrap_or(0);
            emotion_values.insert(e.clone(), v);
        }

        let (gemini_tx, gemini_rx) = mpsc::channel(10);

        let obs_server_port = settings.obs_server_port;
        let obs_server_enabled = settings.obs_server_enabled;
        let gemini_name = settings.gemini_name.clone();
        let gemini_enabled = settings.gemini_enabled;

        let mut app = Self {
            video_id_input: settings.video_id.clone(),
            is_connected: false,
            chat_tx: None,
            chat_event_rx: None,
            messages: Vec::new(),
            status_text: "接続待機中".to_string(),
            auto_scroll: true,

            tts_enabled: settings.tts_enabled,
            narrators,
            selected_narrator,
            emotions,
            cevio_tx: None,

            speed: profile.speed,
            pitch: profile.pitch,
            volume: profile.volume,
            alpha: profile.alpha,
            intonation: profile.intonation,
            emotion_values,
            skip_threshold: settings.skip_threshold,

            show_dict_window: false,
            dict_entries,
            dict_pattern_input: String::new(),
            dict_replacement_input: String::new(),
            dict_is_regex: false,

            settings,
            left_panel_width: 320.0,

            video_title: String::new(),
            editing_user_note: None,
            note_input: String::new(),
            show_user_management_window: false,
            user_search_query: String::new(),
            ng_word_input: String::new(),
            ng_user_input: String::new(),
            se_pattern_input: String::new(),
            se_file_input: String::new(),
            se_volume_input: 1.0,
            audio_output: OutputStream::try_default().ok(),

            gemini_responses: Vec::new(),
            gemini_tx,
            gemini_rx,
            last_gemini_sent_comment_id: None,
            last_gemini_sent_time: None,
            is_waiting_gemini: false,
            gemini_scroll_to_bottom: false,

            web_server_state: Arc::new(std::sync::Mutex::new(web_server::WebServerState {
                latest_response: "まだGeminiの回答はありません。".to_string(),
                latest_timestamp: String::new(),
                gemini_name,
                comments: Vec::new(),
                gemini_enabled,
            })),
            web_server_shutdown_tx: None,
            last_web_server_port: obs_server_port,
            last_web_server_enabled: obs_server_enabled,
            active_tab: SettingsTab::Voice,
        };

        // WebServerを開始
        if app.settings.obs_server_enabled {
            let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
            app.web_server_shutdown_tx = Some(shutdown_tx);
            let state = app.web_server_state.clone();
            let port = app.settings.obs_server_port;
            tokio::spawn(async move {
                web_server::start_obs_web_server(port, state, shutdown_rx).await;
            });
        }

        // CeVIOスレッドを開始
        app.cevio_tx = Some(start_cevio_thread(app.build_cevio_params()));

        // スタイル適用
        cc.egui_ctx.set_visuals(Self::dark_theme());
        cc.egui_ctx.set_style(Style {
            spacing: Self::custom_spacing(),
            ..Default::default()
        });

        app
    }

    fn dark_theme() -> Visuals {
        let mut v = Visuals::dark();
        v.panel_fill = Color32::from_rgb(20, 20, 24);
        v.window_fill = Color32::from_rgb(28, 28, 34);
        v.extreme_bg_color = Color32::from_rgb(12, 12, 16);
        v.faint_bg_color = Color32::from_rgb(30, 30, 38);
        v.widgets.noninteractive.bg_fill = Color32::from_rgb(35, 35, 42);
        v.widgets.inactive.bg_fill = Color32::from_rgb(55, 55, 65); // スライダーの溝などを明るく
        v.widgets.hovered.bg_fill = Color32::from_rgb(70, 70, 85);
        v.widgets.active.bg_fill = Color32::from_rgb(70, 100, 160);
        v.widgets.active.expansion = 1.0; // ノブの膨らみを抑える
        v.widgets.hovered.expansion = 1.0;
        v.widgets.inactive.expansion = 0.0;
        
        v.selection.bg_fill = Color32::from_rgb(50, 90, 150);
        v
    }

    fn custom_spacing() -> Spacing {
        let mut s = Spacing::default();
        s.slider_width = 160.0;
        s.interact_size.y = 22.0;
        s.button_padding = vec2(12.0, 7.0);
        s.item_spacing = vec2(10.0, 10.0); // 余白を広げる
        s
    }

    fn load_japanese_font(ctx: &Context) {
        let font_paths = [
            r"C:\Windows\Fonts\YuGothM.ttc",
            r"C:\Windows\Fonts\msgothic.ttc",
            r"C:\Windows\Fonts\meiryo.ttc",
        ];
        for path in &font_paths {
            if let Ok(data) = std::fs::read(path) {
                let mut font_data = FontData::from_owned(data);
                // 日本語フォントが上に寄りやすいため、下にずらす(y_offset)
                // 入力欄でのバランスを考慮し、0.16 に設定
                font_data.tweak = FontTweak {
                    y_offset_factor: 0.16, // 正の値で下に移動
                    ..Default::default()
                };
                
                let mut fonts = FontDefinitions::default();
                fonts.font_data.insert(
                    "jp_font".to_owned(),
                    font_data,
                );
                // デフォルトのサイズを少し大きく設定
                fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "jp_font".to_owned());
                fonts.families.get_mut(&FontFamily::Monospace).unwrap().push("jp_font".to_owned());
                
                // 各テキストスタイルのサイズを底上げ
                let mut style = (*ctx.style()).clone();
                style.text_styles.insert(TextStyle::Body, FontId::new(15.0, FontFamily::Proportional));
                style.text_styles.insert(TextStyle::Button, FontId::new(15.0, FontFamily::Proportional));
                style.text_styles.insert(TextStyle::Heading, FontId::new(20.0, FontFamily::Proportional));
                style.text_styles.insert(TextStyle::Small, FontId::new(13.0, FontFamily::Proportional));
                ctx.set_style(style);

                ctx.set_fonts(fonts);
                break;
            }
        }
    }

    fn get_new_comments_for_gemini(&self) -> Vec<ChatMessage> {
        let mut result = Vec::new();
        let mut found_last_sent = self.last_gemini_sent_comment_id.is_none();

        for msg in &self.messages {
            if !found_last_sent {
                if let Some(ref last_id) = self.last_gemini_sent_comment_id {
                    if &msg.id == last_id {
                        found_last_sent = true;
                    }
                }
                continue;
            }
            result.push(msg.clone());
        }

        if result.len() > 30 {
            let len = result.len();
            result = result.split_off(len - 30);
        }

        result
    }

    fn send_comments_to_gemini(&mut self, comments: Vec<ChatMessage>) {
        if self.settings.gemini_api_key.trim().is_empty() {
            return;
        }

        self.is_waiting_gemini = true;
        self.last_gemini_sent_time = Some(std::time::Instant::now());

        if let Some(last_msg) = comments.last() {
            self.last_gemini_sent_comment_id = Some(last_msg.id.clone());
        }

        let api_key = self.settings.gemini_api_key.clone();
        let model = self.settings.gemini_model.clone();
        let system_prompt = self.settings.gemini_system_prompt.clone();

        // 要約元のコメントプレビューを作成
        let prompt_summary = if comments.len() == 1 {
            format!("{}: {}", comments[0].author, comments[0].message)
        } else {
            format!("{} 件のコメント ({} 〜 {})", comments.len(), comments[0].author, comments.last().unwrap().author)
        };

        // コメントを結合
        let mut prompt_text = String::new();
        if !self.video_title.is_empty() {
            prompt_text.push_str(&format!("現在の配信タイトル: {}\n\n", self.video_title));
        }
        prompt_text.push_str("以下はリスナーからの新着コメントです。これらに基づいて回答してください:\n");
        for msg in &comments {
            prompt_text.push_str(&format!("{}: {}\n", msg.author, msg.message));
        }

        let tx = self.gemini_tx.clone();

        tokio::spawn(async move {
            let res = crate::gemini::query_gemini(&api_key, &model, &system_prompt, &prompt_text).await;
            let _ = tx.send(GeminiTaskResult {
                result: res,
                prompt_summary,
            }).await;
        });
    }

    fn poll_gemini_responses(&mut self) {
        while let Ok(task_res) = self.gemini_rx.try_recv() {
            self.is_waiting_gemini = false;
            match task_res.result {
                Ok(text) => {
                    let response_text = text.trim().to_string();
                    
                    // WebServer側の状態を更新
                    if let Ok(mut state) = self.web_server_state.lock() {
                        state.latest_response = response_text.clone();
                        state.latest_timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                    }

                    self.gemini_responses.push(GeminiResponse {
                        timestamp: chrono::Local::now(),
                        prompt_summary: task_res.prompt_summary,
                        response: response_text.clone(),
                        is_error: false,
                    });
                    self.gemini_scroll_to_bottom = true;

                    // CeVIOで読み上げる
                    if self.tts_enabled {
                        if let Some(tx) = &self.cevio_tx {
                            // clean_textをかける
                            let clean_resp = clean_text(
                                &response_text, 
                                &self.dict_entries, 
                                300, 
                                &self.settings.read_more_text
                            );
                            if !clean_resp.is_empty() {
                                let _ = tx.try_send(CevioCommand::Speak { text: clean_resp });
                            }
                        }
                    }
                }
                Err(e) => {
                    self.gemini_responses.push(GeminiResponse {
                        timestamp: chrono::Local::now(),
                        prompt_summary: task_res.prompt_summary,
                        response: format!("エラー: {}", e),
                        is_error: true,
                    });
                    self.gemini_scroll_to_bottom = true;
                    self.status_text = format!("Geminiエラー: {}", e);
                }
            }
        }
    }

    fn handle_web_server_lifecycle(&mut self) {
        let current_enabled = self.settings.obs_server_enabled;
        let current_port = self.settings.obs_server_port;

        if current_enabled != self.last_web_server_enabled || current_port != self.last_web_server_port {
            // 設定が変更された場合、既存のサーバーがあれば終了する
            if let Some(shutdown_tx) = self.web_server_shutdown_tx.take() {
                let _ = shutdown_tx.send(());
            }

            // 新たに有効化される場合、サーバーを起動する
            if current_enabled {
                let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
                self.web_server_shutdown_tx = Some(shutdown_tx);
                let state = self.web_server_state.clone();
                tokio::spawn(async move {
                    web_server::start_obs_web_server(current_port, state, shutdown_rx).await;
                });
            }

            self.last_web_server_enabled = current_enabled;
            self.last_web_server_port = current_port;
        }
    }

    fn connect(&mut self) {
        if self.video_id_input.trim().is_empty() {
            self.status_text = "動画IDまたはURLを入力してください".to_string();
            return;
        }
        self.save_settings();

        let (tx, rx) = mpsc::channel::<ChatEvent>(200);
        self.chat_event_rx = Some(rx);

        let video_id = self.video_id_input.clone();
        let handle = tokio::spawn(async move {
            start_chat_monitor(video_id, tx).await;
        });
        self.chat_tx = Some(handle);
        self.is_connected = true;
        self.messages.clear();
        self.status_text = "接続中...".to_string();

        // Gemini状態リセット
        self.last_gemini_sent_comment_id = None;
        self.last_gemini_sent_time = Some(std::time::Instant::now());
        self.is_waiting_gemini = false;
        self.gemini_responses.clear();

        // OBSコメント履歴リセット
        if let Ok(mut state) = self.web_server_state.lock() {
            state.comments.clear();
        }

        // CeVIO AIスレッド起動・更新
        if self.tts_enabled {
            let params = self.build_cevio_params();
            if let Some(tx) = &self.cevio_tx {
                let _ = tx.try_send(CevioCommand::UpdateParams(params));
            } else {
                self.cevio_tx = Some(start_cevio_thread(params));
            }
        }
    }

    fn disconnect(&mut self) {
        self.save_settings();
        self.is_connected = false;
        self.status_text = "切断しました".to_string();

        self.last_gemini_sent_comment_id = None;
        self.last_gemini_sent_time = None;
        self.is_waiting_gemini = false;

        if let Some(handle) = self.chat_tx.take() {
            handle.abort();
        }
        self.chat_event_rx = None;

        if let Some(tx) = &self.cevio_tx {
            let _ = tx.try_send(CevioCommand::Stop);
        }

        self.save_logs();
    }

    fn get_log_dir(&self) -> std::path::PathBuf {
        if !self.settings.log_dir.trim().is_empty() {
            let path = std::path::PathBuf::from(&self.settings.log_dir);
            if path.exists() || std::fs::create_dir_all(&path).is_ok() {
                return path;
            }
        }
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        exe_dir.join("logs")
    }

    fn save_logs(&self) {
        if self.messages.is_empty() { return; }

        let log_dir = self.get_log_dir();
        let _ = std::fs::create_dir_all(&log_dir);

        let now = chrono::Local::now();
        let filename = format!("log_{}.md", now.format("%Y%m%d_%H%M%S"));
        let path = log_dir.join(filename);

        let mut content = format!("# 配信ログ\n\n");
        content.push_str(&format!("- **配信タイトル**: {}\n", self.video_title));
        content.push_str(&format!("- **動画ID/URL**: {}\n", self.video_id_input));
        content.push_str(&format!("- **保存日時**: {}\n\n", now.format("%Y-%m-%d %H:%M:%S")));
        content.push_str("---\n\n");

        for msg in &self.messages {
            let time = msg.timestamp.format("%H:%M:%S");
            content.push_str(&format!("- [{}] [{}] **{}**: {}\n", time, msg.author_id, msg.author, msg.message));
        }

        let _ = std::fs::write(path, content);
    }

    fn load_log_file(&mut self) {
        let log_dir = self.get_log_dir();

        if let Some(path) = rfd::FileDialog::new()
            .set_directory(&log_dir)
            .add_filter("Markdown files", &["md"])
            .add_filter("Text files", &["txt"])
            .pick_file() 
        {
            if let Ok(content) = std::fs::read_to_string(path) {
                self.messages.clear();
                self.is_connected = false; // ログ表示中は接続を切断状態にする
                self.video_title = "(ログ再生中)".to_string();

                let mut in_logs = false;
                for line in content.lines() {
                    let trimmed_line = line.trim();
                    if trimmed_line.starts_with("---") {
                        in_logs = true;
                        continue;
                    }
                    if !in_logs {
                        if let Some(title) = trimmed_line.strip_prefix("配信タイトル: ") {
                            self.video_title = format!("(ログ) {}", title);
                        } else if let Some(title) = trimmed_line.strip_prefix("- **配信タイトル**: ") {
                            self.video_title = format!("(ログ) {}", title);
                        }
                        continue;
                    }

                    // Markdown形式 (先頭が "- [" または "* [") のトリム処理
                    let is_md_line = trimmed_line.starts_with("- [") || trimmed_line.starts_with("* [");
                    let clean_line = if is_md_line {
                        &trimmed_line[2..]
                    } else {
                        trimmed_line
                    };

                    // 形式: [HH:MM:SS] [ID] Author: Message または **Author**: Message
                    if clean_line.starts_with('[') && clean_line.contains("] ") {
                        let parts: Vec<&str> = clean_line.splitn(3, "] ").collect();
                        if parts.len() == 3 {
                            let time_str = parts[0].trim_start_matches('[');
                            let author_id = parts[1].trim_start_matches('[');
                            let rest = parts[2];
                            
                            if let Some(sep_pos) = rest.find(": ") {
                                let mut author = &rest[..sep_pos];
                                let message = &rest[sep_pos + 2..];
                                
                                // Markdownの強調表示 "**" のトリム
                                if author.starts_with("**") && author.ends_with("**") && author.len() > 4 {
                                    author = &author[2..author.len() - 2];
                                }
                                
                                // 時刻のパース
                                let now = chrono::Local::now();
                                let hms: Vec<&str> = time_str.split(':').collect();
                                let timestamp = if hms.len() == 3 {
                                    let h = hms[0].parse().unwrap_or(0);
                                    let m = hms[1].parse().unwrap_or(0);
                                    let s = hms[2].parse().unwrap_or(0);
                                    now.date_naive().and_hms_opt(h, m, s)
                                        .and_then(|dt| dt.and_local_timezone(chrono::Local).single())
                                        .unwrap_or(now)
                                } else {
                                    now
                                };

                                self.messages.push(ChatMessage {
                                    id: format!("log_{}", self.messages.len()),
                                    author: author.to_string(),
                                    author_id: author_id.to_string(),
                                    message: message.to_string(),
                                    timestamp,
                                    is_owner: false,
                                    is_moderator: false,
                                    is_member: false,
                                });
                            }
                        } else if parts.len() == 2 {
                            // 旧形式 ([HH:MM:SS] Author: Message) への互換性維持
                            let _time_str = parts[0].trim_start_matches('[');
                            let rest = parts[1];
                            
                            if let Some(sep_pos) = rest.find(": ") {
                                let author = &rest[..sep_pos];
                                let message = &rest[sep_pos + 2..];
                                
                                let now = chrono::Local::now();
                                self.messages.push(ChatMessage {
                                    id: format!("log_{}", self.messages.len()),
                                    author: author.to_string(),
                                    author_id: String::new(),
                                    message: message.to_string(),
                                    timestamp: now,
                                    is_owner: false,
                                    is_moderator: false,
                                    is_member: false,
                                });
                            }
                        }
                    }
                }
                self.status_text = format!("ログを読み込みました ({} 件)", self.messages.len());
            }
        }
    }

    fn build_cevio_params(&self) -> CevioParams {
        CevioParams {
            narrator: if self.selected_narrator.is_empty() {
                None
            } else {
                Some(self.selected_narrator.clone())
            },
            speed: self.speed,
            pitch: self.pitch,
            volume: self.volume,
            alpha: self.alpha,
            intonation: self.intonation,
            emotions: self.emotion_values.clone(),
            skip_threshold: self.skip_threshold,
        }
    }

    fn poll_chat_events(&mut self) {
        // eprintln!("DEBUG: poll_chat_events called, tts_enabled={}, has_tx={}", self.tts_enabled, self.cevio_tx.is_some());
        let mut new_messages = Vec::new();
        if let Some(rx) = &mut self.chat_event_rx {
            let mut ended = false;
            loop {
                match rx.try_recv() {
                    Ok(ChatEvent::Title(title)) => {
                        self.video_title = title;
                    }
                    Ok(ChatEvent::Message(msg)) => new_messages.push(msg),
                    Ok(ChatEvent::Error(e)) => {
                        self.status_text = format!("エラー: {}", e);
                    }
                    Ok(ChatEvent::Ended) => {
                        self.status_text = "配信が終了しました".to_string();
                        self.is_connected = false;
                        ended = true;
                    }
                    Err(_) => break,
                }
            }
            if ended {
                self.save_logs();
            }
        }
        for msg in new_messages {
            // スキップ判定
            let mut skip = false;

            // 1. NGユーザー (ユーザー指定NGは完全にスキップ)
            if self.settings.ng_users.contains(&msg.author_id) {
                skip = true;
            }

            // 効果音の再生チェック
            for se in &self.settings.se_entries {
                if msg.message.contains(&se.pattern) {
                    self.play_se(&se.file_path, se.volume);
                }
            }

            // (NGワードはスキップせず、読み上げ内容を置換する処理を後続で行う)

            if skip {
                // 読み上げはスキップするが、表示は行われる（必要なら表示もスキップできるが、一旦読み上げのみ）
                continue;
            }

            // CeVIOで読み上げ
            if self.tts_enabled {
                if let Some(tx) = &self.cevio_tx {
                    // NGワード判定によるテキスト置換
                    let mut has_ng_word = false;
                    for word in &self.settings.ng_words {
                        if msg.message.contains(word) {
                            has_ng_word = true;
                            break;
                        }
                    }

                    let read_text = if has_ng_word {
                        self.settings.ng_replacement_text.clone()
                    } else {
                        msg.message.clone()
                    };

                    let text = clean_text(
                        &read_text, 
                        &self.dict_entries, 
                        self.settings.max_read_length,
                        &self.settings.read_more_text
                    );
                    if !text.is_empty() {
                        self.status_text = format!("送信中: {}", text);
                        if let Err(e) = tx.try_send(CevioCommand::Speak { text }) {
                            self.status_text = format!("送信失敗: {:?}", e);
                        }
                    } else {
                        self.status_text = "テキスト空".to_string();
                    }
                } else {
                    self.status_text = "TX欠損".to_string();
                }
            }
            // OBS配信用サーバーのステート更新
            let obs_msg = web_server::ObsComment {
                id: msg.id.clone(),
                author: msg.author.clone(),
                message: msg.message.clone(),
                timestamp: msg.timestamp.format("%H:%M:%S").to_string(),
                is_owner: msg.is_owner,
                is_moderator: msg.is_moderator,
                is_member: msg.is_member,
            };
            if let Ok(mut state) = self.web_server_state.lock() {
                state.comments.push(obs_msg);
                if state.comments.len() > 50 {
                    state.comments.remove(0);
                }
            }

            self.messages.push(msg);
            // メッセージ上限
            if self.messages.len() > 500 {
                self.messages.remove(0);
            }
        }

        if self.is_connected {
            // 「送信中」や「失敗」などの特殊な表示がない場合のみ、ステータスを表示する
            if !self.status_text.contains(":") && !self.status_text.contains("欠損") && !self.status_text.contains("空") {
                if self.video_title.is_empty() {
                    self.status_text = format!("配信中 — {} コメント", self.messages.len());
                } else {
                    self.status_text = format!("{} — {} コメント", self.video_title, self.messages.len());
                }
            }
        }
    }

    fn save_settings(&mut self) {
        self.settings.video_id = self.video_id_input.clone();
        self.settings.narrator = self.selected_narrator.clone();
        self.settings.tts_enabled = self.tts_enabled;
        self.settings.skip_threshold = self.skip_threshold;

        let profile = NarratorProfile {
            speed: self.speed,
            pitch: self.pitch,
            volume: self.volume,
            alpha: self.alpha,
            intonation: self.intonation,
            emotions: self.emotion_values.clone(),
        };
        if !self.selected_narrator.is_empty() {
            self.settings
                .profiles
                .insert(self.selected_narrator.clone(), profile);
        }
        settings::save(&self.settings);
    }

    fn on_narrator_changed(&mut self) {
        self.emotions = if !self.selected_narrator.is_empty() {
            get_emotions(&self.selected_narrator)
        } else {
            vec![]
        };

        let profile = self
            .settings
            .profiles
            .get(&self.selected_narrator)
            .cloned()
            .unwrap_or_else(NarratorProfile::default_values);

        self.speed = profile.speed;
        self.pitch = profile.pitch;
        self.volume = profile.volume;
        self.alpha = profile.alpha;
        self.intonation = profile.intonation;

        self.emotion_values.clear();
        for e in &self.emotions {
            let v = profile.emotions.get(e).copied().unwrap_or(0);
            self.emotion_values.insert(e.clone(), v);
        }
    }

    // ============================================================
    // UI描画
    // ============================================================

    fn draw_top_bar(&mut self, ui: &mut Ui) {
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.label(RichText::new("動画ID / URL:").color(COLOR_NORMAL));
            
            let available_w = ui.available_width();
            let edit_w = (available_w - 110.0).max(100.0);
            
            // 入力欄の背景と枠をカスタムFrameで作成
            let response = Frame::none()
                .fill(ui.visuals().extreme_bg_color)
                .stroke(ui.visuals().widgets.inactive.bg_stroke)
                .rounding(4.0)
                .inner_margin(Margin { left: 8.0, right: 8.0, top: 5.0, bottom: 5.0 }) // 余白を均等にして垂直中央へ
                .show(ui, |ui| {
                    ui.add_sized(
                        [edit_w - 16.0, 22.0], // 高さを微増
                        TextEdit::singleline(&mut self.video_id_input)
                            .frame(false)
                            .hint_text("https://www.youtube.com/watch?v=... または動画ID"),
                    )
                }).inner;

            if self.is_connected {
                if ui
                    .add(Button::new(
                        RichText::new(" ⏹ 切断 ").color(Color32::WHITE),
                    ).fill(Color32::from_rgb(180, 50, 50)).min_size(vec2(80.0, 32.0)))
                    .clicked()
                {
                    self.disconnect();
                }
            } else {
                if ui
                    .add(Button::new(
                        RichText::new(" ▶ 接続 ").color(Color32::WHITE),
                    ).fill(Color32::from_rgb(40, 130, 60)).min_size(vec2(80.0, 32.0)))
                    .clicked()
                    || (response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
                {
                    self.connect();
                }
            }
        });
    }

    fn draw_left_panel(&mut self, ui: &mut Ui) {
        // タブの選択UI
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            
            ui.selectable_value(&mut self.active_tab, SettingsTab::Voice, "🎙 音声");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Ai, "🤖 AI");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Obs, "📺 配信");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Filter, "🛡 フィルタ");
            ui.selectable_value(&mut self.active_tab, SettingsTab::SoundEffect, "📢 効果音");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Other, "⚙ その他");
        });
        ui.separator();
        ui.add_space(4.0);

        ScrollArea::vertical().show(ui, |ui| {
            match self.active_tab {
                SettingsTab::Voice => self.draw_voice_settings(ui),
                SettingsTab::Ai => self.draw_ai_settings(ui),
                SettingsTab::Obs => self.draw_obs_settings(ui),
                SettingsTab::Filter => self.draw_filter_settings(ui),
                SettingsTab::SoundEffect => self.draw_se_settings(ui),
                SettingsTab::Other => self.draw_other_settings(ui),
            }
        });
    }

    fn draw_voice_settings(&mut self, ui: &mut Ui) {
        // CeVIO AI 設定グループ
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("🔊 CeVIO AI 読み上げ").strong().color(Color32::from_rgb(100, 180, 255)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.checkbox(&mut self.tts_enabled, "");
                });
            });
            ui.separator();

            ui.add_enabled_ui(self.tts_enabled, |ui| {
                // ナレーター選択
                ui.label("ナレーター:");
                let prev = self.selected_narrator.clone();
                ComboBox::from_id_salt("narrator_combo")
                    .selected_text(if self.selected_narrator.is_empty() {
                        "選択してください"
                    } else {
                        &self.selected_narrator
                    })
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for n in self.narrators.clone() {
                            ui.selectable_value(&mut self.selected_narrator, n.clone(), &n);
                        }
                    });
                
                let mut params_changed = false;
                if self.selected_narrator != prev {
                    self.on_narrator_changed();
                    params_changed = true;
                }

                ui.add_space(6.0);

                // パラメータ
                Grid::new("params_grid")
                    .num_columns(3)
                    .spacing([12.0, 14.0]) // 垂直間隔を少し広げる
                    .min_col_width(90.0)   // 1列目（ラベル）の幅を確保
                    .show(ui, |ui| {
                        ui.label(RichText::new("速さ").color(COLOR_NORMAL));
                        params_changed |= Self::param_slider_raw(ui, &mut self.speed, 0, 100);
                        ui.end_row();

                        ui.label(RichText::new("高さ").color(COLOR_NORMAL));
                        params_changed |= Self::param_slider_raw(ui, &mut self.pitch, 0, 100);
                        ui.end_row();

                        ui.label(RichText::new("大きさ").color(COLOR_NORMAL));
                        params_changed |= Self::param_slider_raw(ui, &mut self.volume, 0, 100);
                        ui.end_row();

                        ui.label(RichText::new("声質").color(COLOR_NORMAL));
                        params_changed |= Self::param_slider_raw(ui, &mut self.alpha, 0, 100);
                        ui.end_row();

                        ui.label(RichText::new("抑揚").color(COLOR_NORMAL));
                        params_changed |= Self::param_slider_raw(ui, &mut self.intonation, 0, 100);
                        ui.end_row();

                        ui.label(RichText::new("最大待ち件数").color(COLOR_NORMAL));
                        params_changed |= Self::param_slider_raw(ui, &mut self.skip_threshold, 0, 20);
                        ui.end_row();
                    });

                ui.add_space(4.0);
                ui.label(RichText::new("※これを超えると古いコメントをスキップします (0で無効)").small().color(COLOR_TIME));

                // 感情パラメータ
                if !self.emotions.is_empty() {
                    ui.separator();
                    ui.label(RichText::new("感情パラメータ:").color(COLOR_TIME));
                    
                    Grid::new("emotions_grid")
                        .num_columns(3)
                        .spacing([12.0, 14.0])
                        .min_col_width(90.0)
                        .show(ui, |ui| {
                            for emotion in self.emotions.clone() {
                                ui.label(RichText::new(&emotion).color(COLOR_NORMAL));
                                let val = self.emotion_values.entry(emotion.clone()).or_insert(0);
                                params_changed |= Self::param_slider_raw(ui, val, 0, 100);
                                ui.end_row();
                            }
                        });
                }

                if params_changed {
                    if let Some(tx) = &self.cevio_tx {
                        let params = self.build_cevio_params();
                        let _ = tx.try_send(CevioCommand::UpdateParams(params));
                    }
                }
            });
        });
    }

    fn draw_ai_settings(&mut self, ui: &mut Ui) {
        // Gemini AI 設定グループ
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("🤖 Gemini 連携").strong().color(Color32::from_rgb(180, 120, 255)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.checkbox(&mut self.settings.gemini_enabled, "").changed() {
                        self.save_settings();
                        if let Ok(mut state) = self.web_server_state.lock() {
                            state.gemini_enabled = self.settings.gemini_enabled;
                        }
                    }
                });
            });
            ui.separator();

            ui.add_enabled_ui(self.settings.gemini_enabled, |ui| {
                ui.label("API キー:");
                let mut api_key = self.settings.gemini_api_key.clone();
                let response = ui.add(
                    TextEdit::singleline(&mut api_key)
                        .password(true)
                        .hint_text("AI StudioのAPIキーを入力")
                        .desired_width(ui.available_width()),
                );
                if response.changed() {
                    self.settings.gemini_api_key = api_key;
                    self.save_settings();
                }

                ui.add_space(4.0);

                ui.label("モデル:");
                let current_model = self.settings.gemini_model.clone();
                let model_options = vec![
                    "gemini-2.5-flash",
                    "gemini-2.5-flash-lite",
                    "gemini-2.0-flash",
                    "gemini-1.5-flash",
                    "gemini-1.5-flash-8b",
                ];
                
                let mut selected_option = if model_options.contains(&current_model.as_str()) {
                    current_model.clone()
                } else {
                    "手動入力".to_string()
                };

                let prev_option = selected_option.clone();
                ComboBox::from_id_salt("gemini_model_combo")
                    .selected_text(if selected_option == "手動入力" {
                        "手動入力（カスタム）"
                    } else {
                        &selected_option
                    })
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for opt in &model_options {
                            ui.selectable_value(&mut selected_option, opt.to_string(), *opt);
                        }
                        ui.selectable_value(&mut selected_option, "手動入力".to_string(), "手動入力（カスタム）");
                    });

                if selected_option != prev_option {
                    if selected_option != "手動入力" {
                        self.settings.gemini_model = selected_option.clone();
                        self.save_settings();
                    }
                }

                if selected_option == "手動入力" {
                    ui.add_space(2.0);
                    let mut custom_model = if model_options.contains(&self.settings.gemini_model.as_str()) {
                        String::new()
                    } else {
                        self.settings.gemini_model.clone()
                    };
                    
                    let response = ui.add(
                        TextEdit::singleline(&mut custom_model)
                            .hint_text("任意のモデル名を入力")
                            .desired_width(ui.available_width()),
                    );
                    if response.changed() {
                        self.settings.gemini_model = custom_model;
                        self.save_settings();
                    }
                }

                ui.add_space(4.0);

                ui.label("インターバル (秒):");
                let mut interval = self.settings.gemini_interval_secs;
                if ui.add(Slider::new(&mut interval, 10..=300).suffix("秒")).changed() {
                    self.settings.gemini_interval_secs = interval;
                    self.save_settings();
                }

                ui.add_space(4.0);

                ui.label("システムプロンプト:");
                let mut sys_prompt = self.settings.gemini_system_prompt.clone();
                let response = ui.add(
                    TextEdit::multiline(&mut sys_prompt)
                        .desired_rows(3)
                        .desired_width(ui.available_width())
                        .hint_text("AIへのシステム指示を入力"),
                );
                if response.changed() {
                    self.settings.gemini_system_prompt = sys_prompt;
                    self.save_settings();
                }
            });
        });
    }

    fn draw_obs_settings(&mut self, ui: &mut Ui) {
        // OBS配信用サーバー設定
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("📺 OBS 配信表示連携").strong().color(Color32::from_rgb(50, 180, 255)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.checkbox(&mut self.settings.obs_server_enabled, "").changed() {
                        self.save_settings();
                    }
                });
            });
            ui.separator();

            ui.add_enabled_ui(self.settings.obs_server_enabled, |ui| {
                ui.horizontal(|ui| {
                    ui.label("表示名:");
                    let mut name = self.settings.gemini_name.clone();
                    if ui.text_edit_singleline(&mut name).changed() {
                        self.settings.gemini_name = name.clone();
                        self.save_settings();
                        if let Ok(mut state) = self.web_server_state.lock() {
                            state.gemini_name = name;
                        }
                    }
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label("ポート番号:");
                    let mut port = self.settings.obs_server_port;
                    if ui.add(egui::DragValue::new(&mut port).range(1024..=65535)).changed() {
                        self.settings.obs_server_port = port;
                        self.save_settings();
                    }
                });

                ui.add_space(4.0);

                let url = format!("http://localhost:{}", self.settings.obs_server_port);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("配信URL:").small().color(COLOR_TIME));
                    ui.label(RichText::new(&url).small().underline().color(Color32::from_rgb(50, 180, 255)));
                });

                ui.add_space(2.0);

                if ui.button("📋 OBS用URLをコピー").clicked() {
                    ui.ctx().copy_text(url);
                }
            });
        });
    }

    fn draw_filter_settings(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.button("📖 辞書登録").clicked() {
                self.show_dict_window = true;
            }
            if ui.button("👤 ユーザー管理").clicked() {
                self.show_user_management_window = true;
            }
        });

        ui.add_space(6.0);

        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("🚫 NG / フィルタ設定").strong().color(Color32::from_rgb(255, 120, 120)));
            ui.separator();

            ui.label(RichText::new("NGワード").small().color(COLOR_TIME));
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.ng_word_input);
                if ui.button("追加").clicked() && !self.ng_word_input.is_empty() {
                    self.settings.ng_words.push(self.ng_word_input.clone());
                    self.ng_word_input.clear();
                    self.save_settings();
                }
            });
            
            // NGワードリスト
            let mut remove_idx = None;
            for (i, word) in self.settings.ng_words.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(word);
                    if ui.small_button("x").clicked() {
                        remove_idx = Some(i);
                    }
                });
            }
            if let Some(i) = remove_idx {
                self.settings.ng_words.remove(i);
                self.save_settings();
            }

            ui.add_space(4.0);
            ui.label(RichText::new("NGワード検出時の代わりの文言").small().color(COLOR_TIME));
            if ui.text_edit_singleline(&mut self.settings.ng_replacement_text).lost_focus() {
                self.save_settings();
            }

            ui.separator();
            ui.label(RichText::new("最大読み上げ文字数").small().color(COLOR_TIME));
            ui.horizontal(|ui| {
                if ui.add(DragValue::new(&mut self.settings.max_read_length).range(10..=500)).changed() {
                    self.save_settings();
                }
                ui.label("文字で省略");
            });
            
            ui.label(RichText::new("省略時に追加する文言").small().color(COLOR_TIME));
            if ui.text_edit_singleline(&mut self.settings.read_more_text).lost_focus() {
                self.save_settings();
            }

            ui.separator();
            ui.label(RichText::new("NGユーザーID").small().color(COLOR_TIME));
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.ng_user_input);
                if ui.button("追加").clicked() && !self.ng_user_input.is_empty() {
                    self.settings.ng_users.push(self.ng_user_input.clone());
                    self.ng_user_input.clear();
                    self.save_settings();
                }
            });
            // NGユーザーリスト
            let mut remove_user_idx = None;
            for (i, user_id) in self.settings.ng_users.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(user_id).small());
                    if ui.small_button("x").clicked() {
                        remove_user_idx = Some(i);
                    }
                });
            }
            if let Some(i) = remove_user_idx {
                self.settings.ng_users.remove(i);
                self.save_settings();
            }
        });
    }

    fn draw_se_settings(&mut self, ui: &mut Ui) {
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("📢 効果音（SE）設定").strong().color(Color32::from_rgb(255, 180, 100)));
            ui.separator();

            ui.label(RichText::new("新規登録").small().color(COLOR_TIME));
            let right_col_w = (ui.available_width() - 80.0).max(100.0);
            Grid::new("se_registration_grid")
                .num_columns(2)
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    ui.label("単語:");
                    ui.add_sized([right_col_w, 20.0], TextEdit::singleline(&mut self.se_pattern_input));
                    ui.end_row();

                    ui.label("ファイル:");
                    ui.horizontal(|ui| {
                        ui.set_width(right_col_w);
                        let edit_w = right_col_w - 32.0; // ボタン+余白分
                        ui.add_sized([edit_w, 20.0], TextEdit::singleline(&mut self.se_file_input));
                        if ui.small_button("📁").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Audio files", &["mp3", "wav", "ogg", "flac"])
                                .pick_file() 
                            {
                                self.se_file_input = path.to_string_lossy().to_string();
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("音量:");
                    ui.add_sized([right_col_w, 20.0], Slider::new(&mut self.se_volume_input, 0.0..=2.0));
                    ui.end_row();
                });

            ui.add_space(4.0);
            if ui.button("SEを追加").clicked() && !self.se_pattern_input.is_empty() && !self.se_file_input.is_empty() {
                self.settings.se_entries.push(settings::SeEntry {
                    pattern: self.se_pattern_input.clone(),
                    file_path: self.se_file_input.clone(),
                    volume: self.se_volume_input,
                });
                self.se_pattern_input.clear();
                self.se_file_input.clear();
                self.save_settings();
            }

            ui.separator();
            ui.label(RichText::new("登録済みSE一覧").small().color(COLOR_TIME));
            
            let mut remove_se_idx = None;
            for (i, se) in self.settings.se_entries.iter().enumerate() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&se.pattern).strong());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("x").clicked() {
                                remove_se_idx = Some(i);
                            }
                            if ui.small_button("▶").clicked() {
                                self.play_se(&se.file_path, se.volume);
                            }
                        });
                    });
                    ui.label(RichText::new(&se.file_path).small().color(COLOR_TIME));
                });
            }
            if let Some(i) = remove_se_idx {
                self.settings.se_entries.remove(i);
                self.save_settings();
            }
        });
    }

    fn draw_other_settings(&mut self, ui: &mut Ui) {
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("⚙️ その他表示設定").strong().color(COLOR_TIME));
            ui.separator();
            ui.checkbox(&mut self.auto_scroll, "自動スクロール");
            ui.add_space(4.0);
            if ui.button("コメントをクリア").clicked() {
                self.messages.clear();
            }
            ui.add_space(4.0);
            if ui.button("📜 ログを読み込む").clicked() {
                self.load_log_file();
            }
            ui.add_space(8.0);
            
            ui.label(RichText::new("📂 ログ保存先フォルダ").strong());
            ui.horizontal(|ui| {
                let display_dir = if self.settings.log_dir.is_empty() {
                    "（デフォルト: logs/）".to_string()
                } else {
                    self.settings.log_dir.clone()
                };
                ui.label(&display_dir);
                if ui.button("変更").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.settings.log_dir = path.to_string_lossy().to_string();
                        self.save_settings();
                    }
                }
                if !self.settings.log_dir.is_empty() && ui.button("初期化").clicked() {
                    self.settings.log_dir.clear();
                    self.save_settings();
                }
            });
        });
    }



    fn param_slider_raw(ui: &mut Ui, val: &mut u32, min: u32, max: u32) -> bool {
        let mut changed = false;
        
        // スライダー (Gridの2列目)
        let mut f = *val as f32;
        if ui.add(Slider::new(&mut f, min as f32..=max as f32).show_value(false).smart_aim(false)).changed() {
            *val = f as u32;
            changed = true;
        }
        
        // 数値 (Gridの3列目)
        let mut n = *val as i32;
        if ui.add(DragValue::new(&mut n).range(min as i32..=max as i32).speed(1.0)).changed() {
            *val = n as u32;
            changed = true;
        }

        changed
    }

    fn draw_comment_log(&mut self, ui: &mut Ui) {
        let available = ui.available_rect_before_wrap();

        // 送信済みコメントの最終インデックスを特定
        let mut last_sent_idx = None;
        if let Some(ref last_id) = self.last_gemini_sent_comment_id {
            for (idx, msg) in self.messages.iter().enumerate() {
                if &msg.id == last_id {
                    last_sent_idx = Some(idx);
                    break;
                }
            }
        }

        // ヘッダー
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("💬 コメントログ")
                    .strong()
                    .color(Color32::from_rgb(100, 180, 255)),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let total = self.messages.len();
                let sent = match last_sent_idx {
                    Some(idx) => idx + 1,
                    None => 0,
                };
                let unsent = total - sent;
                
                ui.label(
                    RichText::new(format!("{} 件 (未送信: {}件)", total, unsent))
                        .small()
                        .color(COLOR_TIME),
                );
            });
        });
        ui.separator();

        let scroll = ScrollArea::vertical()
            .auto_shrink([false; 2])
            .stick_to_bottom(self.auto_scroll);

        scroll.show(ui, |ui| {
            ui.set_width((available.width() - 20.0).max(100.0));
            // インデックスで回すことで借用の問題を回避
            for i in 0..self.messages.len() {
                let msg = self.messages[i].clone();
                let is_sent = match last_sent_idx {
                    Some(last_idx) => i <= last_idx,
                    None => false,
                };
                self.draw_comment_item(ui, &msg, is_sent);

                // 送信済みと未送信の境界にインジケータを挟む
                if let Some(last_idx) = last_sent_idx {
                    if i == last_idx && i < self.messages.len() - 1 {
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            ui.label(RichText::new("─── 🤖 ここまでGemini送信済み ───")
                                .color(Color32::from_rgb(140, 100, 220))
                                .small());
                        });
                        ui.add_space(4.0);
                    }
                }
            }
        });
    }

    fn draw_gemini_log(&mut self, ui: &mut Ui) {
        let available = ui.available_rect_before_wrap();

        // ヘッダー
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("🤖 Gemini回答ログ")
                    .strong()
                    .color(Color32::from_rgb(180, 120, 255)),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("クリア").clicked() {
                    self.gemini_responses.clear();
                }
            });
        });
        ui.separator();

        let mut scroll = ScrollArea::vertical()
            .auto_shrink([false; 2]);

        // 新しい回答を受信したときに、自動で末尾にスクロールする
        if self.gemini_scroll_to_bottom {
            scroll = scroll.stick_to_bottom(true);
            self.gemini_scroll_to_bottom = false;
        } else {
            scroll = scroll.stick_to_bottom(self.auto_scroll);
        }

        scroll.show(ui, |ui| {
            ui.set_width((available.width() - 20.0).max(100.0));
            
            if self.gemini_responses.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("回答はまだありません").color(COLOR_TIME));
                });
                return;
            }

            for resp in &self.gemini_responses {
                ui.group(|ui| {
                    ui.set_width(ui.available_width());
                    
                    // タイムスタンプと要約元サマリー
                    ui.horizontal(|ui| {
                        let time_str = resp.timestamp.format("%H:%M:%S").to_string();
                        ui.label(RichText::new(time_str).color(COLOR_TIME).monospace().small());
                        if !resp.prompt_summary.is_empty() {
                            ui.label(RichText::new(&resp.prompt_summary).color(COLOR_TIME).small());
                        }
                    });
                    
                    ui.add_space(4.0);
                    
                    // 生成された回答
                    if resp.is_error {
                        ui.label(RichText::new(&resp.response).color(Color32::from_rgb(255, 100, 100)));
                    } else {
                        ui.label(RichText::new(&resp.response).color(Color32::from_rgb(240, 230, 255)));
                    }
                });
                ui.add_space(6.0);
            }
        });
    }

    fn draw_comment_item(&mut self, ui: &mut Ui, msg: &ChatMessage, is_sent: bool) {
        let author_color = if is_sent {
            Color32::from_rgb(110, 110, 120) // 送信済みの場合は一律暗いグレー
        } else if msg.is_owner {
            COLOR_OWNER
        } else if msg.is_moderator {
            COLOR_MODERATOR
        } else if msg.is_member {
            COLOR_MEMBER
        } else {
            COLOR_NORMAL
        };

        let time_color = if is_sent {
            Color32::from_rgb(80, 80, 90)
        } else {
            COLOR_TIME
        };

        let msg_color = if is_sent {
            Color32::from_rgb(110, 110, 120)
        } else {
            Color32::from_rgb(245, 245, 245)
        };

        let time_str = msg.timestamp.format("%H:%M:%S").to_string();

        ui.horizontal_wrapped(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new(&time_str).color(time_color).monospace());
            
            // ユーザー名にコンテキストメニュー（メモ編集）を追加
            let name_label = ui.label(RichText::new(&msg.author).color(author_color).strong());
            
            // メモがある場合は「📝」を表示
            if let Some(user_note) = self.settings.user_notes.get(&msg.author_id) {
                let note_color = if is_sent { Color32::from_rgb(80, 80, 90) } else { COLOR_TIME };
                ui.label(RichText::new("📝").small().color(note_color))
                    .on_hover_text(&user_note.note);
            }

            let name_resp = name_label.interact(Sense::click());
            if name_resp.clicked() || name_resp.secondary_clicked() {
                self.editing_user_note = Some((msg.author_id.clone(), msg.author.clone()));
                self.note_input = self.settings.user_notes.get(&msg.author_id).map(|un| un.note.clone()).unwrap_or_default();
            }

            ui.label(RichText::new(":").color(time_color));
            // メッセージ本文のサイズをBody(15.0)に
            ui.label(RichText::new(&msg.message).color(msg_color));
        });
        ui.add_space(3.0); // 行間を少し広げる
    }

    fn draw_dict_window(&mut self, ctx: &Context) {
        let mut open = self.show_dict_window;
        Window::new("📖 辞書登録")
            .open(&mut open)
            .resizable(true)
            .default_size([480.0, 400.0])
            .show(ctx, |ui| {
                // 追加フォーム
                ui.group(|ui| {
                    ui.label("新しく追加:");
                    ui.horizontal(|ui| {
                        ui.label("パターン:");
                        ui.text_edit_singleline(&mut self.dict_pattern_input);
                    });
                    ui.horizontal(|ui| {
                        ui.label("置換後:");
                        ui.text_edit_singleline(&mut self.dict_replacement_input);
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.dict_is_regex, "正規表現");
                        if ui.button("追加").clicked() && !self.dict_pattern_input.is_empty() {
                            self.dict_entries.push(DictEntry {
                                pattern: self.dict_pattern_input.clone(),
                                replacement: self.dict_replacement_input.clone(),
                                is_regex: self.dict_is_regex,
                            });
                            save_dictionary(&self.dict_entries);
                            self.dict_pattern_input.clear();
                            self.dict_replacement_input.clear();
                        }
                    });
                });

                ui.separator();

                // 一覧表示
                let mut delete_idx: Option<usize> = None;
                let mut move_up_idx: Option<usize> = None;
                let mut move_down_idx: Option<usize> = None;

                ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    let len = self.dict_entries.len();
                    for (i, entry) in self.dict_entries.iter().enumerate() {
                        ui.horizontal(|ui| {
                            // 並び替えボタン
                            ui.add_enabled_ui(i > 0, |ui| {
                                if ui.button("↑").on_hover_text("上に移動").clicked() {
                                    move_up_idx = Some(i);
                                }
                            });
                            ui.add_enabled_ui(i < len - 1, |ui| {
                                if ui.button("↓").on_hover_text("下に移動").clicked() {
                                    move_down_idx = Some(i);
                                }
                            });

                            ui.label(if entry.is_regex { 
                                RichText::new("[R]").small().color(Color32::from_rgb(200, 150, 100)) 
                            } else { 
                                RichText::new("[S]").small().color(COLOR_TIME) 
                            });

                            ui.label(RichText::new(&entry.pattern).small().strong());
                            ui.label(RichText::new("→").small().color(COLOR_TIME));
                            ui.label(RichText::new(&entry.replacement).small().color(Color32::from_rgb(100, 200, 100)));
                            
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button("❌").clicked() {
                                    delete_idx = Some(i);
                                }
                            });
                        });
                        ui.separator();
                    }
                });

                if let Some(i) = delete_idx {
                    self.dict_entries.remove(i);
                    save_dictionary(&self.dict_entries);
                }
                if let Some(i) = move_up_idx {
                    self.dict_entries.swap(i, i - 1);
                    save_dictionary(&self.dict_entries);
                }
                if let Some(i) = move_down_idx {
                    self.dict_entries.swap(i, i + 1);
                    save_dictionary(&self.dict_entries);
                }
            });
        self.show_dict_window = open;
    }

    fn draw_user_note_window(&mut self, ctx: &Context) {
        let mut open = self.editing_user_note.is_some();
        let mut author_name = String::new();
        let mut author_id = String::new();
        
        if let Some((id, name)) = &self.editing_user_note {
            author_id = id.clone();
            author_name = name.clone();
        }

        Window::new(format!("👤 ユーザーメモ: {}", author_name))
            .open(&mut open)
            .resizable(true)
            .default_size([300.0, 200.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(format!("ID: {}", author_id)).small().color(COLOR_TIME));
                ui.add_space(4.0);
                
                ui.add(TextEdit::multiline(&mut self.note_input)
                    .hint_text("メモを入力...")
                    .desired_width(f32::INFINITY)
                    .desired_rows(5));
                
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("保存").clicked() {
                        if self.note_input.trim().is_empty() {
                            self.settings.user_notes.remove(&author_id);
                        } else {
                            self.settings.user_notes.insert(
                                author_id.clone(), 
                                UserNote { name: author_name.clone(), note: self.note_input.clone() }
                            );
                        }
                        self.save_settings();
                        self.editing_user_note = None;
                    }
                    if ui.button("キャンセル").clicked() {
                        self.editing_user_note = None;
                    }
                });
            });

        if !open {
            self.editing_user_note = None;
        }
    }

    fn play_se(&self, path: &str, volume: f32) {
        if let Some((_, handle)) = &self.audio_output {
            let path_buf = std::path::PathBuf::from(path);
            if let Ok(file) = File::open(path_buf) {
                let reader = BufReader::new(file);
                if let Ok(source) = Decoder::new(reader) {
                    if let Ok(sink) = Sink::try_new(handle) {
                        sink.set_volume(volume);
                        sink.append(source);
                        sink.detach();
                    }
                }
            }
        }
    }

    fn draw_user_management_window(&mut self, ctx: &Context) {
        let mut open = self.show_user_management_window;
        Window::new("👤 ユーザー管理")
            .open(&mut open)
            .resizable(true)
            .default_size([500.0, 400.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("検索:");
                    ui.text_edit_singleline(&mut self.user_search_query);
                });
                ui.add_space(8.0);

                ScrollArea::vertical().show(ui, |ui| {
                    let mut notes_vec: Vec<_> = self.settings.user_notes.iter().collect();
                    // 名前でソート
                    notes_vec.sort_by(|a, b| a.1.name.cmp(&b.1.name));

                    Grid::new("user_management_grid")
                        .num_columns(3)
                        .spacing([12.0, 8.0])
                        .min_col_width(100.0)
                        .show(ui, |ui| {
                            for (id, user_note) in notes_vec {
                                if !self.user_search_query.is_empty() && 
                                   !user_note.name.to_lowercase().contains(&self.user_search_query.to_lowercase()) &&
                                   !user_note.note.to_lowercase().contains(&self.user_search_query.to_lowercase()) {
                                    continue;
                                }

                                ui.label(RichText::new(&user_note.name).strong());
                                
                                let note_preview = if user_note.note.len() > 30 {
                                    format!("{}...", &user_note.note.chars().take(30).collect::<String>())
                                } else {
                                    user_note.note.clone()
                                };
                                ui.label(RichText::new(note_preview).small().color(COLOR_TIME));

                                if ui.button("編集").clicked() {
                                    self.editing_user_note = Some((id.clone(), user_note.name.clone()));
                                    self.note_input = user_note.note.clone();
                                }
                                ui.end_row();
                            }
                        });
                });
            });
        self.show_user_management_window = open;
    }

    fn draw_status_bar(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let (dot_color, dot) = if self.is_connected {
                (Color32::from_rgb(50, 200, 80), "●")
            } else {
                (Color32::from_rgb(140, 140, 140), "○")
            };
            ui.label(RichText::new(dot).color(dot_color));
            ui.label(RichText::new(&self.status_text).small().color(COLOR_TIME));
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // WebServerのライフサイクル監視
        self.handle_web_server_lifecycle();

        // チャットイベントのポーリング
        self.poll_chat_events();

        // Geminiの回答受信ポーリング
        self.poll_gemini_responses();

        // Gemini送信のタイマー制御
        if self.is_connected && self.settings.gemini_enabled && !self.is_waiting_gemini {
            let now = std::time::Instant::now();
            let should_send = match self.last_gemini_sent_time {
                None => true,
                Some(last_time) => now.duration_since(last_time).as_secs() >= self.settings.gemini_interval_secs as u64,
            };

            if should_send {
                let new_comments = self.get_new_comments_for_gemini();
                if !new_comments.is_empty() {
                    self.send_comments_to_gemini(new_comments);
                }
            }
        }

        // 常に一定間隔で再描画を要求（バックグラウンドでの受信・読み上げを継続するため）
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        // 辞書ウィンドウ
        if self.show_dict_window {
            self.draw_dict_window(ctx);
        }

        // ユーザーメモウィンドウ
        if self.editing_user_note.is_some() {
            self.draw_user_note_window(ctx);
        }

        // ユーザー管理ウィンドウ
        if self.show_user_management_window {
            self.draw_user_management_window(ctx);
        }

        // トップバー
        TopBottomPanel::top("top_bar")
            .frame(Frame::default().fill(Color32::from_rgb(25, 25, 32)).inner_margin(8.0))
            .show(ctx, |ui| {
                self.draw_top_bar(ui);
            });

        // ステータスバー
        TopBottomPanel::bottom("status_bar")
            .frame(Frame::default().fill(Color32::from_rgb(18, 18, 22)).inner_margin(4.0))
            .show(ctx, |ui| {
                self.draw_status_bar(ui);
            });

        // 左パネル（設定）
        SidePanel::left("left_panel")
            .resizable(true)
            .default_width(self.left_panel_width)
            .width_range(220.0..=400.0)
            .frame(Frame::default().fill(Color32::from_rgb(22, 22, 28)).inner_margin(10.0))
            .show(ctx, |ui| {
                self.draw_left_panel(ui);
            });

        // 右パネル（Geminiの回答）
        if self.settings.gemini_enabled {
            SidePanel::right("gemini_panel")
                .resizable(true)
                .default_width(320.0)
                .width_range(200.0..=500.0)
                .frame(Frame::default().fill(Color32::from_rgb(22, 22, 28)).inner_margin(10.0))
                .show(ctx, |ui| {
                    self.draw_gemini_log(ui);
                });
        }

        // 中央パネル（コメントログ）
        CentralPanel::default()
            .frame(Frame::default().fill(Color32::from_rgb(20, 20, 26)).inner_margin(10.0))
            .show(ctx, |ui| {
                self.draw_comment_log(ui);
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_settings();
        if let Some(tx) = self.cevio_tx.take() {
            let _ = tx.try_send(CevioCommand::Stop);
        }
        if let Some(shutdown_tx) = self.web_server_shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
    }
}
