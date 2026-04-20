use crate::auth::{AuthManager, ServiceType};
use crate::claude_providers::{
    apply_claude_provider as apply_claude_provider_impl,
    delete_claude_provider as delete_claude_provider_impl,
    duplicate_claude_provider as duplicate_claude_provider_impl, list_claude_providers,
    set_claude_provider_enabled as set_claude_provider_enabled_impl,
    test_claude_provider_connectivity as test_claude_provider_connectivity_impl,
    upsert_claude_provider, ClaudeProviderSummary, ClaudeProviderUpsertInput,
};
use crate::codex::{CodexAccountSnapshot, CodexClient, CodexKey};
use crate::droid_models::{
    delete_droid_custom_model as delete_droid_custom_model_impl,
    duplicate_droid_custom_model as duplicate_droid_custom_model_impl,
    list_droid_custom_models, set_droid_default_model as set_droid_default_model_impl,
    upsert_droid_custom_model, DroidCustomModelSummary, DroidCustomModelUpsertInput,
};
use crate::management::{ManagementClient, ServiceRoutingOverview};
use crate::server::{AuthResult, ProxyServer};
use crate::thinking_proxy::ThinkingProxy;
use crate::usage::UsageClient;
use crate::watcher::AuthFileWatcher;
use serde::Serialize;
use std::sync::Mutex;
use tauri::State;

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
    log::info!(
        "Connecting to service: {}, email: {:?}",
        service_id,
        qwen_email
    );
    state
        .server
        .run_auth_command(&service_id, qwen_email.as_deref())
}

#[tauri::command]
pub fn disconnect_service(state: State<AppState>, account_id: String) -> Result<String, String> {
    log::info!("Disconnecting account: {}", account_id);

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
    log::info!("Fetching usage for account: {}", account_id);

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
    log::info!("Deleting Codex API key: {}", api_key);
    let client = CodexClient::new();
    client.delete_codex_key(&api_key).await
}

#[tauri::command]
pub async fn delete_codex_account(
    state: State<'_, AppState>,
    account_ref: String,
) -> Result<(), String> {
    log::info!("Deleting Codex account: {}", account_ref);
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
