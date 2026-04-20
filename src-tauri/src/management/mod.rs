use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::time::Duration;

const MANAGEMENT_SECRET: &str = "toapiproxy-local-dev";
const ROUND_ROBIN_STRATEGY: &str = "round-robin";
const FILL_FIRST_STRATEGY: &str = "fill-first";
const MODE_ROUND_ROBIN: &str = "round-robin";
const MODE_PREFERRED: &str = "preferred";
const PREFERRED_PRIORITY: i32 = 1000;
const SERVICE_CATALOG: [(&str, &str); 7] = [
    ("claude", "Claude"),
    ("codex", "Codex"),
    ("gemini", "Gemini"),
    ("copilot", "GitHub Copilot"),
    ("qwen", "Qwen"),
    ("kiro", "Kiro"),
    ("antigravity", "Antigravity"),
];

#[derive(Debug, Clone, Serialize)]
pub struct ServiceRoutingOverview {
    #[serde(rename = "globalStrategy")]
    pub global_strategy: String,
    pub services: Vec<ServiceRoutingState>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceRoutingState {
    pub id: String,
    pub name: String,
    pub connected: bool,
    #[serde(rename = "accountCount")]
    pub account_count: usize,
    pub mode: String,
    #[serde(
        rename = "preferredAccountName",
        skip_serializing_if = "Option::is_none"
    )]
    pub preferred_account_name: Option<String>,
    pub accounts: Vec<ServiceRoutingAccount>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceRoutingAccount {
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    pub priority: i32,
    #[serde(rename = "isPreferred")]
    pub is_preferred: bool,
    pub disabled: bool,
    pub unavailable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AuthFilesResponse {
    #[serde(default)]
    files: Vec<AuthFileEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct CodexKeysResponse {
    #[serde(rename = "codex-api-key", default)]
    codex_api_key: Vec<CodexConfigKeyEntry>,
}

#[derive(Debug, Clone, Serialize)]
struct CodexKeysRequest {
    items: Vec<CodexConfigKeyEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct RoutingStrategyResponse {
    #[serde(default)]
    strategy: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AuthFileEntry {
    #[serde(default)]
    name: String,
    #[serde(rename = "type", default)]
    type_name: String,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    account: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    disabled: bool,
    #[serde(default)]
    unavailable: bool,
    #[serde(rename = "runtime_only", default)]
    runtime_only: bool,
    #[serde(default)]
    priority: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct CodexConfigKeyEntry {
    #[serde(rename = "api-key")]
    api_key: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    priority: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    prefix: String,
    #[serde(rename = "base-url", default, skip_serializing_if = "String::is_empty")]
    base_url: String,
    #[serde(default, skip_serializing_if = "is_false")]
    websockets: bool,
    #[serde(
        rename = "proxy-url",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    proxy_url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    models: Vec<CodexModelEntry>,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    headers: std::collections::HashMap<String, String>,
    #[serde(
        rename = "excluded-models",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    excluded_models: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct CodexModelEntry {
    #[serde(default)]
    name: String,
    #[serde(default)]
    alias: String,
}

pub struct ManagementClient {
    client: Client,
    backend_port: u16,
}

impl ManagementClient {
    pub fn new(backend_port: u16) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(8))
            .build()
            .expect("failed to build management client");

        Self {
            client,
            backend_port,
        }
    }

    pub async fn get_service_routing_overview(&self) -> Result<ServiceRoutingOverview, String> {
        let auth_files = self.list_auth_files().await?;
        let codex_keys = self.list_codex_keys().await?;
        let global_strategy = self.get_routing_strategy().await?;
        let services = SERVICE_CATALOG
            .iter()
            .map(|(service_id, service_name)| {
                build_service_state(service_id, service_name, &auth_files, &codex_keys)
            })
            .collect();

        Ok(ServiceRoutingOverview {
            global_strategy,
            services,
        })
    }

    pub async fn apply_service_account_mode(
        &self,
        service_id: &str,
        mode: &str,
        preferred_account_name: Option<&str>,
    ) -> Result<(), String> {
        let normalized_mode = normalize_mode(mode).ok_or_else(|| "Unsupported mode".to_string())?;

        if service_id == "codex" {
            return self
                .apply_codex_account_mode(normalized_mode, preferred_account_name)
                .await;
        }

        let auth_files = self.list_auth_files().await?;
        let service_auths = auth_files_for_service(service_id, &auth_files);

        if service_auths.is_empty() {
            return Err(format!("No accounts found for service '{}'", service_id));
        }

        self.set_routing_strategy(ROUND_ROBIN_STRATEGY).await?;

        match normalized_mode {
            MODE_ROUND_ROBIN => {
                for auth in service_auths {
                    if auth.priority != 0 {
                        self.set_auth_priority(&auth.name, 0).await?;
                    }
                }
            }
            MODE_PREFERRED => {
                let preferred_name = preferred_account_name
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "Preferred account is required".to_string())?;

                let selected = service_auths
                    .iter()
                    .find(|auth| auth.name == preferred_name)
                    .ok_or_else(|| "Preferred account was not found".to_string())?;

                if selected.disabled {
                    return Err("Preferred account is disabled".to_string());
                }

                for auth in service_auths {
                    let target_priority = if auth.name == preferred_name {
                        PREFERRED_PRIORITY
                    } else {
                        0
                    };

                    if auth.priority != target_priority {
                        self.set_auth_priority(&auth.name, target_priority).await?;
                    }
                }
            }
            _ => return Err("Unsupported mode".to_string()),
        }

        Ok(())
    }

    async fn list_auth_files(&self) -> Result<Vec<AuthFileEntry>, String> {
        let url = self.management_url("/v0/management/auth-files");
        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", MANAGEMENT_SECRET))
            .send()
            .await
            .map_err(|error| format!("Failed to fetch auth files: {}", error))?;

        if !response.status().is_success() {
            return Err(unexpected_response("Fetching auth files", response).await);
        }

        let payload: AuthFilesResponse = response
            .json()
            .await
            .map_err(|error| format!("Failed to parse auth files response: {}", error))?;

        Ok(payload.files)
    }

    async fn get_routing_strategy(&self) -> Result<String, String> {
        let url = self.management_url("/v0/management/routing/strategy");
        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", MANAGEMENT_SECRET))
            .send()
            .await
            .map_err(|error| format!("Failed to fetch routing strategy: {}", error))?;

        if !response.status().is_success() {
            return Err(unexpected_response("Fetching routing strategy", response).await);
        }

        let payload: RoutingStrategyResponse = response
            .json()
            .await
            .map_err(|error| format!("Failed to parse routing strategy response: {}", error))?;

        Ok(normalize_strategy(&payload.strategy).to_string())
    }

    async fn set_routing_strategy(&self, strategy: &str) -> Result<(), String> {
        let url = self.management_url("/v0/management/routing/strategy");
        let response = self
            .client
            .put(url)
            .header("Authorization", format!("Bearer {}", MANAGEMENT_SECRET))
            .json(&json!({ "value": normalize_strategy(strategy) }))
            .send()
            .await
            .map_err(|error| format!("Failed to update routing strategy: {}", error))?;

        if !response.status().is_success() {
            return Err(unexpected_response("Updating routing strategy", response).await);
        }

        Ok(())
    }

    async fn set_auth_priority(&self, name: &str, priority: i32) -> Result<(), String> {
        let url = self.management_url("/v0/management/auth-files/fields");
        let response = self
            .client
            .patch(url)
            .header("Authorization", format!("Bearer {}", MANAGEMENT_SECRET))
            .json(&json!({
                "name": name,
                "priority": priority,
            }))
            .send()
            .await
            .map_err(|error| format!("Failed to update account priority: {}", error))?;

        if !response.status().is_success() {
            return Err(unexpected_response("Updating account priority", response).await);
        }

        Ok(())
    }

    async fn list_codex_keys(&self) -> Result<Vec<CodexConfigKeyEntry>, String> {
        let url = self.management_url("/v0/management/codex-api-key");
        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", MANAGEMENT_SECRET))
            .send()
            .await
            .map_err(|error| format!("Failed to fetch Codex keys: {}", error))?;

        if !response.status().is_success() {
            return Err(unexpected_response("Fetching Codex keys", response).await);
        }

        let payload: CodexKeysResponse = response
            .json()
            .await
            .map_err(|error| format!("Failed to parse Codex keys response: {}", error))?;

        Ok(payload.codex_api_key)
    }

    async fn put_codex_keys(&self, items: Vec<CodexConfigKeyEntry>) -> Result<(), String> {
        let url = self.management_url("/v0/management/codex-api-key");
        let response = self
            .client
            .put(url)
            .header("Authorization", format!("Bearer {}", MANAGEMENT_SECRET))
            .json(&CodexKeysRequest { items })
            .send()
            .await
            .map_err(|error| format!("Failed to update Codex keys: {}", error))?;

        if !response.status().is_success() {
            return Err(unexpected_response("Updating Codex keys", response).await);
        }

        Ok(())
    }

    async fn apply_codex_account_mode(
        &self,
        mode: &str,
        preferred_account_name: Option<&str>,
    ) -> Result<(), String> {
        let auth_files = self.list_auth_files().await?;
        let codex_auths = auth_files_for_service("codex", &auth_files);
        let mut codex_keys = self.list_codex_keys().await?;

        if codex_auths.is_empty() && codex_keys.is_empty() {
            return Err("No accounts found for service 'codex'".to_string());
        }

        self.set_routing_strategy(ROUND_ROBIN_STRATEGY).await?;

        match mode {
            MODE_ROUND_ROBIN => {
                for auth in &codex_auths {
                    if auth.priority != 0 {
                        self.set_auth_priority(&auth.name, 0).await?;
                    }
                }

                let mut changed = false;
                for key in &mut codex_keys {
                    if key.priority != 0 {
                        key.priority = 0;
                        changed = true;
                    }
                }

                if changed {
                    self.put_codex_keys(codex_keys).await?;
                }
            }
            MODE_PREFERRED => {
                let preferred_name = preferred_account_name
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "Preferred account is required".to_string())?;

                let preferred_auth_name = preferred_name.strip_prefix("auth::");
                let preferred_key_index = preferred_name
                    .strip_prefix("codex-key::")
                    .and_then(|value| value.parse::<usize>().ok());

                let auth_names: HashSet<&str> =
                    codex_auths.iter().map(|auth| auth.name.as_str()).collect();
                if let Some(auth_name) = preferred_auth_name {
                    let selected = codex_auths
                        .iter()
                        .find(|auth| auth.name == auth_name)
                        .ok_or_else(|| "Preferred account was not found".to_string())?;
                    if selected.disabled {
                        return Err("Preferred account is disabled".to_string());
                    }
                }

                if let Some(index) = preferred_key_index {
                    if index >= codex_keys.len() {
                        return Err("Preferred account was not found".to_string());
                    }
                }

                if preferred_auth_name.is_none() && preferred_key_index.is_none() {
                    if auth_names.contains(preferred_name) {
                        let selected = codex_auths
                            .iter()
                            .find(|auth| auth.name == preferred_name)
                            .ok_or_else(|| "Preferred account was not found".to_string())?;
                        if selected.disabled {
                            return Err("Preferred account is disabled".to_string());
                        }

                        for auth in &codex_auths {
                            let target_priority = if auth.name == preferred_name {
                                PREFERRED_PRIORITY
                            } else {
                                0
                            };
                            if auth.priority != target_priority {
                                self.set_auth_priority(&auth.name, target_priority).await?;
                            }
                        }

                        let mut changed = false;
                        for key in &mut codex_keys {
                            if key.priority != 0 {
                                key.priority = 0;
                                changed = true;
                            }
                        }
                        if changed {
                            self.put_codex_keys(codex_keys).await?;
                        }
                        return Ok(());
                    }
                    return Err("Preferred account was not found".to_string());
                }

                for auth in &codex_auths {
                    let target_priority = if preferred_auth_name == Some(auth.name.as_str()) {
                        PREFERRED_PRIORITY
                    } else {
                        0
                    };

                    if auth.priority != target_priority {
                        self.set_auth_priority(&auth.name, target_priority).await?;
                    }
                }

                let mut changed = false;
                for (index, key) in codex_keys.iter_mut().enumerate() {
                    let target_priority = if preferred_key_index == Some(index) {
                        PREFERRED_PRIORITY
                    } else {
                        0
                    };

                    if key.priority != target_priority {
                        key.priority = target_priority;
                        changed = true;
                    }
                }

                if changed {
                    self.put_codex_keys(codex_keys).await?;
                }
            }
            _ => return Err("Unsupported mode".to_string()),
        }

        Ok(())
    }

    fn management_url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.backend_port, path)
    }
}

fn build_service_state(
    service_id: &str,
    service_name: &str,
    auth_files: &[AuthFileEntry],
    codex_keys: &[CodexConfigKeyEntry],
) -> ServiceRoutingState {
    if service_id == "codex" {
        return build_codex_service_state(service_name, auth_files, codex_keys);
    }

    let service_auths = auth_files_for_service(service_id, auth_files);
    build_auth_file_service_state(service_id, service_name, service_auths)
}

fn build_auth_file_service_state(
    service_id: &str,
    service_name: &str,
    service_auths: Vec<&AuthFileEntry>,
) -> ServiceRoutingState {
    let mut accounts = build_accounts_from_auth_files(service_auths, false);
    finalize_service_state(service_id, service_name, &mut accounts)
}

fn build_codex_service_state(
    service_name: &str,
    auth_files: &[AuthFileEntry],
    codex_keys: &[CodexConfigKeyEntry],
) -> ServiceRoutingState {
    let codex_auths = auth_files_for_service("codex", auth_files);
    let mut accounts = build_accounts_from_auth_files(codex_auths, true);
    accounts.extend(build_accounts_from_codex_keys(codex_keys));
    finalize_service_state("codex", service_name, &mut accounts)
}

fn build_accounts_from_auth_files(
    auth_files: Vec<&AuthFileEntry>,
    prefix_name: bool,
) -> Vec<ServiceRoutingAccount> {
    auth_files
        .into_iter()
        .map(|auth| ServiceRoutingAccount {
            name: if prefix_name {
                format!("auth::{}", auth.name)
            } else {
                auth.name.clone()
            },
            display_name: account_display_name(auth),
            email: non_empty(&auth.email),
            label: non_empty(&auth.label),
            account: non_empty(&auth.account),
            priority: auth.priority,
            is_preferred: false,
            disabled: auth.disabled,
            unavailable: auth.unavailable,
            status: non_empty(&auth.status),
        })
        .collect()
}

fn build_accounts_from_codex_keys(
    codex_keys: &[CodexConfigKeyEntry],
) -> Vec<ServiceRoutingAccount> {
    codex_keys
        .iter()
        .enumerate()
        .map(|(index, key)| ServiceRoutingAccount {
            name: format!("codex-key::{}", index),
            display_name: codex_key_display_name(index, key),
            email: None,
            label: non_empty(&key.prefix),
            account: non_empty(&key.base_url),
            priority: key.priority,
            is_preferred: false,
            disabled: false,
            unavailable: false,
            status: Some("active".to_string()),
        })
        .collect()
}

fn finalize_service_state(
    service_id: &str,
    service_name: &str,
    accounts: &mut Vec<ServiceRoutingAccount>,
) -> ServiceRoutingState {
    accounts.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.display_name.cmp(&right.display_name))
    });

    let preferred = preferred_account_name(accounts);
    for account in accounts.iter_mut() {
        account.is_preferred = preferred
            .as_ref()
            .map(|preferred_name| preferred_name == &account.name)
            .unwrap_or(false);
    }

    let mode = if preferred.is_some() {
        MODE_PREFERRED
    } else {
        MODE_ROUND_ROBIN
    };

    ServiceRoutingState {
        id: service_id.to_string(),
        name: service_name.to_string(),
        connected: !accounts.is_empty(),
        account_count: accounts.len(),
        mode: mode.to_string(),
        preferred_account_name: preferred,
        accounts: accounts.clone(),
    }
}

fn preferred_account_name(accounts: &[ServiceRoutingAccount]) -> Option<String> {
    accounts
        .iter()
        .filter(|account| !account.disabled)
        .max_by_key(|account| account.priority)
        .and_then(|account| {
            if account.priority > 0 {
                Some(account.name.clone())
            } else {
                None
            }
        })
}

fn auth_files_for_service<'a>(
    service_id: &str,
    auth_files: &'a [AuthFileEntry],
) -> Vec<&'a AuthFileEntry> {
    auth_files
        .iter()
        .filter(|auth| !auth.runtime_only)
        .filter(|auth| matches_service_id(service_id, auth))
        .collect()
}

fn matches_service_id(service_id: &str, auth: &AuthFileEntry) -> bool {
    let aliases = service_aliases(service_id);
    let fields = [
        normalize_key(&auth.provider),
        normalize_key(&auth.type_name),
        normalize_key(&auth.name),
        normalize_key(&auth.label),
        normalize_key(&auth.account),
        normalize_key(&auth.email),
    ];

    fields
        .iter()
        .any(|field| !field.is_empty() && aliases.iter().any(|alias| field.contains(alias)))
}

fn service_aliases(service_id: &str) -> &'static [&'static str] {
    match service_id {
        "claude" => &["claude"],
        "codex" => &["codex"],
        "gemini" => &["gemini"],
        "copilot" => &["copilot", "githubcopilot"],
        "qwen" => &["qwen"],
        "kiro" => &["kiro", "kilo", "codewhisperer"],
        "antigravity" => &["antigravity"],
        _ => &[],
    }
}

fn account_display_name(auth: &AuthFileEntry) -> String {
    non_empty(&auth.label)
        .or_else(|| non_empty(&auth.email))
        .or_else(|| non_empty(&auth.account))
        .unwrap_or_else(|| auth.name.clone())
}

fn codex_key_display_name(index: usize, key: &CodexConfigKeyEntry) -> String {
    let masked = mask_secret(&key.api_key);
    if let Some(prefix) = non_empty(&key.prefix) {
        format!("{} · {}", prefix, masked)
    } else {
        format!("Codex Key {} · {}", index + 1, masked)
    }
}

fn normalize_key(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn normalize_strategy(strategy: &str) -> &'static str {
    match strategy.trim().to_ascii_lowercase().as_str() {
        FILL_FIRST_STRATEGY | "fillfirst" | "ff" => FILL_FIRST_STRATEGY,
        _ => ROUND_ROBIN_STRATEGY,
    }
}

fn normalize_mode(mode: &str) -> Option<&'static str> {
    match mode.trim().to_ascii_lowercase().as_str() {
        MODE_ROUND_ROBIN | "roundrobin" | "rotation" => Some(MODE_ROUND_ROBIN),
        MODE_PREFERRED | "preferredaccount" | "manual" | "manualselect" => Some(MODE_PREFERRED),
        _ => None,
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn mask_secret(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() <= 8 {
        return "****".to_string();
    }

    let suffix_len = trimmed.len().min(4);
    format!("****{}", &trimmed[trimmed.len() - suffix_len..])
}

fn is_zero(value: &i32) -> bool {
    *value == 0
}

fn is_false(value: &bool) -> bool {
    !*value
}

async fn unexpected_response(action: &str, response: Response) -> String {
    let status = response.status();
    let body = response.text().await.unwrap_or_default().trim().to_string();

    if body.is_empty() {
        format!("{} failed: {}", action, status)
    } else {
        format!("{} failed: {} {}", action, status, body)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_service_state, matches_service_id, AuthFileEntry, CodexConfigKeyEntry,
        MODE_PREFERRED, MODE_ROUND_ROBIN,
    };

    fn auth(provider: &str, name: &str, priority: i32) -> AuthFileEntry {
        AuthFileEntry {
            name: name.to_string(),
            type_name: provider.to_string(),
            provider: provider.to_string(),
            label: String::new(),
            email: format!("{}@example.com", name),
            account: String::new(),
            status: "active".to_string(),
            disabled: false,
            unavailable: false,
            runtime_only: false,
            priority,
        }
    }

    #[test]
    fn matches_copilot_aliases() {
        let entry = auth("github-copilot", "copilot-main.json", 0);
        assert!(matches_service_id("copilot", &entry));
    }

    #[test]
    fn round_robin_mode_when_no_positive_priority_exists() {
        let auths = vec![
            auth("codex", "codex-1.json", 0),
            auth("codex", "codex-2.json", 0),
        ];
        let state = build_service_state("codex", "Codex", &auths, &[]);

        assert_eq!(state.mode, MODE_ROUND_ROBIN);
        assert!(state.preferred_account_name.is_none());
    }

    #[test]
    fn preferred_mode_when_one_account_has_higher_priority() {
        let auths = vec![
            auth("codex", "codex-1.json", 0),
            auth("codex", "codex-2.json", 1000),
        ];
        let state = build_service_state("codex", "Codex", &auths, &[]);

        assert_eq!(state.mode, MODE_PREFERRED);
        assert_eq!(
            state.preferred_account_name.as_deref(),
            Some("auth::codex-2.json")
        );
        assert!(state.accounts.iter().any(|account| account.is_preferred));
    }

    #[test]
    fn codex_keys_are_visible_in_service_state() {
        let keys = vec![CodexConfigKeyEntry {
            api_key: "sk-test-12345678".to_string(),
            priority: 1000,
            prefix: String::new(),
            base_url: "https://chatgpt.com/backend-api/codex".to_string(),
            websockets: false,
            proxy_url: String::new(),
            models: Vec::new(),
            headers: std::collections::HashMap::new(),
            excluded_models: Vec::new(),
        }];

        let state = build_service_state("codex", "Codex", &[], &keys);

        assert!(state.connected);
        assert_eq!(state.account_count, 1);
        assert_eq!(state.mode, MODE_PREFERRED);
        assert_eq!(
            state.preferred_account_name.as_deref(),
            Some("codex-key::0")
        );
    }
}
