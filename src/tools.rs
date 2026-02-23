use std::process::Stdio;
use tokio::fs;
use tokio::process::Command;

/// Main entry point for tool execution
pub async fn execute_tool(name: &str, args: &str) -> String {
    match name {
        "run_command" => run_command(args).await,
        "create_file" => create_file(args).await,
        "update_file" => update_file(args).await,
        "delete_file" => delete_file(args).await,
        "search_google" => search_google(args).await,
        _ => format!("Error: Unknown tool '{}'", name),
    }
}

/// Executes a terminal command via `sh -c`
async fn run_command(args: &str) -> String {
    // Assuming the AI passes the raw command string, or parse JSON if formatted as {"command": "..."}
    let command_str = extract_json_field(args, "command").unwrap_or_else(|| args.to_string());

    match Command::new("sh")
        .arg("-c")
        .arg(&command_str)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
    {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            format!("STDOUT:\n{}\nSTDERR:\n{}", stdout, stderr)
        }
        Err(e) => format!("Failed to execute command: {}", e),
    }
}

/// Creates a new file
async fn create_file(args: &str) -> String {
    let path = extract_json_field(args, "path").unwrap_or_default();
    let content = extract_json_field(args, "content").unwrap_or_default();

    if path.is_empty() {
        return "Error: 'path' is required".into();
    }

    match fs::write(&path, content).await {
        Ok(_) => format!("Successfully created/written to {}", path),
        Err(e) => format!("Error writing file: {}", e),
    }
}

/// Updates an existing file (appends content)
async fn update_file(args: &str) -> String {
    let path = extract_json_field(args, "path").unwrap_or_default();
    let content = extract_json_field(args, "content").unwrap_or_default();

    if path.is_empty() {
        return "Error: 'path' is required".into();
    }

    use tokio::io::AsyncWriteExt;
    match fs::OpenOptions::new().append(true).open(&path).await {
        Ok(mut file) => {
            if let Err(e) = file.write_all(content.as_bytes()).await {
                return format!("Error writing to file: {}", e);
            }
            format!("Successfully updated {}", path)
        }
        Err(e) => format!("Error opening file: {}", e),
    }
}

/// Deletes a file
async fn delete_file(args: &str) -> String {
    let path = extract_json_field(args, "path").unwrap_or_else(|| args.to_string());

    match fs::remove_file(&path).await {
        Ok(_) => format!("Successfully deleted {}", path),
        Err(e) => format!("Error deleting file: {}", e),
    }
}

/// Performs a simple google search
async fn search_google(args: &str) -> String {
    let query = extract_json_field(args, "query").unwrap_or_else(|| args.to_string());

    let url = match reqwest::Url::parse_with_params(
        "https://html.duckduckgo.com/html/",
        &[("q", &query)],
    ) {
        Ok(u) => u,
        Err(e) => return format!("URL builder error: {}", e),
    };
    match reqwest::get(url).await {
        Ok(res) => {
            if let Ok(text) = res.text().await {
                // Return a simplified snippet of the HTML or just the success text
                format!(
                    "Search returned {} bytes. (Consider parsing this with scraper/visdom)",
                    text.len()
                )
            } else {
                "Failed to read response text".into()
            }
        }
        Err(e) => format!("Search request failed: {}", e),
    }
}

/// Helper to parse basic tool JSON payload if the LLM uses Function Calling formatting
fn extract_json_field(json_str: &str, field: &str) -> Option<String> {
    // Falls back if serde_json is missing, but highly recommended to add `serde_json`
    serde_json::from_str::<serde_json::Value>(json_str)
        .ok()?
        .get(field)?
        .as_str()
        .map(|s| s.to_string())
}
