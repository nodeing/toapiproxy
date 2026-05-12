//! ThinkingProxy - HTTP 代理层，用于处理 Claude 模型的 thinking 参数
//!
//! 功能：
//! 1. 监听 8317 端口，转发请求到 8318 端口的 CLIProxyAPI
//! 2. 解析模型名称中的 `-thinking-NUMBER` 后缀，添加 thinking 参数
//! 3. 转发 Amp CLI 管理请求到 ampcode.com

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::async_runtime::{spawn, JoinHandle};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const PROXY_PORT: u16 = 8317;
const BACKEND_PORT: u16 = 8318;
const HARD_TOKEN_CAP: i64 = 32000;

pub struct ThinkingProxy {
    running: Arc<AtomicBool>,
    handle: std::sync::Mutex<Option<JoinHandle<()>>>,
}

impl ThinkingProxy {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            handle: std::sync::Mutex::new(None),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn proxy_port(&self) -> u16 {
        PROXY_PORT
    }

    /// 启动代理服务器
    pub fn start(&self) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Err("ThinkingProxy already running".into());
        }

        let running = self.running.clone();
        running.store(true, Ordering::SeqCst);

        let handle = spawn(async move {
            if let Err(e) = run_proxy(running.clone()).await {
                log::error!("[ThinkingProxy] Error: {}", e);
            }
            running.store(false, Ordering::SeqCst);
        });

        *self.handle.lock().unwrap() = Some(handle);
        log::info!("[ThinkingProxy] Started on port {}", PROXY_PORT);
        Ok(())
    }

    /// 停止代理服务器
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.lock().unwrap().take() {
            handle.abort();
        }
        log::info!("[ThinkingProxy] Stopped");
    }
}

impl Default for ThinkingProxy {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ThinkingProxy {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn run_proxy(
    running: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener_v4 = TcpListener::bind(format!("127.0.0.1:{}", PROXY_PORT)).await?;
    log::info!("[ThinkingProxy] Listening on 127.0.0.1:{}", PROXY_PORT);
    let listener_v6 = match TcpListener::bind(format!("[::1]:{}", PROXY_PORT)).await {
        Ok(listener) => {
            log::info!("[ThinkingProxy] Listening on [::1]:{}", PROXY_PORT);
            Some(listener)
        }
        Err(error) => {
            log::warn!(
                "[ThinkingProxy] IPv6 loopback unavailable on [::1]:{}: {}",
                PROXY_PORT,
                error
            );
            None
        }
    };

    while running.load(Ordering::SeqCst) {
        if let Some(listener_v6) = listener_v6.as_ref() {
            tokio::select! {
                result = listener_v4.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            log::debug!("[ThinkingProxy] Connection from {}", addr);
                            spawn(handle_connection(stream));
                        }
                        Err(e) => {
                            log::error!("[ThinkingProxy] Accept error: {}", e);
                        }
                    }
                }
                result = listener_v6.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            log::debug!("[ThinkingProxy] Connection from {}", addr);
                            spawn(handle_connection(stream));
                        }
                        Err(e) => {
                            log::error!("[ThinkingProxy] Accept error: {}", e);
                        }
                    }
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {}
            }
            continue;
        }

        tokio::select! {
            result = listener_v4.accept() => {
                match result {
                    Ok((stream, addr)) => {
                        log::debug!("[ThinkingProxy] Connection from {}", addr);
                        spawn(handle_connection(stream));
                    }
                    Err(e) => {
                        log::error!("[ThinkingProxy] Accept error: {}", e);
                    }
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                // 检查是否应该停止
            }
        }
    }

    Ok(())
}

async fn handle_connection(mut client: TcpStream) {
    let mut buffer = vec![0u8; 1024 * 1024]; // 1MB buffer
    let mut total_read = 0;

    // 读取请求
    loop {
        match client.read(&mut buffer[total_read..]).await {
            Ok(0) => break,
            Ok(n) => {
                total_read += n;
                // 检查是否收到完整的 HTTP 请求
                if let Some(header_end) = find_header_end(&buffer[..total_read]) {
                    if let Some(content_length) = parse_content_length(&buffer[..header_end]) {
                        let body_start = header_end;
                        let body_received = total_read - body_start;
                        if body_received >= content_length {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            Err(e) => {
                log::error!("[ThinkingProxy] Read error: {}", e);
                return;
            }
        }
    }

    if total_read == 0 {
        return;
    }

    let request = &buffer[..total_read];

    // 处理请求
    match process_request(request).await {
        Ok(response) => {
            if let Err(e) = client.write_all(&response).await {
                log::error!("[ThinkingProxy] Write error: {}", e);
            }
        }
        Err(e) => {
            let error_response = format!(
                "HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                e.len(),
                e
            );
            let _ = client.write_all(error_response.as_bytes()).await;
        }
    }
}

async fn process_request(request: &[u8]) -> Result<Vec<u8>, String> {
    let request_str = String::from_utf8_lossy(request);

    // 解析请求行
    let first_line = request_str.lines().next().ok_or("Empty request")?;
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 3 {
        return Err("Invalid request line".into());
    }

    let method = parts[0];
    let path = parts[1];

    // 检查是否是 Amp 管理请求
    if path.starts_with("/api/") && !path.starts_with("/api/provider/") {
        // 转发到 ampcode.com
        return forward_to_amp(request).await;
    }

    // 处理 thinking 参数
    let modified_request = if method == "POST" {
        process_thinking_parameter(request)?
    } else {
        request.to_vec()
    };

    // 转发到后端
    forward_to_backend(&modified_request).await
}

fn process_thinking_parameter(request: &[u8]) -> Result<Vec<u8>, String> {
    // 找到 body 开始位置
    let header_end = find_header_end(request).ok_or("No header end found")?;
    let headers = &request[..header_end - 4];
    let body = &request[header_end..];

    if body.is_empty() {
        return Ok(request.to_vec());
    }

    // 尝试解析 JSON body
    let body_str = String::from_utf8_lossy(body);
    let mut json: serde_json::Value = match serde_json::from_str(&body_str) {
        Ok(v) => v,
        Err(_) => return Ok(request.to_vec()),
    };

    // 检查模型名称
    let model = match json.get("model").and_then(|v| v.as_str()) {
        Some(m) => m.to_string(),
        None => return Ok(request.to_vec()),
    };

    // 只处理 Claude 模型
    if !model.starts_with("claude-") {
        return Ok(request.to_vec());
    }

    // 检查 -thinking-NUMBER 后缀
    if let Some(idx) = model.rfind("-thinking-") {
        let suffix = &model[idx + 10..];
        let clean_model = &model[..idx];

        // 更新模型名称
        json["model"] = serde_json::Value::String(clean_model.to_string());

        // 解析 thinking budget
        if let Ok(budget) = suffix.parse::<i64>() {
            if budget > 0 {
                let effective_budget = budget.min(HARD_TOKEN_CAP - 1);

                // 添加 thinking 参数
                json["thinking"] = serde_json::json!({
                    "type": "enabled",
                    "budget_tokens": effective_budget
                });

                // 确保 max_tokens 大于 thinking budget
                let required_max = effective_budget + 1024;
                let required_max = required_max.min(HARD_TOKEN_CAP);

                if let Some(max_tokens) = json.get("max_tokens").and_then(|v| v.as_i64()) {
                    if max_tokens <= effective_budget {
                        json["max_tokens"] = serde_json::Value::Number(required_max.into());
                    }
                } else {
                    json["max_tokens"] = serde_json::Value::Number(required_max.into());
                }

                log::info!(
                    "[ThinkingProxy] Transformed '{}' → '{}' with thinking budget {}",
                    model,
                    clean_model,
                    effective_budget
                );
            }
        }
    }

    // 重建请求
    let new_body = serde_json::to_vec(&json).map_err(|e| e.to_string())?;

    // 更新 Content-Length
    let headers_str = String::from_utf8_lossy(headers);
    let mut new_headers = String::new();
    for line in headers_str.lines() {
        if line.to_lowercase().starts_with("content-length:") {
            new_headers.push_str(&format!("Content-Length: {}\r\n", new_body.len()));
        } else {
            new_headers.push_str(line);
            new_headers.push_str("\r\n");
        }
    }

    let mut result = new_headers.into_bytes();
    result.extend_from_slice(b"\r\n");
    result.extend_from_slice(&new_body);
    Ok(result)
}

async fn forward_to_backend(request: &[u8]) -> Result<Vec<u8>, String> {
    let request = prepare_backend_request(request);
    let mut backend = TcpStream::connect(format!("127.0.0.1:{}", BACKEND_PORT))
        .await
        .map_err(|e| format!("Backend connection failed: {}", e))?;

    backend
        .write_all(&request)
        .await
        .map_err(|e| e.to_string())?;
    // Do not half-close the socket after sending the request body.
    // Go's HTTP server may treat the early FIN as a client disconnect and cancel the request context.

    let mut response = Vec::new();
    backend
        .read_to_end(&mut response)
        .await
        .map_err(|e| e.to_string())?;

    Ok(response)
}

fn prepare_backend_request(request: &[u8]) -> Vec<u8> {
    let Some(header_end) = find_header_end(request) else {
        return request.to_vec();
    };

    let headers = &request[..header_end - 4];
    let body = &request[header_end..];
    let headers_str = String::from_utf8_lossy(headers);
    let mut new_headers = String::new();
    let mut saw_host = false;
    let mut saw_connection = false;

    for line in headers_str.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("host:") {
            new_headers.push_str(&format!("Host: 127.0.0.1:{}\r\n", BACKEND_PORT));
            saw_host = true;
            continue;
        }
        if lower.starts_with("connection:") || lower.starts_with("proxy-connection:") {
            if !saw_connection {
                new_headers.push_str("Connection: close\r\n");
                saw_connection = true;
            }
            continue;
        }

        new_headers.push_str(line);
        new_headers.push_str("\r\n");
    }

    if !saw_host {
        new_headers.push_str(&format!("Host: 127.0.0.1:{}\r\n", BACKEND_PORT));
    }
    if !saw_connection {
        new_headers.push_str("Connection: close\r\n");
    }
    new_headers.push_str("\r\n");

    let mut result = new_headers.into_bytes();
    result.extend_from_slice(body);
    result
}

#[cfg(test)]
mod tests {
    use super::{prepare_backend_request, process_thinking_parameter};

    #[test]
    fn prepare_backend_request_rewrites_host_and_connection_headers() {
        let request = concat!(
            "POST /v1/responses HTTP/1.1\r\n",
            "Host: 127.0.0.1:8317\r\n",
            "Connection: keep-alive\r\n",
            "Content-Type: application/json\r\n",
            "Content-Length: 2\r\n",
            "\r\n",
            "{}"
        );

        let rewritten = String::from_utf8(prepare_backend_request(request.as_bytes()))
            .expect("request should stay utf-8");

        assert!(rewritten.contains("Host: 127.0.0.1:8318\r\n"));
        assert!(rewritten.contains("Connection: close\r\n"));
        assert!(!rewritten.contains("Connection: keep-alive\r\n"));
        assert!(rewritten.ends_with("\r\n\r\n{}"));
    }

    #[test]
    fn prepare_backend_request_keeps_connection_out_of_body() {
        let request = concat!(
            "POST /v1/responses HTTP/1.1\r\n",
            "Host: 127.0.0.1:8317\r\n",
            "Content-Type: application/json\r\n",
            "Content-Length: 56\r\n",
            "X-Api-Key: dummy-not-used\r\n",
            "\r\n",
            "{\"model\":\"gpt-5.4\",\"input\":\"ping\",\"max_output_tokens\":1}"
        );

        let rewritten = String::from_utf8(prepare_backend_request(request.as_bytes()))
            .expect("request should stay utf-8");

        assert!(rewritten.contains("Connection: close\r\n\r\n{\"model\":\"gpt-5.4\""));
        assert!(!rewritten.contains("\r\n\r\nConnection: close\r\n\r\n"));
    }

    #[test]
    fn process_thinking_parameter_preserves_header_body_boundary() {
        let request = concat!(
            "POST /v1/messages HTTP/1.1\r\n",
            "Host: 127.0.0.1:8317\r\n",
            "Content-Type: application/json\r\n",
            "Content-Length: 69\r\n",
            "\r\n",
            "{\"model\":\"claude-3-7-sonnet-thinking-2048\",\"messages\":[],\"max_tokens\":10}"
        );

        let rewritten = String::from_utf8(
            process_thinking_parameter(request.as_bytes())
                .expect("thinking rewrite should succeed"),
        )
        .expect("request should stay utf-8");

        let mut parts = rewritten.splitn(2, "\r\n\r\n");
        let header_text = parts.next().expect("headers should exist");
        let body_text = parts.next().expect("body should exist");

        assert!(header_text.contains("Content-Length: "));
        assert!(body_text.starts_with('{'));

        let body_json: serde_json::Value =
            serde_json::from_str(body_text).expect("body should remain valid json");
        assert_eq!(body_json["thinking"]["budget_tokens"], 2048);
        assert_eq!(body_json["thinking"]["type"], "enabled");
    }
}

async fn forward_to_amp(_request: &[u8]) -> Result<Vec<u8>, String> {
    // 简化实现：直接返回 404，因为 Amp 功能不是核心需求
    // 完整实现需要 HTTPS 客户端连接到 ampcode.com
    log::info!("[ThinkingProxy] Amp request detected, returning 404 (not implemented)");

    let body = "Amp integration not implemented";
    let response = format!(
        "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    Ok(response.into_bytes())
}

fn find_header_end(data: &[u8]) -> Option<usize> {
    for i in 0..data.len().saturating_sub(3) {
        if &data[i..i + 4] == b"\r\n\r\n" {
            return Some(i + 4);
        }
    }
    None
}

fn parse_content_length(headers: &[u8]) -> Option<usize> {
    let headers_str = String::from_utf8_lossy(headers);
    for line in headers_str.lines() {
        if line.to_lowercase().starts_with("content-length:") {
            if let Some(value) = line.split(':').nth(1) {
                return value.trim().parse().ok();
            }
        }
    }
    None
}
