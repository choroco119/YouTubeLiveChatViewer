use std::sync::Arc;
use std::sync::Mutex;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;
use serde_json::json;
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct ObsComment {
    pub id: String,
    pub author: String,
    pub message: String,
    pub timestamp: String,
    pub is_owner: bool,
    pub is_moderator: bool,
    pub is_member: bool,
}

pub struct WebServerState {
    pub latest_response: String,
    pub latest_timestamp: String,
    pub gemini_name: String,
    pub comments: Vec<ObsComment>,
    pub gemini_enabled: bool,
}

pub async fn start_obs_web_server(
    port: u16,
    state: Arc<Mutex<WebServerState>>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let addr = format!("127.0.0.1:{}", port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("OBS Web Server bind error on {}: {}", addr, e);
            return;
        }
    };

    loop {
        tokio::select! {
            accept_res = listener.accept() => {
                let (mut socket, _) = match accept_res {
                    Ok(res) => res,
                    Err(_) => continue,
                };
                let state_clone = state.clone();
                tokio::spawn(async move {
                    let mut buf = [0; 2048];
                    let mut request = String::new();
                    if let Ok(n) = socket.read(&mut buf).await {
                        if n > 0 {
                            request = String::from_utf8_lossy(&buf[..n]).to_string();
                        }
                    }

                    if request.is_empty() {
                        return;
                    }

                    let first_line = request.lines().next().unwrap_or("");
                    let parts: Vec<&str> = first_line.split_whitespace().collect();
                    if parts.len() < 2 {
                        return;
                    }
                    let path = parts[1];

                    if path == "/api/response" {
                        let response_body = {
                            let current_state = state_clone.lock().unwrap();
                            let response_data = json!({
                                "response": current_state.latest_response,
                                "timestamp": current_state.latest_timestamp,
                                "name": current_state.gemini_name,
                                "comments": current_state.comments,
                                "gemini_enabled": current_state.gemini_enabled,
                            });
                            serde_json::to_string(&response_data).unwrap()
                        };
                        let http_response = format!(
                            "HTTP/1.1 200 OK\r\n\
                             Content-Type: application/json\r\n\
                             Access-Control-Allow-Origin: *\r\n\
                             Content-Length: {}\r\n\
                             Connection: close\r\n\r\n\
                             {}",
                            response_body.len(),
                            response_body
                        );
                        let _ = socket.write_all(http_response.as_bytes()).await;
                    } else if path == "/" || path == "/index.html" {
                        let html = get_overlay_html();
                        let http_response = format!(
                            "HTTP/1.1 200 OK\r\n\
                             Content-Type: text/html; charset=utf-8\r\n\
                             Access-Control-Allow-Origin: *\r\n\
                             Content-Length: {}\r\n\
                             Connection: close\r\n\r\n\
                             {}",
                            html.len(),
                            html
                        );
                        let _ = socket.write_all(http_response.as_bytes()).await;
                    } else {
                        let http_response = "HTTP/1.1 404 NOT FOUND\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = socket.write_all(http_response.as_bytes()).await;
                    }
                });
            }
            _ = &mut shutdown_rx => {
                break;
            }
        }
    }
}

fn get_overlay_html() -> String {
    r#"<!DOCTYPE html>
<html lang="ja">
<head>
    <meta charset="UTF-8">
    <title>Gemini OBS Overlay</title>
    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
    <link href="https://fonts.googleapis.com/css2?family=M+PLUS+Rounded+1c:wght@500;700&display=swap" rel="stylesheet">
    <style>
        body {
            margin: 0;
            padding: 20px;
            background-color: transparent;
            color: #ffffff;
            font-family: 'M PLUS Rounded 1c', sans-serif;
            overflow: hidden;
            height: 100vh;
            box-sizing: border-box;
        }

        /* 上下レイアウト用 */
        #wrapper {
            display: flex;
            flex-direction: column;
            gap: 20px;
            width: 100%;
            height: 100%;
        }

        /* Geminiエリア（上側） */
        #gemini-column {
            flex-shrink: 0;
            display: flex;
            justify-content: flex-start;
            align-items: flex-start;
            width: 100%;
        }

        /* コンテナ */
        #container {
            width: 100%;
            max-width: 800px; /* 横長画面に対応できるよう最大幅を広げる */
            opacity: 0;
            transform: translateY(20px) scale(0.95);
            transition: opacity 0.5s cubic-bezier(0.16, 1, 0.3, 1), 
                        transform 0.5s cubic-bezier(0.16, 1, 0.3, 1);
        }

        #container.visible {
            opacity: 1;
            transform: translateY(0) scale(1);
        }

        /* メインのメッセージカード */
        .card {
            background: rgba(20, 20, 25, 0.85);
            border: 2px solid rgba(168, 85, 247, 0.5);
            border-radius: 18px;
            padding: 16px 20px;
            box-shadow: 0 10px 25px rgba(0, 0, 0, 0.5), 
                        inset 0 0 10px rgba(168, 85, 247, 0.1);
            backdrop-filter: blur(8px);
        }

        /* ラベル（AI Assistant） */
        .label {
            font-size: 12px;
            text-transform: uppercase;
            letter-spacing: 1.5px;
            color: #a855f7;
            font-weight: 700;
            margin-bottom: 6px;
            display: flex;
            align-items: center;
            gap: 5px;
        }

        .label-dot {
            width: 6px;
            height: 6px;
            background-color: #a855f7;
            border-radius: 50%;
            animation: pulse 1.5s infinite;
        }

        @keyframes pulse {
            0%, 100% { opacity: 0.3; }
            50% { opacity: 1; }
        }

        /* 回答テキスト */
        .text {
            font-size: 16px;
            line-height: 1.6;
            font-weight: 500;
            text-shadow: 0 2px 4px rgba(0, 0, 0, 0.5);
            word-wrap: break-word;
        }

        /* チャットカラム（下側） */
        #chat-column {
            flex: 1;
            width: 100%;
            max-width: 800px;
            display: flex;
            flex-direction: column;
            justify-content: flex-end; /* コメントを下寄せにして上へ押し出す */
            overflow: hidden;
        }

        #chat-container {
            display: flex;
            flex-direction: column;
            gap: 12px;
            overflow-y: auto;
            max-height: 100%;
            scrollbar-width: none; /* Firefox */
        }

        #chat-container::-webkit-scrollbar {
            display: none; /* Chrome, Safari */
        }

        /* コメントカード */
        .comment-card {
            background: rgba(20, 20, 25, 0.75);
            border-left: 3px solid rgba(255, 255, 255, 0.3);
            border-radius: 0 10px 10px 0;
            padding: 10px 14px;
            box-shadow: 0 4px 15px rgba(0, 0, 0, 0.3);
            backdrop-filter: blur(4px);
            animation: slideInComment 0.25s cubic-bezier(0.16, 1, 0.3, 1) forwards;
            word-wrap: break-word;
        }

        @keyframes slideInComment {
            from {
                opacity: 0;
                transform: translateX(-20px);
            }
            to {
                opacity: 1;
                transform: translateX(0);
            }
        }

        .comment-header {
            display: flex;
            align-items: center;
            gap: 6px;
            font-size: 11px;
            margin-bottom: 4px;
        }

        .comment-badge {
            font-size: 12px;
        }

        .comment-author {
            font-weight: 700;
            text-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);
        }

        .comment-time {
            color: rgba(255, 255, 255, 0.4);
            font-size: 10px;
            margin-left: auto;
        }

        .comment-message {
            font-size: 14px;
            line-height: 1.5;
            font-weight: 500;
            text-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);
        }
    </style>
</head>
<body>
    <div id="wrapper">
        <!-- Gemini回答エリア（上側） -->
        <div id="gemini-column">
            <div id="container">
                <div class="card">
                    <div class="label">
                        <span class="label-dot"></span><span id="label-text">Gemini Assistant</span>
                    </div>
                    <div class="text" id="response-text">...</div>
                </div>
            </div>
        </div>
        <!-- 通常コメントエリア（下側） -->
        <div id="chat-column">
            <div id="chat-container"></div>
        </div>
    </div>

    <script>
        let lastTimestamp = "";
        let lastResponse = "";
        const container = document.getElementById('container');
        const textElem = document.getElementById('response-text');
        const chatContainer = document.getElementById('chat-container');
        const displayedCommentIds = new Set();

        function escapeHtml(str) {
            return str
                .replace(/&/g, "&amp;")
                .replace(/</g, "&lt;")
                .replace(/>/g, "&gt;")
                .replace(/"/g, "&quot;")
                .replace(/'/g, "&#039;");
        }

        async function checkUpdate() {
            try {
                const res = await fetch('/api/response');
                if (!res.ok) return;
                const data = await res.json();
                
                // 表示名更新
                if (data.name) {
                    document.getElementById('label-text').innerText = data.name;
                }

                // Geminiの有効無効による表示切り替え
                const geminiCol = document.getElementById('gemini-column');
                if (geminiCol) {
                    if (data.gemini_enabled === false) {
                        geminiCol.style.display = 'none';
                    } else {
                        geminiCol.style.display = 'flex';
                    }
                }

                // Gemini回答更新
                if (data.response && (data.timestamp !== lastTimestamp || data.response !== lastResponse)) {
                    lastTimestamp = data.timestamp;
                    lastResponse = data.response;
                    
                    // フェードアウト
                    container.classList.remove('visible');
                    
                    setTimeout(() => {
                        textElem.innerText = data.response;
                        // フェードイン
                        container.classList.add('visible');
                    }, 500);
                }

                // 通常コメント更新
                if (data.comments && Array.isArray(data.comments)) {
                    let added = false;
                    data.comments.forEach(comment => {
                        if (!displayedCommentIds.has(comment.id)) {
                            displayedCommentIds.add(comment.id);
                            
                            const card = document.createElement('div');
                            card.className = 'comment-card';
                            card.setAttribute('data-id', comment.id);
                            
                            let badge = '';
                            let authorColor = '#e0e0e0';
                            let borderLeftColor = 'rgba(255, 255, 255, 0.3)';
                            
                            if (comment.is_owner) {
                                badge = '<span class="comment-badge">👑</span>';
                                authorColor = '#ffe664';
                                borderLeftColor = '#ffe664';
                            } else if (comment.is_moderator) {
                                badge = '<span class="comment-badge">🔧</span>';
                                authorColor = '#82dcff';
                                borderLeftColor = '#82dcff';
                            } else if (comment.is_member) {
                                badge = '<span class="comment-badge">🟢</span>';
                                authorColor = '#64ffb4';
                                borderLeftColor = '#64ffb4';
                            }
                            
                            card.style.borderLeft = `3px solid ${borderLeftColor}`;
                            card.innerHTML = `
                                <div class="comment-header">
                                    ${badge}
                                    <span class="comment-author" style="color: ${authorColor}">${escapeHtml(comment.author)}</span>
                                    <span class="comment-time">${comment.timestamp}</span>
                                </div>
                                <div class="comment-message">${escapeHtml(comment.message)}</div>
                            `;
                            
                            chatContainer.appendChild(card);
                            added = true;
                        }
                    });
                    
                    if (added) {
                        // 古いコメントを削除（最大30件）
                        while (chatContainer.children.length > 30) {
                            const firstChild = chatContainer.firstChild;
                            if (firstChild) {
                                const oldId = firstChild.getAttribute('data-id');
                                if (oldId) {
                                    displayedCommentIds.delete(oldId);
                                }
                                chatContainer.removeChild(firstChild);
                            }
                        }
                        
                        // 自動スクロール
                        chatContainer.scrollTop = chatContainer.scrollHeight;
                    }
                }
            } catch (e) {
                console.error("Failed to fetch update:", e);
            }
        }

        setInterval(checkUpdate, 1000);
        checkUpdate();
    </script>
</body>
</html>"#.to_string()
}
