use std::env;
use reqwest::Client;
use serde_json::json;
use color_eyre::Result;
use tokio::sync::mpsc::UnboundedSender;
use futures_util::StreamExt;

#[derive(Debug, Clone)]
pub struct Usage {
    pub prompt_tokens: i32,
    pub response_tokens: i32,
    pub total_tokens: i32,
}

pub enum AiUpdate {
    Finished,
    Error(String),
    Content(String),
    Usage(Usage),
}

pub async fn stream_response(input: String, tx: UnboundedSender<AiUpdate>) {
    if let Ok(key) = env::var("GEMINI_API_KEY") {
        if let Err(e) = stream_gemini(&key, &input, tx.clone()).await {
             let _ = tx.send(AiUpdate::Error(format!("Error: {}", e)));
        }
    } else {
        // Fallback/Mock
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let _ = tx.send(AiUpdate::Content("(Mock AI): ".to_string()));
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let _ = tx.send(AiUpdate::Content(format!("I received: '{}'.\n", input)));
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let _ = tx.send(AiUpdate::Content("Set GEMINI_API_KEY for real responses.".to_string()));
        let _ = tx.send(AiUpdate::Usage(Usage { prompt_tokens: 10, response_tokens: 20, total_tokens: 30 }));
    }
    let _ = tx.send(AiUpdate::Finished);
}

async fn stream_gemini(api_key: &str, prompt: &str, tx: UnboundedSender<AiUpdate>) -> Result<()> {
    let client = Client::new();
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/gemini-3-flash-preview:streamGenerateContent?key={}&alt=sse", api_key);
    
    let body = json!({
        "contents": [{
            "parts": [{
                "text": prompt
            }]
        }]
    });

    let resp = client.post(url)
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_else(|_| "Could not read error body".to_string());
        return Err(color_eyre::eyre::eyre!("API Error {}: {}", status, text));
    }

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    // specific logging
    use std::io::Write;
    let mut debug_log = std::fs::OpenOptions::new().create(true).append(true).open("debug.log").ok();

    while let Some(item) = stream.next().await {
        let chunk = item?;
        let text = String::from_utf8_lossy(&chunk);
        
        if let Some(log) = &mut debug_log {
            writeln!(log, "Chunk: {:?}", text).ok();
        }

        buffer.push_str(&text);

        while let Some(pos) = buffer.find('\n') {
            let mut line = buffer[..pos].to_string();
            // Advance buffer past the \n
            buffer = buffer[pos + 1..].to_string();

            // Trim trailing \r if present (for \r\n support)
            if line.ends_with('\r') {
                line.pop();
            }

            if line.starts_with("data: ") {
                let json_str = &line[6..];
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                    // Extract Content
                    if let Some(candidates) = json.get("candidates") {
                        if let Some(first) = candidates.get(0) {
                            if let Some(content) = first.get("content") {
                                if let Some(parts) = content.get("parts") {
                                    if let Some(first_part) = parts.get(0) {
                                         if let Some(text_chunk) = first_part.get("text").and_then(|t| t.as_str()) {
                                             let _ = tx.send(AiUpdate::Content(text_chunk.to_string()));
                                         }
                                    }
                                }
                            }
                        }
                    }
                    
                    // Extract Usage Metadata
                    if let Some(usage) = json.get("usageMetadata") {
                        let prompt_tokens = usage["promptTokenCount"].as_i64().unwrap_or(0) as i32;
                        let response_tokens = usage["candidatesTokenCount"].as_i64().unwrap_or(0) as i32;
                        let total_tokens = usage["totalTokenCount"].as_i64().unwrap_or(0) as i32;
                        
                        let _ = tx.send(AiUpdate::Usage(Usage {
                            prompt_tokens,
                            response_tokens,
                            total_tokens,
                        }));
                    }
                }
            }
        }
    }

    Ok(())
}