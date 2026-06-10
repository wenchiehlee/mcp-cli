use crate::config::{
    debug, get_max_retries, get_retry_delay_ms, get_timeout_ms, is_daemon_enabled, ServerConfig,
};
use crate::daemon_client::{cleanup_orphaned_daemons, get_daemon_connection, DaemonConnection};
use crate::errors::{server_connection_error, tool_execution_error, CliError};
use crate::output::ToolInfo;
use futures_util::StreamExt;
use rand::Rng;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, Mutex};

// ============================================================================
// Stdio Client Implementation
// ============================================================================

type PendingRequests = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<serde_json::Value, String>>>>>;

#[derive(Clone)]
pub struct StdioClient {
    server_name: String,
    stdin_tx: tokio::sync::mpsc::Sender<String>,
    pending_requests: PendingRequests,
    next_id: Arc<AtomicU64>,
    child: Arc<Mutex<Option<Child>>>,
}

impl StdioClient {
    pub async fn connect(
        server_name: &str,
        config: &crate::config::StdioServerConfig,
    ) -> Result<Self, CliError> {
        let mut cmd = Command::new(&config.command);
        if let Some(ref args) = config.args {
            cmd.args(args);
        }

        // Environment variables
        let mut merged_env = HashMap::new();
        for (k, v) in std::env::vars() {
            merged_env.insert(k, v);
        }
        if let Some(ref env) = config.env {
            for (k, v) in env {
                merged_env.insert(k.clone(), v.clone());
            }
        }
        cmd.envs(merged_env);

        if let Some(ref cwd) = config.cwd {
            cmd.current_dir(cwd);
        }

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            server_connection_error(server_name, &format!("Failed to spawn process: {}", e))
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| server_connection_error(server_name, "Failed to open stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| server_connection_error(server_name, "Failed to open stdout"))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| server_connection_error(server_name, "Failed to open stderr"))?;

        let pending_requests: PendingRequests = Arc::new(Mutex::new(HashMap::new()));

        // Stderr forwarding task with prefix immediately
        let server_name_str = server_name.to_string();
        tokio::spawn(async move {
            let mut reader = BufReader::new(&mut stderr);
            let mut line = String::new();
            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 {
                    break;
                }
                eprint!("[{}] {}", server_name_str, line);
                line.clear();
            }
        });

        // Stdin writing task
        let (stdin_tx, mut stdin_rx) = tokio::sync::mpsc::channel::<String>(100);
        let mut stdin_writer = stdin;
        tokio::spawn(async move {
            while let Some(msg) = stdin_rx.recv().await {
                if stdin_writer.write_all(msg.as_bytes()).await.is_err() {
                    break;
                }
                if stdin_writer.write_all(b"\n").await.is_err() {
                    break;
                }
                if stdin_writer.flush().await.is_err() {
                    break;
                }
            }
        });

        // Stdout reading task
        let pending_requests_reader = Arc::clone(&pending_requests);
        let server_name_str2 = server_name.to_string();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 {
                    break;
                }
                let line_trimmed = line.trim();
                if line_trimmed.is_empty() {
                    line.clear();
                    continue;
                }

                if let Ok(val) = serde_json::from_str::<serde_json::Value>(line_trimmed) {
                    if let Some(id_val) = val.get("id") {
                        let id = id_val.as_u64();
                        if let Some(id_u64) = id {
                            let mut reqs = pending_requests_reader.lock().await;
                            if let Some(tx) = reqs.remove(&id_u64) {
                                if let Some(error) = val.get("error") {
                                    let msg = error
                                        .get("message")
                                        .and_then(|m| m.as_str())
                                        .unwrap_or("Unknown error");
                                    let _ = tx.send(Err(msg.to_string()));
                                } else {
                                    let result_val = val
                                        .get("result")
                                        .cloned()
                                        .unwrap_or(serde_json::Value::Null);
                                    let _ = tx.send(Ok(result_val));
                                }
                            }
                        }
                    }
                }
                line.clear();
            }

            // Cleanup pending on EOF
            let mut reqs = pending_requests_reader.lock().await;
            for (_, tx) in reqs.drain() {
                let _ = tx.send(Err("Server process exited unexpectedly".to_string()));
            }
            debug(&format!("Stdio connection to {} closed", server_name_str2));
        });

        let client = StdioClient {
            server_name: server_name.to_string(),
            stdin_tx,
            pending_requests,
            next_id: Arc::new(AtomicU64::new(1)),
            child: Arc::new(Mutex::new(Some(child))),
        };

        // Handshake: initialize
        let mut params = serde_json::Map::new();
        params.insert(
            "protocolVersion".to_string(),
            serde_json::json!("2024-11-05"),
        );
        params.insert("capabilities".to_string(), serde_json::json!({}));
        let mut client_info = serde_json::Map::new();
        client_info.insert("name".to_string(), serde_json::json!("mcp-cli"));
        client_info.insert(
            "version".to_string(),
            serde_json::json!(env!("CARGO_PKG_VERSION")),
        );
        params.insert(
            "clientInfo".to_string(),
            serde_json::Value::Object(client_info),
        );

        let response = client
            .request("initialize", serde_json::Value::Object(params))
            .await?;

        // Check protocol version
        let _server_protocol_version = response
            .get("protocolVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("2024-11-05");

        // Send notifications/initialized
        client
            .notify("notifications/initialized", serde_json::json!({}))
            .await?;

        Ok(client)
    }

    async fn request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, CliError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let mut req = serde_json::Map::new();
        req.insert("jsonrpc".to_string(), serde_json::json!("2.0"));
        req.insert("id".to_string(), serde_json::json!(id));
        req.insert("method".to_string(), serde_json::json!(method));
        req.insert("params".to_string(), params);

        let (tx, rx) = oneshot::channel();
        {
            let mut reqs = self.pending_requests.lock().await;
            reqs.insert(id, tx);
        }

        let serialized = serde_json::to_string(&req).unwrap();
        if self.stdin_tx.send(serialized).await.is_err() {
            let mut reqs = self.pending_requests.lock().await;
            reqs.remove(&id);
            return Err(server_connection_error(
                &self.server_name,
                "Failed to send request to process stdin",
            ));
        }

        match rx.await {
            Ok(Ok(res)) => Ok(res),
            Ok(Err(err)) => Err(tool_execution_error(method, &self.server_name, &err)),
            Err(_) => Err(server_connection_error(
                &self.server_name,
                "Stdio response receiver canceled",
            )),
        }
    }

    async fn notify(&self, method: &str, params: serde_json::Value) -> Result<(), CliError> {
        let mut req = serde_json::Map::new();
        req.insert("jsonrpc".to_string(), serde_json::json!("2.0"));
        req.insert("method".to_string(), serde_json::json!(method));
        req.insert("params".to_string(), params);

        let serialized = serde_json::to_string(&req).unwrap();
        if self.stdin_tx.send(serialized).await.is_err() {
            return Err(server_connection_error(
                &self.server_name,
                "Failed to send notification to process stdin",
            ));
        }
        Ok(())
    }

    pub async fn list_tools(&self) -> Result<Vec<ToolInfo>, CliError> {
        let response = self.request("tools/list", serde_json::json!({})).await?;
        let tools_val = response
            .get("tools")
            .and_then(|t| t.as_array())
            .ok_or_else(|| {
                tool_execution_error(
                    "tools/list",
                    &self.server_name,
                    "Invalid tools response format",
                )
            })?;

        let mut tools = Vec::new();
        for tool_val in tools_val {
            if let Ok(tool) = serde_json::from_value::<ToolInfo>(tool_val.clone()) {
                tools.push(tool);
            }
        }
        Ok(tools)
    }

    pub async fn call_tool(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, CliError> {
        let mut params = serde_json::Map::new();
        params.insert("name".to_string(), serde_json::json!(tool_name));
        params.insert("arguments".to_string(), args);

        self.request("tools/call", serde_json::Value::Object(params))
            .await
    }

    pub async fn get_instructions(&self) -> Result<Option<String>, CliError> {
        // Safe to call getInstructions, return None if not supported or not present
        match self.request("prompt/get", serde_json::json!({})).await {
            Ok(res) => {
                let inst = res
                    .get("instructions")
                    .and_then(|i| i.as_str())
                    .map(|s| s.to_string());
                Ok(inst)
            }
            Err(_) => Ok(None),
        }
    }

    pub async fn close(self) -> Result<(), CliError> {
        let mut child_guard = self.child.lock().await;
        if let Some(mut child) = child_guard.take() {
            let _ = child.kill().await;
        }
        Ok(())
    }
}

// ============================================================================
// Http Client Implementation (Streamable HTTP & Standard SSE)
// ============================================================================

#[derive(Clone)]
enum HttpMode {
    Streamable,
    StandardSse {
        post_url: Arc<Mutex<String>>,
        pending_requests: Arc<Mutex<HashMap<serde_json::Value, oneshot::Sender<Result<serde_json::Value, CliError>>>>>,
        next_id: Arc<AtomicU64>,
    },
}

#[derive(Clone)]
pub struct HttpClient {
    server_name: String,
    url: String,
    headers: HashMap<String, String>,
    client: reqwest::Client,
    session_id: Arc<Mutex<Option<String>>>,
    protocol_version: Arc<Mutex<Option<String>>>,
    mode: HttpMode,
}

async fn process_sse_event(
    event: &str,
    data: &str,
    base_url: &str,
    post_url: &Arc<Mutex<String>>,
    pending_requests: &Arc<Mutex<HashMap<serde_json::Value, oneshot::Sender<Result<serde_json::Value, CliError>>>>>,
    server_name: &str,
) {
    match event {
        "endpoint" => {
            let new_url = if data.starts_with("http://") || data.starts_with("https://") {
                data.to_string()
            } else {
                let base = base_url.trim_end_matches('/');
                let rel = data.trim_start_matches('/');
                format!("{}/{}", base, rel)
            };
            *post_url.lock().await = new_url;
            debug(&format!("[{}] Updated post_url to: {}", server_name, data));
        }
        "message" => {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                if let Some(id) = val.get("id") {
                    let mut reqs = pending_requests.lock().await;
                    if let Some(sender) = reqs.remove(id) {
                        if let Some(error) = val.get("error") {
                            let msg = error
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("Unknown error");
                            let _ = sender.send(Err(tool_execution_error("SSE message", server_name, msg)));
                        } else {
                            let result = val.get("result").cloned().unwrap_or(serde_json::Value::Null);
                            let _ = sender.send(Ok(result));
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

impl HttpClient {
    pub async fn connect(
        server_name: &str,
        config: &crate::config::HttpServerConfig,
    ) -> Result<Self, CliError> {
        let client = reqwest::Client::new();
        let mut headers = HashMap::new();
        if let Some(ref h) = config.headers {
            for (k, v) in h {
                headers.insert(k.clone(), v.clone());
            }
        }

        // 嘗試探測並建立標準 SSE 的 GET 連線，設定 5 秒超時
        let mut sse_builder = client.get(&config.url).header("accept", "text/event-stream");
        for (k, v) in &headers {
            sse_builder = sse_builder.header(k, v);
        }

        let sse_probe = tokio::time::timeout(
            Duration::from_secs(5),
            sse_builder.send()
        ).await;

        let mut is_standard_sse = false;
        let mut post_url_val = config.url.clone();
        let mut sse_stream_opt = None;

        if let Ok(Ok(resp)) = sse_probe {
            let status = resp.status();
            let content_type = resp.headers().get("content-type")
                .and_then(|c| c.to_str().ok())
                .unwrap_or("");
            if status.is_success() && content_type.contains("text/event-stream") {
                is_standard_sse = true;
                sse_stream_opt = Some(resp.bytes_stream());
            }
        }

        let mode = if is_standard_sse {
            let mut sse_stream = sse_stream_opt.unwrap();
            let mut current_event = String::new();
            let mut current_data = String::new();
            let mut found_endpoint = false;
            let mut buffer = String::new();

            // 讀取第一個 endpoint 事件，最多等待 5 秒
            let endpoint_probe = tokio::time::timeout(Duration::from_secs(5), async {
                while let Some(chunk_res) = sse_stream.next().await {
                    if let Ok(chunk) = chunk_res {
                        let text = String::from_utf8_lossy(&chunk);
                        buffer.push_str(&text);
                        while let Some(pos) = buffer.find('\n') {
                            let mut line = buffer.drain(..=pos).collect::<String>();
                            if line.ends_with('\n') { line.pop(); }
                            if line.ends_with('\r') { line.pop(); }
                            let line = line.trim();
                            if line.is_empty() {
                                if current_event == "endpoint" && !current_data.is_empty() {
                                    let data = &current_data;
                                    let resolved_url = if data.starts_with("http://") || data.starts_with("https://") {
                                        data.to_string()
                                    } else {
                                        let base = config.url.trim_end_matches('/');
                                        let rel = data.trim_start_matches('/');
                                        format!("{}/{}", base, rel)
                                    };
                                    post_url_val = resolved_url;
                                    found_endpoint = true;
                                    break;
                                }
                                current_event.clear();
                                current_data.clear();
                            } else if line.starts_with("event:") {
                                current_event = line.trim_start_matches("event:").trim().to_string();
                            } else if line.starts_with("data:") {
                                let data_line = line.trim_start_matches("data:").trim();
                                if current_data.is_empty() {
                                    current_data = data_line.to_string();
                                } else {
                                    current_data.push_str(data_line);
                                }
                            }
                        }
                    }
                    if found_endpoint {
                        break;
                    }
                }
                found_endpoint
            }).await;

            let endpoint_found = match endpoint_probe {
                Ok(found) => found,
                Err(_) => false,
            };

            if endpoint_found {
                let post_url = Arc::new(Mutex::new(post_url_val));
                let pending_requests = Arc::new(Mutex::new(HashMap::new()));
                let next_id = Arc::new(AtomicU64::new(1));

                // 啟動背景 task 監聽剩餘 of SSE stream
                let post_url_clone = Arc::clone(&post_url);
                let pending_requests_clone = Arc::clone(&pending_requests);
                let server_name_clone = server_name.to_string();
                let base_url_clone = config.url.clone();

                tokio::spawn(async move {
                    let mut stream = sse_stream;
                    let mut current_event = String::new();
                    let mut current_data = String::new();
                    let mut buffer = String::new();

                    while let Some(chunk_res) = stream.next().await {
                        let chunk = match chunk_res {
                            Ok(c) => c,
                            Err(e) => {
                                debug(&format!("SSE stream read error for {}: {}", server_name_clone, e));
                                break;
                            }
                        };
                        let text = String::from_utf8_lossy(&chunk);
                        buffer.push_str(&text);
                        while let Some(pos) = buffer.find('\n') {
                            let mut line = buffer.drain(..=pos).collect::<String>();
                            if line.ends_with('\n') { line.pop(); }
                            if line.ends_with('\r') { line.pop(); }
                            let line = line.trim();
                            if line.is_empty() {
                                if !current_event.is_empty() && !current_data.is_empty() {
                                    process_sse_event(
                                        &current_event,
                                        &current_data,
                                        &base_url_clone,
                                        &post_url_clone,
                                        &pending_requests_clone,
                                        &server_name_clone
                                    ).await;
                                }
                                current_event.clear();
                                current_data.clear();
                            } else if line.starts_with("event:") {
                                current_event = line.trim_start_matches("event:").trim().to_string();
                            } else if line.starts_with("data:") {
                                let data_line = line.trim_start_matches("data:").trim();
                                if current_data.is_empty() {
                                    current_data = data_line.to_string();
                                } else {
                                    current_data.push_str(data_line);
                                }
                            }
                        }
                    }
                    debug(&format!("SSE GET stream ended for standard SSE server {}", server_name_clone));
                });

                HttpMode::StandardSse {
                    post_url,
                    pending_requests,
                    next_id,
                }
            } else {
                HttpMode::Streamable
            }
        } else {
            HttpMode::Streamable
        };

        let http_client = HttpClient {
            server_name: server_name.to_string(),
            url: config.url.clone(),
            headers,
            client,
            session_id: Arc::new(Mutex::new(None)),
            protocol_version: Arc::new(Mutex::new(Some("2024-11-05".to_string()))),
            mode,
        };

        // Step 1: Handshake
        let mut params = serde_json::Map::new();
        params.insert(
            "protocolVersion".to_string(),
            serde_json::json!("2024-11-05"),
        );
        params.insert("capabilities".to_string(), serde_json::json!({}));
        let mut client_info = serde_json::Map::new();
        client_info.insert("name".to_string(), serde_json::json!("mcp-cli"));
        client_info.insert(
            "version".to_string(),
            serde_json::json!(env!("CARGO_PKG_VERSION")),
        );
        params.insert(
            "clientInfo".to_string(),
            serde_json::Value::Object(client_info),
        );

        http_client
            .request("initialize", serde_json::Value::Object(params))
            .await?;

        // initialized notification
        http_client
            .notify("notifications/initialized", serde_json::json!({}))
            .await?;

        Ok(http_client)
    }

    async fn request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, CliError> {
        match &self.mode {
            HttpMode::Streamable => self.request_streamable(method, params).await,
            HttpMode::StandardSse {
                post_url,
                pending_requests,
                next_id,
            } => {
                self.request_standard_sse(method, params, post_url, pending_requests, next_id).await
            }
        }
    }

    async fn request_standard_sse(
        &self,
        method: &str,
        params: serde_json::Value,
        post_url: &Arc<Mutex<String>>,
        pending_requests: &Arc<Mutex<HashMap<serde_json::Value, oneshot::Sender<Result<serde_json::Value, CliError>>>>>,
        next_id: &Arc<AtomicU64>,
    ) -> Result<serde_json::Value, CliError> {
        let id_num = next_id.fetch_add(1, Ordering::SeqCst);
        let id_val = serde_json::json!(id_num);

        let mut req = serde_json::Map::new();
        req.insert("jsonrpc".to_string(), serde_json::json!("2.0"));
        req.insert("id".to_string(), id_val.clone());
        req.insert("method".to_string(), serde_json::json!(method));
        req.insert("params".to_string(), params);

        let target_url = post_url.lock().await.clone();
        let mut builder = self.client.post(&target_url);

        builder = builder.header("content-type", "application/json");
        for (k, v) in &self.headers {
            builder = builder.header(k, v);
        }

        if let Some(ref sid) = *self.session_id.lock().await {
            builder = builder.header("mcp-session-id", sid);
        }
        if let Some(ref pv) = *self.protocol_version.lock().await {
            builder = builder.header("mcp-protocol-version", pv);
        }

        let (tx, rx) = oneshot::channel();
        {
            let mut reqs = pending_requests.lock().await;
            reqs.insert(id_val.clone(), tx);
        }

        let response = builder.json(&req).send().await.map_err(|e| {
            let pending_clone = Arc::clone(pending_requests);
            let id_clone = id_val.clone();
            tokio::spawn(async move {
                pending_clone.lock().await.remove(&id_clone);
            });
            server_connection_error(&self.server_name, &format!("HTTP request failed: {}", e))
        })?;

        if !response.status().is_success() {
            let mut reqs = pending_requests.lock().await;
            reqs.remove(&id_val);
            return Err(server_connection_error(
                &self.server_name,
                &format!("HTTP POST returned error status: {}", response.status()),
            ));
        }

        let timeout_duration = Duration::from_millis(get_timeout_ms());
        tokio::select! {
            res = rx => {
                match res {
                    Ok(rpc_res) => rpc_res,
                    Err(_) => Err(server_connection_error(
                        &self.server_name,
                        "Oneshot channel closed without response",
                    )),
                }
            }
            _ = tokio::time::sleep(timeout_duration) => {
                let mut reqs = pending_requests.lock().await;
                reqs.remove(&id_val);
                Err(server_connection_error(
                    &self.server_name,
                    "HTTP request timed out waiting for SSE message",
                ))
            }
        }
    }

    async fn request_streamable(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, CliError> {
        let mut req = serde_json::Map::new();
        req.insert("jsonrpc".to_string(), serde_json::json!("2.0"));
        req.insert("id".to_string(), serde_json::json!(1));
        req.insert("method".to_string(), serde_json::json!(method));
        req.insert("params".to_string(), params);

        let mut builder = self.client.post(&self.url);

        builder = builder.header("content-type", "application/json");
        builder = builder.header("accept", "application/json, text/event-stream");

        for (k, v) in &self.headers {
            builder = builder.header(k, v);
        }

        if let Some(ref sid) = *self.session_id.lock().await {
            builder = builder.header("mcp-session-id", sid);
        }

        if let Some(ref pv) = *self.protocol_version.lock().await {
            builder = builder.header("mcp-protocol-version", pv);
        }

        let response = builder.json(&req).send().await.map_err(|e| {
            server_connection_error(&self.server_name, &format!("HTTP request failed: {}", e))
        })?;

        if let Some(sid) = response.headers().get("mcp-session-id") {
            if let Ok(sid_str) = sid.to_str() {
                *self.session_id.lock().await = Some(sid_str.to_string());
            }
        }

        if !response.status().is_success() {
            return Err(server_connection_error(
                &self.server_name,
                &format!("HTTP POST returned error status: {}", response.status()),
            ));
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|c| c.to_str().ok())
            .unwrap_or("");

        if content_type.contains("text/event-stream") {
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            while let Some(chunk_res) = stream.next().await {
                let chunk = chunk_res.map_err(|e| {
                    server_connection_error(&self.server_name, &format!("SSE read failed: {}", e))
                })?;
                let text = String::from_utf8_lossy(&chunk);
                buffer.push_str(&text);
                while let Some(pos) = buffer.find('\n') {
                    let mut line = buffer.drain(..=pos).collect::<String>();
                    if line.ends_with('\n') { line.pop(); }
                    if line.ends_with('\r') { line.pop(); }
                    let line = line.trim();
                    if line.starts_with("data:") {
                        let data_json = line.trim_start_matches("data:").trim();
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(data_json) {
                            if let Some(error) = val.get("error") {
                                let msg = error
                                    .get("message")
                                    .and_then(|m| m.as_str())
                                    .unwrap_or("Unknown error");
                                return Err(tool_execution_error(method, &self.server_name, msg));
                            } else if let Some(result) = val.get("result") {
                                return Ok(result.clone());
                            }
                        }
                    }
                }
            }
            Err(server_connection_error(
                &self.server_name,
                "SSE stream ended without response",
            ))
        } else {
            let val = response.json::<serde_json::Value>().await.map_err(|e| {
                server_connection_error(&self.server_name, &format!("Invalid JSON response: {}", e))
            })?;

            if let Some(error) = val.get("error") {
                let msg = error
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown error");
                Err(tool_execution_error(method, &self.server_name, msg))
            } else {
                Ok(val
                    .get("result")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null))
            }
        }
    }

    async fn notify(&self, method: &str, params: serde_json::Value) -> Result<(), CliError> {
        match &self.mode {
            HttpMode::Streamable => self.notify_streamable(method, params).await,
            HttpMode::StandardSse { post_url, .. } => {
                self.notify_standard_sse(method, params, post_url).await
            }
        }
    }

    async fn notify_standard_sse(
        &self,
        method: &str,
        params: serde_json::Value,
        post_url: &Arc<Mutex<String>>,
    ) -> Result<(), CliError> {
        let mut req = serde_json::Map::new();
        req.insert("jsonrpc".to_string(), serde_json::json!("2.0"));
        req.insert("method".to_string(), serde_json::json!(method));
        req.insert("params".to_string(), params);

        let target_url = post_url.lock().await.clone();
        let mut builder = self.client.post(&target_url);
        builder = builder.header("content-type", "application/json");

        for (k, v) in &self.headers {
            builder = builder.header(k, v);
        }

        if let Some(ref sid) = *self.session_id.lock().await {
            builder = builder.header("mcp-session-id", sid);
        }
        if let Some(ref pv) = *self.protocol_version.lock().await {
            builder = builder.header("mcp-protocol-version", pv);
        }

        let response = builder.json(&req).send().await.map_err(|e| {
            server_connection_error(
                &self.server_name,
                &format!("HTTP notification failed: {}", e),
            )
        })?;

        if !response.status().is_success() {
            return Err(server_connection_error(
                &self.server_name,
                &format!("HTTP notification returned error status: {}", response.status()),
            ));
        }

        Ok(())
    }

    async fn notify_streamable(&self, method: &str, params: serde_json::Value) -> Result<(), CliError> {
        let mut req = serde_json::Map::new();
        req.insert("jsonrpc".to_string(), serde_json::json!("2.0"));
        req.insert("method".to_string(), serde_json::json!(method));
        req.insert("params".to_string(), params);

        let mut builder = self.client.post(&self.url);
        builder = builder.header("content-type", "application/json");

        for (k, v) in &self.headers {
            builder = builder.header(k, v);
        }

        if let Some(ref sid) = *self.session_id.lock().await {
            builder = builder.header("mcp-session-id", sid);
        }

        if let Some(ref pv) = *self.protocol_version.lock().await {
            builder = builder.header("mcp-protocol-version", pv);
        }

        let response = builder.json(&req).send().await.map_err(|e| {
            server_connection_error(
                &self.server_name,
                &format!("HTTP notification failed: {}", e),
            )
        })?;

        if response.status() == reqwest::StatusCode::ACCEPTED
            && method == "notifications/initialized"
        {
            let client = self.client.clone();
            let url = self.url.clone();
            let headers = self.headers.clone();
            let session_id_clone = Arc::clone(&self.session_id);
            let protocol_version_clone = Arc::clone(&self.protocol_version);
            let server_name_clone = self.server_name.clone();

            tokio::spawn(async move {
                let mut builder = client.get(&url).header("accept", "text/event-stream");
                for (k, v) in &headers {
                    builder = builder.header(k, v);
                }
                if let Some(ref sid) = *session_id_clone.lock().await {
                    builder = builder.header("mcp-session-id", sid);
                }
                if let Some(ref pv) = *protocol_version_clone.lock().await {
                    builder = builder.header("mcp-protocol-version", pv);
                }

                if let Ok(resp) = builder.send().await {
                    let mut stream = resp.bytes_stream();
                    while let Some(Ok(chunk)) = stream.next().await {
                        let text = String::from_utf8_lossy(&chunk);
                        for _line in text.lines() {}
                    }
                }
                debug(&format!(
                    "SSE GET stream ended for http server {}",
                    server_name_clone
                ));
            });
        }
        Ok(())
    }

    pub async fn list_tools(&self) -> Result<Vec<ToolInfo>, CliError> {
        let response = self.request("tools/list", serde_json::json!({})).await?;
        let tools_val = response
            .get("tools")
            .and_then(|t| t.as_array())
            .ok_or_else(|| {
                tool_execution_error(
                    "tools/list",
                    &self.server_name,
                    "Invalid tools response format",
                )
            })?;

        let mut tools = Vec::new();
        for tool_val in tools_val {
            if let Ok(tool) = serde_json::from_value::<ToolInfo>(tool_val.clone()) {
                tools.push(tool);
            }
        }
        Ok(tools)
    }

    pub async fn call_tool(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, CliError> {
        let mut params = serde_json::Map::new();
        params.insert("name".to_string(), serde_json::json!(tool_name));
        params.insert("arguments".to_string(), args);

        self.request("tools/call", serde_json::Value::Object(params))
            .await
    }

    pub async fn get_instructions(&self) -> Result<Option<String>, CliError> {
        match self.request("prompt/get", serde_json::json!({})).await {
            Ok(res) => {
                let inst = res
                    .get("instructions")
                    .and_then(|i| i.as_str())
                    .map(|s| s.to_string());
                Ok(inst)
            }
            Err(_) => Ok(None),
        }
    }

    pub async fn close(self) -> Result<(), CliError> {
        Ok(())
    }
}

// ============================================================================
// McpConnection Enum Wrapper
// ============================================================================

pub enum McpConnection {
    Stdio(StdioClient),
    Http(HttpClient),
    Daemon(DaemonConnection),
}

impl McpConnection {
    pub async fn list_tools(&self) -> Result<Vec<ToolInfo>, CliError> {
        match self {
            McpConnection::Stdio(c) => c.list_tools().await,
            McpConnection::Http(c) => c.list_tools().await,
            McpConnection::Daemon(c) => {
                let data = c.list_tools().await?;
                let tools: Vec<ToolInfo> = serde_json::from_value(data).map_err(|e| {
                    tool_execution_error("tools/list", &c.server_name, &e.to_string())
                })?;
                Ok(tools)
            }
        }
    }

    pub async fn call_tool(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, CliError> {
        match self {
            McpConnection::Stdio(c) => c.call_tool(tool_name, args).await,
            McpConnection::Http(c) => c.call_tool(tool_name, args).await,
            McpConnection::Daemon(c) => c.call_tool(tool_name, args).await,
        }
    }

    pub async fn get_instructions(&self) -> Result<Option<String>, CliError> {
        match self {
            McpConnection::Stdio(c) => c.get_instructions().await,
            McpConnection::Http(c) => c.get_instructions().await,
            McpConnection::Daemon(c) => c.get_instructions().await,
        }
    }

    pub async fn close(self) -> Result<(), CliError> {
        match self {
            McpConnection::Stdio(c) => c.close().await,
            McpConnection::Http(c) => c.close().await,
            McpConnection::Daemon(c) => c.close().await,
        }
    }

    pub fn is_daemon(&self) -> bool {
        matches!(self, McpConnection::Daemon(_))
    }
}

// ============================================================================
// Transient Error Detection & Retry
// ============================================================================

pub fn is_transient_error(error_msg: &str) -> bool {
    let lower = error_msg.to_lowercase();
    lower.contains("econnrefused")
        || lower.contains("econnreset")
        || lower.contains("etimedout")
        || lower.contains("enotfound")
        || lower.contains("epipe")
        || lower.contains("enetunreach")
        || lower.contains("ehostunreach")
        || lower.contains("eai_again")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("504")
        || lower.contains("429")
        || lower.contains("network error")
        || lower.contains("network fail")
        || lower.contains("connection reset")
        || lower.contains("connection refused")
        || lower.contains("connection timeout")
        || lower.contains("timeout")
}

pub async fn with_retry<F, Fut, T>(mut f: F, op_name: &str) -> Result<T, CliError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, CliError>>,
{
    let max_retries = get_max_retries();
    let base_delay = get_retry_delay_ms();
    let total_budget_ms = get_timeout_ms();
    let start_time = Instant::now();

    for attempt in 0..=max_retries {
        let elapsed = start_time.elapsed().as_millis() as u64;
        if elapsed >= total_budget_ms {
            debug(&format!(
                "{}: timeout budget exhausted after {}ms",
                op_name, elapsed
            ));
            break;
        }

        match f().await {
            Ok(res) => return Ok(res),
            Err(e) => {
                let is_transient = is_transient_error(&e.to_string());
                let remaining_budget =
                    total_budget_ms.saturating_sub(start_time.elapsed().as_millis() as u64);
                let should_retry = attempt < max_retries && is_transient && remaining_budget > 1000;

                if should_retry {
                    let expo_delay = base_delay * 2_u64.pow(attempt);
                    let capped_delay = std::cmp::min(expo_delay, 10000);
                    // Jitter (+-25%)
                    let jitter =
                        capped_delay as f64 * 0.25 * (rand::thread_rng().gen_range(0.0..2.0) - 1.0);
                    let final_delay = std::cmp::min(
                        (capped_delay as f64 + jitter).round() as u64,
                        remaining_budget.saturating_sub(1000),
                    );

                    debug(&format!(
                        "{} failed (attempt {}/{}): {}. Retrying in {}ms...",
                        op_name,
                        attempt + 1,
                        max_retries + 1,
                        e.message,
                        final_delay
                    ));
                    tokio::time::sleep(Duration::from_millis(final_delay)).await;
                } else {
                    return Err(e);
                }
            }
        }
    }

    Err(server_connection_error(
        op_name,
        "Max retry attempts or timeout budget exceeded",
    ))
}

// ============================================================================
// Unified Get Connection
// ============================================================================

pub async fn get_connection(
    server_name: &str,
    config: &ServerConfig,
) -> Result<McpConnection, CliError> {
    cleanup_orphaned_daemons().await;

    if is_daemon_enabled() {
        match get_daemon_connection(server_name, config).await {
            Some(daemon_conn) => {
                debug(&format!("Using daemon connection for {}", server_name));
                return Ok(McpConnection::Daemon(daemon_conn));
            }
            None => {
                debug(&format!(
                    "Daemon connection not available for {}, falling back to direct",
                    server_name
                ));
            }
        }
    }

    debug(&format!("Using direct connection for {}", server_name));
    match config {
        ServerConfig::Stdio(sc) => {
            let client = with_retry(
                || StdioClient::connect(server_name, sc),
                &format!("connect to {}", server_name),
            )
            .await?;
            Ok(McpConnection::Stdio(client))
        }
        ServerConfig::Http(hc) => {
            let client = with_retry(
                || HttpClient::connect(server_name, hc),
                &format!("connect to {}", server_name),
            )
            .await?;
            Ok(McpConnection::Http(client))
        }
    }
}
