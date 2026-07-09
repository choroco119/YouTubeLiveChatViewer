use reqwest::Client;
use serde_json::Value;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub id: String,
    pub author: String,
    pub author_id: String,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Local>,
    pub is_owner: bool,
    pub is_moderator: bool,
    pub is_member: bool,
}

pub enum ChatEvent {
    Message(ChatMessage),
    Title(String),
    Error(String),
    Ended,
}

fn extract_video_id(input: &str) -> String {
    let input = input.trim();
    // 短縮URL youtu.be/VIDEO_ID
    if let Some(pos) = input.find("youtu.be/") {
        return input[pos + 9..].split(['?', '&', '/']).next().unwrap_or("").to_string();
    }
    // 標準URL v=VIDEO_ID
    if let Some(pos) = input.find("v=") {
        return input[pos + 2..].split('&').next().unwrap_or("").to_string();
    }
    // 埋め込み/ショート/ライブ形式 /embed/ /shorts/ /live/
    for marker in &["/embed/", "/shorts/", "/live/"] {
        if let Some(pos) = input.find(marker) {
            return input[pos + marker.len()..].split(['?', '&', '/']).next().unwrap_or("").to_string();
        }
    }
    // IDのみ、または不明な形式はそのまま返す
    input.to_string()
}

/// ytInitialDataからlive chat continuationトークンを取得
fn parse_continuation_from_html(html: &str) -> Option<String> {
    // 抽出マーカーの候補（正規表現的な柔軟な検索）
    let mut json_str = "";
    
    if let Some(pos) = html.find("ytInitialData") {
        let rest = &html[pos..];
        if let Some(eq_pos) = rest.find('=') {
            let after_eq = &rest[eq_pos + 1..].trim_start();
            // JSONの開始位置を探す
            if after_eq.starts_with('{') {
                // 次のスクリプト終了タグを探す
                if let Some(end_pos) = after_eq.find(";</script>")
                    .or_else(|| after_eq.find("</script>"))
                    .or_else(|| after_eq.find("};"))
                {
                    json_str = &after_eq[..end_pos].trim();
                    if json_str.ends_with(';') {
                        json_str = &json_str[..json_str.len()-1];
                    }
                }
            }
        }
    }

    if json_str.is_empty() { return None; }
    let data: Value = serde_json::from_str(json_str).ok()?;

    // 1. 既知のパスを試行（ブラウザ調査結果を含む）
    let cont_paths = [
        "/contents/liveChatRenderer/continuations/0/invalidationContinuationData/continuation",
        "/contents/liveChatRenderer/continuations/0/timedContinuationData/continuation",
        "/contents/liveChatRenderer/continuations/0/reloadContinuationData/continuation",
        // ヘッダー内のサブメニューに隠れている場合がある
        "/contents/liveChatRenderer/header/liveChatHeaderRenderer/viewSelector/sortFilterSubMenuRenderer/subMenuItems/0/continuation/reloadContinuationData/continuation",
        "/contents/liveChatRenderer/header/liveChatHeaderRenderer/viewSelector/sortFilterSubMenuRenderer/subMenuItems/1/continuation/reloadContinuationData/continuation",
    ];
    for path in cont_paths {
        if let Some(c) = data.pointer(path).and_then(|v| v.as_str()) {
            return Some(c.to_string());
        }
    }

    // 2. フォールバック: JSON全体を再帰的に走査して "continuation" キーを探す
    fn find_continuation_deep(v: &Value) -> Option<String> {
        if let Some(obj) = v.as_object() {
            if let Some(c) = obj.get("continuation").and_then(|v| v.as_str()) {
                if c.len() > 20 { return Some(c.to_string()); }
            }
            // reloadContinuationData などの下にある場合
            for (k, val) in obj {
                if k.contains("ContinuationData") {
                    if let Some(c) = val.get("continuation").and_then(|v| v.as_str()) {
                        return Some(c.to_string());
                    }
                }
                if let Some(res) = find_continuation_deep(val) { return Some(res); }
            }
        } else if let Some(arr) = v.as_array() {
            for value in arr {
                if let Some(res) = find_continuation_deep(value) { return Some(res); }
            }
        }
        None
    }

    find_continuation_deep(&data)
}

/// ページからAPIキーを取得
fn parse_api_key_from_html(html: &str) -> Option<String> {
    let marker = "\"INNERTUBE_API_KEY\":\"";
    let start = html.find(marker)? + marker.len();
    let rest = &html[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// ページからvisitorDataを取得
fn parse_visitor_data_from_html(html: &str) -> Option<String> {
    let marker = "\"visitorData\":\"";
    let start = html.find(marker)? + marker.len();
    let rest = &html[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// ページから動画タイトルを取得
fn parse_title_from_html(html: &str) -> Option<String> {
    // 1. og:title
    if let Some(pos) = html.find("property=\"og:title\" content=\"") {
        let start = pos + "property=\"og:title\" content=\"".len();
        if let Some(end) = html[start..].find('"') {
            return Some(html[start..start+end].to_string());
        }
    }
    // 2. <title>
    if let Some(pos) = html.find("<title>") {
        let start = pos + 7;
        if let Some(end) = html[start..].find(" - YouTube") {
            return Some(html[start..start+end].to_string());
        }
    }
    None
}

/// チャットレスポンスからメッセージとnext continuationを解析
fn parse_chat_response(data: &Value) -> (Vec<ChatMessage>, Option<String>) {
    let mut messages = Vec::new();

    let chat_cont = data.pointer("/continuationContents/liveChatContinuation");
    if chat_cont.is_none() {
        return (messages, None);
    }
    let chat_cont = chat_cont.unwrap();

    // continuationの取得
    let next_cont = chat_cont
        .pointer("/continuations/0/invalidationContinuationData/continuation")
        .or_else(|| chat_cont.pointer("/continuations/0/timedContinuationData/continuation"))
        .or_else(|| chat_cont.pointer("/continuations/0/reloadContinuationData/continuation"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // actionsからメッセージ取得
    if let Some(actions) = chat_cont.get("actions").and_then(|v| v.as_array()) {
        for action in actions {
            // 通常のメッセージまたはスパチャ
            if let Some(item) = action.pointer("/addChatItemAction/item") {
                let renderer = item.get("liveChatTextMessageRenderer")
                    .or_else(|| item.get("liveChatPaidMessageRenderer"));
                
                if let Some(renderer) = renderer {
                    let id = renderer.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let author = renderer.pointer("/authorName/simpleText").and_then(|v| v.as_str()).unwrap_or("名無し").to_string();
                    let author_id = renderer.get("authorExternalChannelId").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    
                    // メッセージテキスト取得
                    let mut message = String::new();
                    if let Some(runs) = renderer.pointer("/message/runs").and_then(|v| v.as_array()) {
                        for run in runs {
                            if let Some(text) = run.get("text").and_then(|v| v.as_str()) {
                                message.push_str(text);
                            }
                        }
                    }

                    if !message.is_empty() {
                        let mut is_owner = false;
                        let mut is_moderator = false;
                        let mut is_member = false;

                        if let Some(badges) = renderer.pointer("/authorBadges").and_then(|v| v.as_array()) {
                            for badge in badges {
                                if let Some(icon_type) = badge.pointer("/liveChatAuthorBadgeRenderer/icon/iconType").and_then(|v| v.as_str()) {
                                    match icon_type {
                                        "OWNER" => is_owner = true,
                                        "MODERATOR" => is_moderator = true,
                                        "MEMBER" => is_member = true,
                                        _ => {}
                                    }
                                }
                            }
                        }

                        messages.push(ChatMessage {
                            id, author, author_id, message,
                            timestamp: chrono::Local::now(),
                            is_owner, is_moderator, is_member,
                        });
                    }
                }
            }
        }
    }

    (messages, next_cont)
}

pub async fn start_chat_monitor(
    video_id_raw: String,
    tx: mpsc::Sender<ChatEvent>,
) {
    let video_id = extract_video_id(&video_id_raw);

    let client = Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36")
        .timeout(Duration::from_secs(15))
        .build()
        .expect("HTTPクライアントの作成に失敗");

    let mut html = String::new();
    let mut found = false;

    // 1. タイトルの取得 (watchページを優先的に取得)
    let watch_url = format!("https://www.youtube.com/watch?v={}", video_id);
    if let Ok(resp) = client.get(&watch_url).header("Accept-Language", "ja,en;q=0.9").send().await {
        if let Ok(t) = resp.text().await {
            if let Some(title) = parse_title_from_html(&t) {
                let _ = tx.send(ChatEvent::Title(title)).await;
            }
            // もしwatchページにAPIキーが含まれていれば、htmlとして流用する
            if t.contains("INNERTUBE_API_KEY") {
                html = t;
                found = true;
            }
        }
    }

    // 2. ウォッチページでAPIキーが取れなかった場合、またはチャット用データを補完するために
    // live_chat?is_popout=1&v=... などを試す
    if !found {
        let urls = vec![
            format!("https://www.youtube.com/live_chat?is_popout=1&v={}", video_id),
            format!("https://www.youtube.com/live_chat?v={}", video_id),
        ];

        for url in urls {
            if let Ok(resp) = client.get(&url).header("Accept-Language", "ja,en;q=0.9").send().await {
                if let Ok(t) = resp.text().await {
                    if t.contains("INNERTUBE_API_KEY") {
                        html = t;
                        found = true;
                        break;
                    }
                }
            }
        }
    }

    if !found {
        let _ = tx.send(ChatEvent::Error("YouTubeページの取得に失敗しました。".to_string())).await;
        return;
    }

    let Some(mut continuation) = parse_continuation_from_html(&html) else {
        let _ = tx
            .send(ChatEvent::Error(
                "ライブチャットトークンが見つかりません。配信中であるか確認してください。".to_string(),
            ))
            .await;
        return;
    };

    let api_key = parse_api_key_from_html(&html)
        .unwrap_or_else(|| "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8".to_string());
    let visitor_data = parse_visitor_data_from_html(&html).unwrap_or_default();

    // 2. ポーリングループ
    let mut seen_ids = std::collections::HashSet::new();
    let mut is_first = true;

    loop {
        let api_url = format!(
            "https://www.youtube.com/youtubei/v1/live_chat/get_live_chat?key={}",
            api_key
        );

        let body = serde_json::json!({
            "context": {
                "client": {
                    "clientName": "WEB",
                    "clientVersion": "2.20240501.01.00",
                    "visitorData": visitor_data,
                    "hl": "ja",
                    "gl": "JP",
                    "timeZone": "Asia/Tokyo",
                }
            },
            "continuation": continuation
        });

        let resp = match client
            .post(&api_url)
            .header("Content-Type", "application/json")
            .header("X-YouTube-Client-Name", "1")
            .header("X-YouTube-Client-Version", "2.20240501.01.00")
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let _ = tx
                    .send(ChatEvent::Error(format!("APIエラー: {}", e)))
                    .await;
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        let json: Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                let _ = tx
                    .send(ChatEvent::Error(format!("JSONパースエラー: {}", e)))
                    .await;
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        let (msgs, next_cont) = parse_chat_response(&json);

        for msg in msgs {
            if !seen_ids.contains(&msg.id) {
                seen_ids.insert(msg.id.clone());
                if !is_first {
                    if tx.send(ChatEvent::Message(msg)).await.is_err() {
                        return; // 受信側がドロップしたら終了
                    }
                }
            }
        }

        is_first = false;

        match next_cont {
            Some(c) => continuation = c,
            None => {
                let _ = tx.send(ChatEvent::Ended).await;
                return;
            }
        }

        tokio::time::sleep(Duration::from_millis(2000)).await;
    }
}
