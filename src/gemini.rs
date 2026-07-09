use reqwest::Client;
use serde_json::json;
use std::time::Duration;

/// Gemini API を呼び出して回答を取得する非同期関数
pub async fn query_gemini(
    api_key: &str,
    model: &str,
    system_prompt: &str,
    prompt: &str,
) -> Result<String, String> {
    if api_key.trim().is_empty() {
        return Err("APIキーが設定されていません。左パネルの設定から登録してください。".to_string());
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("HTTPクライアント作成失敗: {}", e))?;

    // models/ プレフィックスの補正
    let model_name = if model.starts_with("models/") {
        model.to_string()
    } else {
        format!("models/{}", model)
    };

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/{}:generateContent?key={}",
        model_name,
        api_key
    );

    let body = json!({
        "contents": [
            {
                "parts": [
                    {
                        "text": prompt
                    }
                ]
            }
        ],
        "systemInstruction": {
            "parts": [
                {
                    "text": system_prompt
                }
            ]
        }
    });

    let mut retries = 0;
    let max_retries = 3;
    let mut delay = Duration::from_secs(3);

    loop {
        let resp = match client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                if retries < max_retries {
                    retries += 1;
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                    continue;
                }
                return Err(format!("APIリクエストエラー: {}", e));
            }
        };

        let status = resp.status();

        if status.is_success() {
            let json: serde_json::Value = match resp.json().await {
                Ok(j) => j,
                Err(e) => return Err(format!("JSONパースエラー: {}", e)),
            };

            if let Some(text) = json
                .pointer("/candidates/0/content/parts/0/text")
                .and_then(|v| v.as_str())
            {
                return Ok(text.trim().to_string());
            } else {
                if let Some(msg) = json.pointer("/error/message").and_then(|v| v.as_str()) {
                    return Err(format!("Gemini APIエラー: {}", msg));
                } else {
                    return Err("APIレスポンスの解析に失敗しました。".to_string());
                }
            }
        } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            if retries < max_retries {
                retries += 1;
                tokio::time::sleep(delay).await;
                delay *= 2;
                continue;
            }
        }

        let error_body = resp.text().await.unwrap_or_default();
        return Err(format!("APIエラー (ステータス {}): {}", status, error_body));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_empty_api_key() {
        let res = query_gemini("", "gemini-2.5-flash", "You are a helpful assistant.", "Hello").await;
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err(),
            "APIキーが設定されていません。左パネルの設定から登録してください。"
        );
    }
}
