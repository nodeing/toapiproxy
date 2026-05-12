use crate::auth::{AuthManager, ServiceType};
use crate::backend_usage::{fetch_backend_usage_statistics, BackendUsageSnapshot};
use crate::claude_providers::{
    apply_claude_provider as apply_claude_provider_impl,
    delete_claude_provider as delete_claude_provider_impl,
    duplicate_claude_provider as duplicate_claude_provider_impl, list_claude_providers,
    set_claude_provider_enabled as set_claude_provider_enabled_impl,
    test_claude_provider_connectivity as test_claude_provider_connectivity_impl,
    upsert_claude_provider, ClaudeProviderSummary, ClaudeProviderUpsertInput,
};
use crate::codex::{CodexAccountSnapshot, CodexClient, CodexKey};
use crate::codex_config::{
    apply_codex_config_profile as apply_codex_config_profile_impl,
    delete_codex_config_profile as delete_codex_config_profile_impl,
    duplicate_codex_config_profile as duplicate_codex_config_profile_impl,
    list_codex_config_profiles, upsert_codex_config_profile, CodexConfigProfileSummary,
    CodexConfigUpsertInput,
};
use crate::droid_models::{
    delete_droid_custom_model as delete_droid_custom_model_impl,
    duplicate_droid_custom_model as duplicate_droid_custom_model_impl, list_droid_custom_models,
    set_droid_default_model as set_droid_default_model_impl, upsert_droid_custom_model,
    DroidCustomModelSummary, DroidCustomModelUpsertInput,
};
use crate::management::{ManagementClient, ServiceRoutingOverview};
use crate::server::{AuthResult, ProxyServer};
use crate::thinking_proxy::ThinkingProxy;
use crate::usage::UsageClient;
use crate::watcher::AuthFileWatcher;
use regex::Regex;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};
use tauri::{Manager, State};

// App State
pub struct AppState {
    pub server: ProxyServer,
    pub auth_manager: Mutex<AuthManager>,
    pub server_running: Mutex<bool>,
    pub file_watcher: Mutex<AuthFileWatcher>,
    pub usage_client: UsageClient,
    pub thinking_proxy: Mutex<ThinkingProxy>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            server: ProxyServer::new(8317, 8318),
            auth_manager: Mutex::new(AuthManager::new()),
            server_running: Mutex::new(false),
            file_watcher: Mutex::new(AuthFileWatcher::new()),
            usage_client: UsageClient::new(),
            thinking_proxy: Mutex::new(ThinkingProxy::new()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn start_proxy_stack(state: &AppState) -> Result<(), String> {
    state.server.start()?;

    let proxy_result = state
        .thinking_proxy
        .lock()
        .map_err(|e| e.to_string())?
        .start();

    if let Err(err) = proxy_result {
        let _ = state.server.stop();
        return Err(format!("Failed to start ThinkingProxy: {}", err));
    }

    Ok(())
}

pub(crate) fn stop_proxy_stack(state: &AppState) -> Result<(), String> {
    if let Ok(proxy) = state.thinking_proxy.lock() {
        proxy.stop();
    }

    state.server.stop()
}

#[derive(Serialize)]
pub struct AppStateResponse {
    #[serde(rename = "serverRunning")]
    pub server_running: bool,
    pub accounts: Vec<AccountResponse>,
    pub services: Vec<ServiceStatus>,
    pub logs: Vec<String>,
}

#[derive(Serialize)]
pub struct AccountResponse {
    pub id: String,
    pub email: String,
    pub provider: String,
    pub subscription: Option<String>,
    pub usage: Option<UsageResponse>,
    #[serde(rename = "isExpired")]
    pub is_expired: bool,
}

#[derive(Serialize)]
pub struct UsageResponse {
    pub used: i32,
    pub limit: i32,
    pub percent: i32,
    #[serde(rename = "resetDays")]
    pub reset_days: Option<i32>,
    #[serde(rename = "bonusUsed")]
    pub bonus_used: Option<i32>,
    #[serde(rename = "bonusLimit")]
    pub bonus_limit: Option<i32>,
}

#[derive(Serialize)]
pub struct ServiceStatus {
    pub id: String,
    pub name: String,
    pub connected: bool,
    #[serde(rename = "accountCount")]
    pub account_count: usize,
}

// Commands
#[tauri::command]
pub fn get_app_state(state: State<AppState>) -> AppStateResponse {
    let server_running = *state.server_running.lock().unwrap();
    let mut auth_manager = state.auth_manager.lock().unwrap();

    let _ = auth_manager.scan_accounts();

    let accounts: Vec<AccountResponse> = auth_manager
        .get_all_accounts()
        .into_iter()
        .map(|a| AccountResponse {
            id: a.id,
            email: a.email,
            provider: a.provider,
            subscription: a.subscription,
            is_expired: a.is_expired,
            usage: a.usage.map(|u| UsageResponse {
                used: u.used,
                limit: u.limit,
                percent: u.percent,
                reset_days: u.reset_days,
                bonus_used: u.bonus_used,
                bonus_limit: u.bonus_limit,
            }),
        })
        .collect();

    let services = vec![
        ServiceStatus {
            id: "claude".to_string(),
            name: "Claude".to_string(),
            connected: auth_manager.is_connected(ServiceType::Claude),
            account_count: auth_manager.get_accounts(ServiceType::Claude).len(),
        },
        ServiceStatus {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            connected: auth_manager.is_connected(ServiceType::Codex),
            account_count: auth_manager.get_accounts(ServiceType::Codex).len(),
        },
        ServiceStatus {
            id: "gemini".to_string(),
            name: "Gemini".to_string(),
            connected: auth_manager.is_connected(ServiceType::Gemini),
            account_count: auth_manager.get_accounts(ServiceType::Gemini).len(),
        },
        ServiceStatus {
            id: "copilot".to_string(),
            name: "GitHub Copilot".to_string(),
            connected: auth_manager.is_connected(ServiceType::Copilot),
            account_count: auth_manager.get_accounts(ServiceType::Copilot).len(),
        },
        ServiceStatus {
            id: "qwen".to_string(),
            name: "Qwen".to_string(),
            connected: auth_manager.is_connected(ServiceType::Qwen),
            account_count: auth_manager.get_accounts(ServiceType::Qwen).len(),
        },
        ServiceStatus {
            id: "kiro".to_string(),
            name: "Kiro".to_string(),
            connected: auth_manager.is_connected(ServiceType::Kiro),
            account_count: auth_manager.get_accounts(ServiceType::Kiro).len(),
        },
        ServiceStatus {
            id: "antigravity".to_string(),
            name: "Antigravity".to_string(),
            connected: auth_manager.is_connected(ServiceType::Antigravity),
            account_count: auth_manager.get_accounts(ServiceType::Antigravity).len(),
        },
    ];

    let logs = state.server.get_logs();

    AppStateResponse {
        server_running,
        accounts,
        services,
        logs,
    }
}

#[tauri::command]
pub fn start_server(state: State<AppState>) -> Result<String, String> {
    let mut running = state.server_running.lock().unwrap();
    if *running {
        return Err("服务器已在运行".to_string());
    }

    start_proxy_stack(state.inner())?;
    *running = true;

    Ok(format!("服务器已启动，端口 {}", state.server.proxy_port()))
}

#[tauri::command]
pub fn stop_server(state: State<AppState>) -> Result<String, String> {
    let mut running = state.server_running.lock().unwrap();
    if !*running {
        return Err("服务器未运行".to_string());
    }

    stop_proxy_stack(state.inner())?;
    *running = false;

    Ok("服务器已停止".to_string())
}

#[tauri::command]
pub fn connect_service(
    state: State<AppState>,
    service_id: String,
    qwen_email: Option<String>,
) -> Result<AuthResult, String> {
    let qwen_email_log = qwen_email.as_deref().map(mask_secret_tail);
    log::info!(
        "Connecting to service: {}, email: {:?}",
        service_id,
        qwen_email_log
    );
    state
        .server
        .run_auth_command(&service_id, qwen_email.as_deref())
}

#[tauri::command]
pub fn disconnect_service(state: State<AppState>, account_id: String) -> Result<String, String> {
    log::info!("Disconnecting account: {}", mask_secret_tail(&account_id));

    let mut auth_manager = state.auth_manager.lock().unwrap();
    auth_manager.remove_account(&account_id)?;

    Ok("已断开连接".to_string())
}

#[tauri::command]
pub fn remove_account(state: State<AppState>, account_id: String) -> Result<String, String> {
    let mut auth_manager = state.auth_manager.lock().unwrap();
    auth_manager.remove_account(&account_id)?;
    Ok("账户已删除".to_string())
}

#[tauri::command]
pub async fn fetch_usage(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<UsageResponse, String> {
    log::info!(
        "Fetching usage for account: {}",
        mask_secret_tail(&account_id)
    );

    // 获取最新的 token
    let token = {
        let auth_manager = state.auth_manager.lock().unwrap();
        auth_manager.get_fresh_token(&account_id)
    };

    let token = token.ok_or("No access token found for this account")?;

    let (usage, _email, _subscription) = state.usage_client.fetch_kiro_usage(&token).await?;

    Ok(UsageResponse {
        used: usage.used,
        limit: usage.limit,
        percent: usage.percent,
        reset_days: usage.reset_days,
        bonus_used: usage.bonus_used,
        bonus_limit: usage.bonus_limit,
    })
}

#[tauri::command]
pub async fn fetch_all_usage(
    state: State<'_, AppState>,
) -> Result<Vec<AccountUsageResult>, String> {
    log::info!("Fetching usage for all Kiro accounts");

    let kiro_accounts: Vec<_> = {
        let mut auth_manager = state.auth_manager.lock().unwrap();
        let _ = auth_manager.scan_accounts();
        auth_manager.get_accounts(ServiceType::Kiro)
    };

    let mut results = Vec::new();

    for account in kiro_accounts {
        if let Some(token) = &account.access_token {
            match state.usage_client.fetch_kiro_usage(token).await {
                Ok((usage, email, subscription)) => {
                    results.push(AccountUsageResult {
                        account_id: account.id.clone(),
                        email: email.unwrap_or(account.email.clone()),
                        subscription,
                        usage: Some(UsageResponse {
                            used: usage.used,
                            limit: usage.limit,
                            percent: usage.percent,
                            reset_days: usage.reset_days,
                            bonus_used: usage.bonus_used,
                            bonus_limit: usage.bonus_limit,
                        }),
                        error: None,
                    });
                }
                Err(e) => {
                    results.push(AccountUsageResult {
                        account_id: account.id.clone(),
                        email: account.email.clone(),
                        subscription: None,
                        usage: None,
                        error: Some(e),
                    });
                }
            }
        }
    }

    Ok(results)
}

#[derive(Serialize)]
pub struct AccountUsageResult {
    #[serde(rename = "accountId")]
    pub account_id: String,
    pub email: String,
    pub subscription: Option<String>,
    pub usage: Option<UsageResponse>,
    pub error: Option<String>,
}

#[tauri::command]
pub fn import_from_kiro_ide(state: State<AppState>) -> Result<AccountResponse, String> {
    log::info!("Importing from Kiro IDE...");

    let mut auth_manager = state.auth_manager.lock().unwrap();
    let account = auth_manager.import_from_kiro_ide()?;

    Ok(AccountResponse {
        id: account.id,
        email: account.email,
        provider: account.provider,
        subscription: account.subscription,
        is_expired: account.is_expired,
        usage: account.usage.map(|u| UsageResponse {
            used: u.used,
            limit: u.limit,
            percent: u.percent,
            reset_days: u.reset_days,
            bonus_used: u.bonus_used,
            bonus_limit: u.bonus_limit,
        }),
    })
}

#[tauri::command]
pub fn open_auth_folder(state: State<AppState>) -> Result<(), String> {
    let auth_manager = state.auth_manager.lock().unwrap();
    let auth_dir = auth_manager.auth_dir();

    if !auth_dir.exists() {
        std::fs::create_dir_all(auth_dir).map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(auth_dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(auth_dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(auth_dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub fn get_server_logs(state: State<AppState>) -> Vec<String> {
    state.server.get_logs()
}

#[tauri::command]
pub fn clear_server_logs(state: State<AppState>) {
    state.server.clear_logs();
}

#[derive(Serialize)]
pub struct DiagnosticReportResponse {
    #[serde(rename = "logDir")]
    pub log_dir: String,
    #[serde(rename = "lineCount")]
    pub line_count: usize,
}

#[tauri::command]
pub fn record_frontend_log(
    level: String,
    message: String,
    context: Option<String>,
) -> Result<(), String> {
    let context = context
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "frontend".to_string());
    let message = truncate_text(&redact_sensitive(&message), 4000);

    match level.trim().to_ascii_lowercase().as_str() {
        "error" => log::error!(target: "frontend", "[{}] {}", context, message),
        "warn" | "warning" => log::warn!(target: "frontend", "[{}] {}", context, message),
        _ => log::info!(target: "frontend", "[{}] {}", context, message),
    }

    Ok(())
}

#[tauri::command]
pub fn open_log_folder(app_handle: tauri::AppHandle) -> Result<(), String> {
    let log_dir = resolve_log_dir(&app_handle)?;
    fs::create_dir_all(&log_dir).map_err(|error| format!("创建日志目录失败: {}", error))?;
    open_folder(&log_dir)
}

#[tauri::command]
pub fn copy_diagnostic_report(
    state: State<AppState>,
    app_handle: tauri::AppHandle,
) -> Result<DiagnosticReportResponse, String> {
    let log_dir = resolve_log_dir(&app_handle)?;
    let report = build_diagnostic_report(state.inner(), &app_handle, &log_dir)?;
    let line_count = report.lines().count();

    let mut clipboard = arboard::Clipboard::new().map_err(|error| {
        format!(
            "无法访问剪贴板，请打开日志目录后手动发送日志文件: {}",
            error
        )
    })?;
    clipboard
        .set_text(report)
        .map_err(|error| format!("写入剪贴板失败: {}", error))?;

    Ok(DiagnosticReportResponse {
        log_dir: display_path(&log_dir),
        line_count,
    })
}

fn build_diagnostic_report(
    state: &AppState,
    app_handle: &tauri::AppHandle,
    log_dir: &Path,
) -> Result<String, String> {
    let package_info = app_handle.package_info();
    let server_running = *state
        .server_running
        .lock()
        .map_err(|error| error.to_string())?;
    let server_logs = state.server.get_logs();
    let log_files = collect_log_files(log_dir);

    let mut report = String::new();
    report.push_str("# TOAPIPROXY 错误报告\n\n");
    report.push_str("## 基本信息\n");
    report.push_str(&format!(
        "- 生成时间: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S %z")
    ));
    report.push_str(&format!("- 应用版本: {}\n", package_info.version));
    report.push_str(&format!(
        "- 系统: {} {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    report.push_str(&format!(
        "- 代理状态: {}\n",
        if server_running {
            "运行中"
        } else {
            "已停止"
        }
    ));
    report.push_str(&format!("- 代理端口: {}\n", state.server.proxy_port()));
    report.push_str(&format!("- 后端端口: {}\n", state.server.backend_port()));
    report.push_str(&format!("- 日志目录: {}\n", display_path(log_dir)));

    report.push_str("\n## 日志文件\n");
    if log_files.is_empty() {
        report.push_str("- 未找到日志文件\n");
    } else {
        for path in log_files.iter().take(8) {
            let size = fs::metadata(path)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            report.push_str(&format!(
                "- {} ({} bytes)\n",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("unknown.log"),
                size
            ));
        }
    }

    report.push_str("\n## 界面服务日志\n");
    if server_logs.is_empty() {
        report.push_str("暂无\n");
    } else {
        let start = server_logs.len().saturating_sub(80);
        for line in &server_logs[start..] {
            report.push_str(&redact_sensitive(line));
            report.push('\n');
        }
    }

    report.push_str("\n## 最新应用日志\n");
    if let Some(latest_log) = log_files.first() {
        if let Some(contents) = read_tail_text(latest_log, 80 * 1024) {
            report.push_str(&redact_sensitive(&contents));
            if !contents.ends_with('\n') {
                report.push('\n');
            }
        } else {
            report.push_str("无法读取最新日志文件\n");
        }
    } else {
        report.push_str("暂无\n");
    }

    Ok(report)
}

fn resolve_log_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    app_handle
        .path()
        .app_log_dir()
        .map_err(|error| format!("获取日志目录失败: {}", error))
}

fn collect_log_files(log_dir: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_dir(log_dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("log"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    files.sort_by(|left, right| modified_at(right).cmp(&modified_at(left)));
    files
}

fn modified_at(path: &Path) -> SystemTime {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn read_tail_text(path: &Path, max_bytes: usize) -> Option<String> {
    let data = fs::read(path).ok()?;
    let start = data.len().saturating_sub(max_bytes);
    Some(String::from_utf8_lossy(&data[start..]).to_string())
}

fn display_path(path: &Path) -> String {
    let raw = path.display().to_string();
    if let Some(home_dir) = dirs::home_dir() {
        let home = home_dir.display().to_string();
        if raw == home {
            return "~".to_string();
        }
        if let Some(rest) = raw.strip_prefix(&(home + std::path::MAIN_SEPARATOR_STR)) {
            return format!("~{}{}", std::path::MAIN_SEPARATOR, rest);
        }
    }
    raw
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        truncated.push_str("\n...[truncated]");
    }
    truncated
}

fn redact_sensitive(value: &str) -> String {
    let mut redacted = value.to_string();
    if let Some(home_dir) = dirs::home_dir() {
        let home = home_dir.display().to_string();
        redacted = redacted.replace(&home, "~");
    }

    let key_value_patterns = [
        r#"(?i)\b(authorization\s*[:=]\s*)bearer\s+[A-Za-z0-9._-]{20,}"#,
        r#"(?i)\b((api[-_ ]?key|access[_-]?token|refresh[_-]?token|id[_-]?token|token|secret|password)\s*[:=]\s*)[^\s,"']+"#,
    ];

    for pattern in key_value_patterns {
        if let Ok(regex) = Regex::new(pattern) {
            redacted = regex.replace_all(&redacted, "${1}[REDACTED]").into_owned();
        }
    }

    let secret_patterns = [
        r#"sk-(proj-)?[A-Za-z0-9_-]{20,}"#,
        r#"sk-ant-[A-Za-z0-9_-]{20,}"#,
        r#"(ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9_]{30,}"#,
        r#"github_pat_[A-Za-z0-9_]{20,}"#,
        r#"AIza[0-9A-Za-z_-]{35}"#,
        r#"eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}"#,
        r#"(?i)[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}"#,
    ];

    for pattern in secret_patterns {
        if let Ok(regex) = Regex::new(pattern) {
            redacted = regex.replace_all(&redacted, "[REDACTED]").into_owned();
        }
    }

    redacted
}

fn mask_secret_tail(value: &str) -> String {
    let tail = value.chars().rev().take(4).collect::<Vec<_>>();
    if tail.is_empty() {
        return "[empty]".to_string();
    }

    let tail = tail.into_iter().rev().collect::<String>();
    format!("***{}", tail)
}

fn open_folder(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|error| error.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|error| error.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub fn start_file_watcher(
    state: State<AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let mut watcher = state.file_watcher.lock().map_err(|e| e.to_string())?;
    watcher.start(app_handle)
}

#[tauri::command]
pub fn stop_file_watcher(state: State<AppState>) -> Result<(), String> {
    let mut watcher = state.file_watcher.lock().map_err(|e| e.to_string())?;
    watcher.stop();
    Ok(())
}

#[tauri::command]
pub fn open_external_url(app_handle: tauri::AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("链接地址不能为空".to_string());
    }

    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err("仅支持打开 http 或 https 链接".to_string());
    }

    app_handle
        .opener()
        .open_url(trimmed, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn copy_server_url(state: State<AppState>) -> Result<String, String> {
    let url = format!("http://127.0.0.1:{}", state.server.proxy_port());

    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        clipboard.set_text(&url).map_err(|e| e.to_string())?;
        Ok(url)
    } else {
        Err("无法访问剪贴板".to_string())
    }
}

#[tauri::command]
pub fn get_autostart_enabled(app_handle: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;

    app_handle
        .autolaunch()
        .is_enabled()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_autostart_enabled(app_handle: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;

    let autostart = app_handle.autolaunch();

    if enabled {
        autostart.enable().map_err(|e| e.to_string())
    } else {
        autostart.disable().map_err(|e| e.to_string())
    }
}

// ThinkingProxy Commands
#[tauri::command]
pub fn start_thinking_proxy(state: State<AppState>) -> Result<String, String> {
    let proxy = state.thinking_proxy.lock().map_err(|e| e.to_string())?;
    proxy.start()?;
    Ok(format!("ThinkingProxy 已启动，端口 {}", proxy.proxy_port()))
}

#[tauri::command]
pub fn stop_thinking_proxy(state: State<AppState>) -> Result<String, String> {
    let proxy = state.thinking_proxy.lock().map_err(|e| e.to_string())?;
    proxy.stop();
    Ok("ThinkingProxy 已停止".to_string())
}

#[tauri::command]
pub fn is_thinking_proxy_running(state: State<AppState>) -> bool {
    state
        .thinking_proxy
        .lock()
        .map(|p| p.is_running())
        .unwrap_or(false)
}

// Codex Commands
#[tauri::command]
pub async fn get_codex_keys() -> Result<Vec<CodexKey>, String> {
    log::info!("Fetching Codex API keys");
    let client = CodexClient::new();
    client.get_codex_keys().await
}

#[tauri::command]
pub async fn get_codex_accounts(
    state: State<'_, AppState>,
) -> Result<Vec<CodexAccountSnapshot>, String> {
    log::info!("Fetching Codex account snapshots");
    let client = CodexClient::with_config_path(state.server.config_path());
    client.get_codex_accounts().await
}

#[tauri::command]
pub async fn add_codex_key(api_key: String, base_url: Option<String>) -> Result<(), String> {
    log::info!("Adding Codex API key");
    let client = CodexClient::new();
    // 先检查后端是否运行
    match client.check_backend().await {
        Ok(_) => {}
        Err(e) => return Err(e),
    }
    client.add_codex_key(&api_key, base_url.as_deref()).await
}

#[tauri::command]
pub async fn delete_codex_key(api_key: String) -> Result<(), String> {
    log::info!("Deleting Codex API key: {}", mask_secret_tail(&api_key));
    let client = CodexClient::new();
    client.delete_codex_key(&api_key).await
}

#[tauri::command]
pub async fn delete_codex_account(
    state: State<'_, AppState>,
    account_ref: String,
) -> Result<(), String> {
    log::info!("Deleting Codex account: {}", mask_secret_tail(&account_ref));
    let client = CodexClient::with_config_path(state.server.config_path());
    client.delete_codex_account(&account_ref).await
}

#[tauri::command]
pub async fn import_codex_token(state: State<'_, AppState>) -> Result<String, String> {
    log::info!("Importing Codex token from local Codex CLI");
    let client = CodexClient::with_config_path(state.server.config_path());
    client.import_codex_token().await
}

#[tauri::command]
pub async fn get_service_routing_overview(
    state: State<'_, AppState>,
) -> Result<ServiceRoutingOverview, String> {
    let backend_port = state.server.backend_port();
    let client = ManagementClient::new(backend_port);
    client.get_service_routing_overview().await
}

#[tauri::command]
pub async fn apply_service_account_mode(
    state: State<'_, AppState>,
    service_id: String,
    mode: String,
    preferred_account_name: Option<String>,
) -> Result<String, String> {
    let backend_port = state.server.backend_port();
    let client = ManagementClient::new(backend_port);

    client
        .apply_service_account_mode(&service_id, &mode, preferred_account_name.as_deref())
        .await?;

    Ok(match mode.trim().to_ascii_lowercase().as_str() {
        "preferred" | "preferredaccount" | "manual" | "manualselect" => {
            if let Some(account_name) = preferred_account_name {
                format!(
                    "Preferred account updated for {}: {}",
                    service_id, account_name
                )
            } else {
                format!("Preferred account mode updated for {}", service_id)
            }
        }
        _ => format!("Round-robin mode restored for {}", service_id),
    })
}

#[tauri::command]
pub async fn get_backend_usage_statistics(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    time_range: Option<String>,
) -> Result<BackendUsageSnapshot, String> {
    let backend_port = state.server.backend_port();
    let server_running = *state.server_running.lock().map_err(|e| e.to_string())?;
    let app_data_dir = app_handle
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Failed to resolve app data directory: {}", error))?;

    fetch_backend_usage_statistics(
        backend_port,
        &app_data_dir,
        time_range.as_deref(),
        server_running,
    )
    .await
}

#[tauri::command]
pub async fn get_claude_providers(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<Vec<ClaudeProviderSummary>, String> {
    let backend_port = state.server.backend_port();
    list_claude_providers(backend_port, &app_handle).await
}

#[tauri::command]
pub async fn save_claude_provider(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    provider: ClaudeProviderUpsertInput,
    original_id: Option<String>,
) -> Result<String, String> {
    let backend_port = state.server.backend_port();
    upsert_claude_provider(backend_port, &app_handle, provider, original_id).await
}

#[tauri::command]
pub async fn apply_claude_provider(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    provider_id: String,
) -> Result<String, String> {
    let backend_port = state.server.backend_port();
    apply_claude_provider_impl(backend_port, &app_handle, &provider_id).await
}

#[tauri::command]
pub async fn delete_claude_provider(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    provider_id: String,
) -> Result<String, String> {
    let backend_port = state.server.backend_port();
    delete_claude_provider_impl(backend_port, &app_handle, &provider_id).await
}

#[tauri::command]
pub async fn duplicate_claude_provider(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    provider_id: String,
) -> Result<String, String> {
    let backend_port = state.server.backend_port();
    duplicate_claude_provider_impl(backend_port, &app_handle, &provider_id).await
}

#[tauri::command]
pub async fn set_claude_provider_enabled(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    provider_id: String,
    enabled: bool,
) -> Result<String, String> {
    let backend_port = state.server.backend_port();
    set_claude_provider_enabled_impl(backend_port, &app_handle, &provider_id, enabled).await
}

#[tauri::command]
pub async fn test_claude_provider_connectivity(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    provider_id: String,
) -> Result<String, String> {
    let backend_port = state.server.backend_port();
    test_claude_provider_connectivity_impl(backend_port, &app_handle, &provider_id).await
}

#[tauri::command]
pub fn get_droid_custom_models() -> Result<Vec<DroidCustomModelSummary>, String> {
    list_droid_custom_models()
}

#[tauri::command]
pub fn save_droid_custom_model(
    model_config: DroidCustomModelUpsertInput,
    original_id: Option<String>,
) -> Result<String, String> {
    upsert_droid_custom_model(model_config, original_id)
}

#[tauri::command]
pub fn delete_droid_custom_model(model_id: String) -> Result<String, String> {
    delete_droid_custom_model_impl(&model_id)
}

#[tauri::command]
pub fn duplicate_droid_custom_model(model_id: String) -> Result<String, String> {
    duplicate_droid_custom_model_impl(&model_id)
}

#[tauri::command]
pub fn set_droid_default_model(model_id: String) -> Result<String, String> {
    set_droid_default_model_impl(&model_id)
}

#[tauri::command]
pub fn get_codex_config_profiles(
    app_handle: tauri::AppHandle,
) -> Result<Vec<CodexConfigProfileSummary>, String> {
    list_codex_config_profiles(&app_handle)
}

#[tauri::command]
pub fn save_codex_config_profile(
    app_handle: tauri::AppHandle,
    profile: CodexConfigUpsertInput,
    original_id: Option<String>,
) -> Result<String, String> {
    upsert_codex_config_profile(&app_handle, profile, original_id)
}

#[tauri::command]
pub fn apply_codex_config_profile(
    app_handle: tauri::AppHandle,
    profile_id: String,
) -> Result<String, String> {
    apply_codex_config_profile_impl(&app_handle, &profile_id)
}

#[tauri::command]
pub fn delete_codex_config_profile(
    app_handle: tauri::AppHandle,
    profile_id: String,
) -> Result<String, String> {
    delete_codex_config_profile_impl(&app_handle, &profile_id)
}

#[tauri::command]
pub fn duplicate_codex_config_profile(
    app_handle: tauri::AppHandle,
    profile_id: String,
) -> Result<String, String> {
    duplicate_codex_config_profile_impl(&app_handle, &profile_id)
}
