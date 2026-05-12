use chrono::{TimeZone, Utc};
use regex::Regex;
use serde::{de::Error as DeError, Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const BACKEND_PORT: u16 = 8318;
const DEFAULT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
#[cfg(test)]
const DEFAULT_CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const CODEX_USAGE_PATH: &str = "/api/codex/usage";
const WHAM_USAGE_PATH: &str = "/wham/usage";
const MANAGEMENT_SECRET: &str = "toapiproxy-local-dev";
const CODEX_USER_AGENT: &str = "codex-cli";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexKey {
    #[serde(rename = "api-key")]
    pub api_key: String,
    #[serde(rename = "base-url", skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(rename = "beta-features", skip_serializing_if = "Option::is_none")]
    pub beta_features: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexAccountSnapshot {
    #[serde(rename = "accountRef")]
    pub account_ref: String,
    #[serde(rename = "routeName")]
    pub route_name: String,
    #[serde(rename = "storageKind")]
    pub storage_kind: String,
    #[serde(rename = "apiKey")]
    pub api_key: String,
    #[serde(rename = "maskedApiKey")]
    pub masked_api_key: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    #[serde(rename = "accountId", skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(rename = "creditsBalance", skip_serializing_if = "Option::is_none")]
    pub credits_balance: Option<f64>,
    #[serde(rename = "creditsUnlimited")]
    pub credits_unlimited: bool,
    #[serde(rename = "baseUrl", skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(rename = "betaFeatures", skip_serializing_if = "Option::is_none")]
    pub beta_features: Option<bool>,
    #[serde(rename = "primaryWindow", skip_serializing_if = "Option::is_none")]
    pub primary_window: Option<CodexRateWindow>,
    #[serde(rename = "secondaryWindow", skip_serializing_if = "Option::is_none")]
    pub secondary_window: Option<CodexRateWindow>,
    #[serde(rename = "updatedAt", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(rename = "usageError", skip_serializing_if = "Option::is_none")]
    pub usage_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexRateWindow {
    #[serde(rename = "usedPercent")]
    pub used_percent: i32,
    #[serde(rename = "remainingPercent")]
    pub remaining_percent: i32,
    #[serde(rename = "resetAt", skip_serializing_if = "Option::is_none")]
    pub reset_at: Option<String>,
    #[serde(rename = "resetInDays", skip_serializing_if = "Option::is_none")]
    pub reset_in_days: Option<i64>,
    #[serde(rename = "windowSeconds")]
    pub window_seconds: i32,
    #[serde(rename = "windowLabel")]
    pub window_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexKeyListResponse {
    #[serde(rename = "codex-api-key", default)]
    pub codex_api_key: Option<Vec<CodexKey>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexKeyListRequest {
    #[serde(rename = "items")]
    pub items: Vec<CodexKey>,
}

#[derive(Debug, Deserialize)]
struct CodexAuthJson {
    #[serde(rename = "OPENAI_API_KEY", default)]
    openai_api_key: Option<String>,
    #[serde(rename = "tokens", default)]
    tokens: Option<CodexTokens>,
    #[serde(rename = "last_refresh", default)]
    last_refresh: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(rename = "type", default)]
    type_name: Option<String>,
    #[serde(rename = "expired", default)]
    expired: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct CodexTokens {
    #[serde(rename = "access_token")]
    access_token: Option<String>,
    #[serde(rename = "refresh_token", default)]
    refresh_token: Option<String>,
    #[serde(rename = "id_token", default)]
    id_token: Option<String>,
    #[serde(rename = "account_id", default)]
    account_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ManagedCodexAuthFile {
    #[serde(rename = "id_token", default)]
    id_token: Option<String>,
    #[serde(rename = "access_token", default)]
    access_token: Option<String>,
    #[serde(rename = "refresh_token", default)]
    refresh_token: Option<String>,
    account_id: Option<String>,
    #[serde(rename = "last_refresh", default)]
    last_refresh: Option<String>,
    email: Option<String>,
    #[serde(rename = "type", default)]
    type_name: Option<String>,
    #[serde(rename = "expired", default)]
    expired: Option<String>,
}

#[derive(Debug, Clone)]
struct ManagedCodexAccount {
    file_name: String,
    storage: ManagedCodexAuthFile,
}

#[derive(Debug, Clone, Default)]
struct TokenClaims {
    email: Option<String>,
    plan: Option<String>,
    account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CodexUsageResponse {
    #[serde(rename = "plan_type", default)]
    plan_type: Option<String>,
    #[serde(rename = "rate_limit", default)]
    rate_limit: Option<CodexRateLimitDetails>,
    #[serde(default)]
    credits: Option<CodexCreditDetails>,
}

#[derive(Debug, Deserialize)]
struct CodexRateLimitDetails {
    #[serde(rename = "primary_window", default)]
    primary_window: Option<CodexUsageWindow>,
    #[serde(rename = "secondary_window", default)]
    secondary_window: Option<CodexUsageWindow>,
}

#[derive(Debug, Deserialize)]
struct CodexUsageWindow {
    #[serde(rename = "used_percent", deserialize_with = "deserialize_i32_from_any")]
    used_percent: i32,
    #[serde(
        rename = "reset_at",
        default,
        deserialize_with = "deserialize_optional_i64_from_any"
    )]
    reset_at: Option<i64>,
    #[serde(
        rename = "limit_window_seconds",
        deserialize_with = "deserialize_i32_from_any"
    )]
    limit_window_seconds: i32,
}

#[derive(Debug, Deserialize)]
struct CodexCreditDetails {
    #[serde(rename = "has_credits", default)]
    has_credits: bool,
    #[serde(default)]
    unlimited: bool,
    #[serde(default, deserialize_with = "deserialize_optional_f64_from_any")]
    balance: Option<f64>,
}

pub struct CodexClient {
    client: reqwest::Client,
    config_path: Option<PathBuf>,
}

impl CodexClient {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .build()
            .expect("failed to build reqwest client");

        Self {
            client,
            config_path: None,
        }
    }

    pub fn with_config_path(config_path: Option<PathBuf>) -> Self {
        let mut client = Self::new();
        client.config_path = config_path;
        client
    }

    pub async fn get_codex_keys(&self) -> Result<Vec<CodexKey>, String> {
        let url = format!(
            "http://127.0.0.1:{}/v0/management/codex-api-key",
            BACKEND_PORT
        );

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", MANAGEMENT_SECRET))
            .send()
            .await
            .map_err(|e| format!("Failed to connect to backend: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("API returned error: {}", response.status()));
        }

        let data: CodexKeyListResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(data.codex_api_key.unwrap_or_default())
    }

    pub async fn get_codex_accounts(&self) -> Result<Vec<CodexAccountSnapshot>, String> {
        let managed_accounts = self.load_managed_codex_accounts()?;
        if !managed_accounts.is_empty() {
            let mut snapshots = Vec::with_capacity(managed_accounts.len());
            for (index, account) in managed_accounts.iter().enumerate() {
                snapshots.push(
                    self.build_managed_codex_account_snapshot(account, index)
                        .await,
                );
            }
            return Ok(snapshots);
        }

        let keys = self.get_codex_keys().await?;
        let mut accounts = Vec::with_capacity(keys.len());
        for (index, key) in keys.into_iter().enumerate() {
            accounts.push(self.build_legacy_codex_account_snapshot(key, index).await);
        }
        Ok(accounts)
    }

    async fn build_managed_codex_account_snapshot(
        &self,
        account: &ManagedCodexAccount,
        index: usize,
    ) -> CodexAccountSnapshot {
        let access_token =
            normalize_optional_string(account.storage.access_token.as_deref()).unwrap_or_default();
        let id_token = normalize_optional_string(account.storage.id_token.as_deref());
        let access_claims = extract_token_claims(&access_token);
        let id_claims = id_token
            .as_deref()
            .map(extract_token_claims)
            .unwrap_or_default();

        let email = normalize_optional_string(account.storage.email.as_deref())
            .or_else(|| access_claims.email.clone())
            .or_else(|| id_claims.email.clone());
        let mut plan = access_claims
            .plan
            .clone()
            .or_else(|| id_claims.plan.clone());
        let account_id = normalize_optional_string(account.storage.account_id.as_deref())
            .or_else(|| access_claims.account_id.clone())
            .or_else(|| id_claims.account_id.clone());
        let mut updated_at = normalize_optional_string(account.storage.last_refresh.as_deref());
        let mut credits_balance = None;
        let mut credits_unlimited = false;
        let mut primary_window = None;
        let mut secondary_window = None;
        let mut usage_error = None;

        if access_token.is_empty() {
            usage_error = Some("Codex auth file is missing an access token".to_string());
        } else {
            match self
                .fetch_codex_usage(&access_token, account_id.as_deref(), None)
                .await
            {
                Ok(usage) => {
                    if let Some(plan_type) = normalize_optional_string(usage.plan_type.as_deref()) {
                        plan = Some(plan_type);
                    }

                    if let Some(credits) = usage.credits {
                        credits_unlimited = credits.unlimited;
                        if credits.has_credits || credits.unlimited {
                            credits_balance = credits.balance;
                        }
                    }

                    if let Some(rate_limit) = usage.rate_limit {
                        primary_window =
                            build_codex_usage_window(rate_limit.primary_window.as_ref());
                        secondary_window =
                            build_codex_usage_window(rate_limit.secondary_window.as_ref());
                    }

                    updated_at = Some(Utc::now().to_rfc3339());
                }
                Err(error) => {
                    usage_error = Some(error);
                }
            }
        }

        let display_name = email
            .clone()
            .or_else(|| account_id.clone())
            .unwrap_or_else(|| format!("Codex Account {}", index + 1));

        CodexAccountSnapshot {
            account_ref: format!("auth::{}", account.file_name),
            route_name: format!("auth::{}", account.file_name),
            storage_kind: "auth-file".to_string(),
            api_key: access_token.clone(),
            masked_api_key: mask_api_key(&access_token),
            display_name,
            email,
            plan,
            account_id,
            credits_balance,
            credits_unlimited,
            base_url: None,
            beta_features: None,
            primary_window,
            secondary_window,
            updated_at,
            usage_error,
        }
    }

    async fn build_legacy_codex_account_snapshot(
        &self,
        key: CodexKey,
        index: usize,
    ) -> CodexAccountSnapshot {
        let key_claims = extract_token_claims(&key.api_key);
        let email = key_claims.email.clone();
        let mut plan = key_claims.plan.clone();
        let account_id = key_claims.account_id.clone();
        let mut updated_at = None;
        let mut credits_balance = None;
        let mut credits_unlimited = false;
        let mut primary_window = None;
        let mut secondary_window = None;
        let mut usage_error = None;

        match self
            .fetch_codex_usage(&key.api_key, account_id.as_deref(), key.base_url.as_deref())
            .await
        {
            Ok(usage) => {
                if let Some(plan_type) = normalize_optional_string(usage.plan_type.as_deref()) {
                    plan = Some(plan_type);
                }

                if let Some(credits) = usage.credits {
                    credits_unlimited = credits.unlimited;
                    if credits.has_credits || credits.unlimited {
                        credits_balance = credits.balance;
                    }
                }

                if let Some(rate_limit) = usage.rate_limit {
                    primary_window = build_codex_usage_window(rate_limit.primary_window.as_ref());
                    secondary_window =
                        build_codex_usage_window(rate_limit.secondary_window.as_ref());
                }

                updated_at = Some(Utc::now().to_rfc3339());
            }
            Err(error) => {
                usage_error = Some(error);
            }
        }

        let display_name = email
            .clone()
            .or_else(|| account_id.clone())
            .unwrap_or_else(|| format!("Codex Key {}", index + 1));

        CodexAccountSnapshot {
            account_ref: format!("codex-key::{}", index),
            route_name: format!("codex-key::{}", index),
            storage_kind: "config-key".to_string(),
            api_key: key.api_key.clone(),
            masked_api_key: mask_api_key(&key.api_key),
            display_name,
            email,
            plan,
            account_id,
            credits_balance,
            credits_unlimited,
            base_url: key.base_url.clone(),
            beta_features: key.beta_features,
            primary_window,
            secondary_window,
            updated_at,
            usage_error,
        }
    }

    async fn fetch_codex_usage(
        &self,
        api_key: &str,
        account_id: Option<&str>,
        base_url: Option<&str>,
    ) -> Result<CodexUsageResponse, String> {
        let url = build_codex_usage_url(base_url);
        let mut request = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("User-Agent", CODEX_USER_AGENT)
            .header("Accept", "application/json");

        if let Some(account_id) = normalize_optional_string(account_id) {
            request = request.header("ChatGPT-Account-Id", account_id);
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("Failed to fetch Codex usage: {}", e))?;

        if response.status().is_success() {
            return response
                .json()
                .await
                .map_err(|e| format!("Failed to parse Codex usage response: {}", e));
        }

        Err(format!(
            "Codex usage API returned {}",
            response.status().as_u16()
        ))
    }

    pub async fn add_codex_key(&self, api_key: &str, base_url: Option<&str>) -> Result<(), String> {
        let url = format!(
            "http://127.0.0.1:{}/v0/management/codex-api-key",
            BACKEND_PORT
        );
        let resolved_base_url = normalize_codex_base_url(base_url);
        let mut items = self.get_codex_keys().await?;
        if items.iter().any(|item| item.api_key == api_key) {
            return Ok(());
        }
        items.push(CodexKey {
            api_key: api_key.to_string(),
            base_url: Some(resolved_base_url),
            beta_features: Some(true),
        });

        let request = CodexKeyListRequest { items };

        let response = self
            .client
            .put(&url)
            .header("Authorization", format!("Bearer {}", MANAGEMENT_SECRET))
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to backend: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("API returned error: {}", response.status()));
        }

        Ok(())
    }

    pub async fn delete_codex_key(&self, api_key: &str) -> Result<(), String> {
        let response = self
            .client
            .delete(format!(
                "http://127.0.0.1:{}/v0/management/codex-api-key",
                BACKEND_PORT
            ))
            .header("Authorization", format!("Bearer {}", MANAGEMENT_SECRET))
            .query(&[("api-key", api_key)])
            .send()
            .await
            .map_err(|e| format!("Failed to connect to backend: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("API returned error: {}", response.status()));
        }

        Ok(())
    }

    pub async fn delete_codex_account(&self, account_ref: &str) -> Result<(), String> {
        if let Some(file_name) = account_ref.strip_prefix("auth::") {
            let response = self
                .client
                .delete(format!(
                    "http://127.0.0.1:{}/v0/management/auth-files",
                    BACKEND_PORT
                ))
                .header("Authorization", format!("Bearer {}", MANAGEMENT_SECRET))
                .query(&[("name", file_name)])
                .send()
                .await
                .map_err(|e| format!("Failed to connect to backend: {}", e))?;

            if !response.status().is_success() {
                return Err(format!("API returned error: {}", response.status()));
            }

            return Ok(());
        }

        if let Some(index_text) = account_ref.strip_prefix("codex-key::") {
            let index = index_text
                .parse::<usize>()
                .map_err(|_| "Invalid Codex key reference".to_string())?;
            let keys = self.get_codex_keys().await?;
            let key = keys
                .get(index)
                .ok_or_else(|| "Codex key was not found".to_string())?;
            return self.delete_codex_key(&key.api_key).await;
        }

        Err("Unsupported Codex account reference".to_string())
    }

    pub async fn check_backend(&self) -> Result<(), String> {
        let url = format!(
            "http://127.0.0.1:{}/v0/management/codex-api-key",
            BACKEND_PORT
        );

        match self.client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success()
                    || response.status().as_u16() == 401
                    || response.status().as_u16() == 403
                {
                    Ok(())
                } else {
                    Err(format!("Backend returned error: {}", response.status()))
                }
            }
            Err(_) => Err(format!(
                "Unable to connect to CLIProxyAPI backend on port {}.",
                BACKEND_PORT
            )),
        }
    }

    pub async fn import_codex_token(&self) -> Result<String, String> {
        self.check_backend_connection().await?;

        let auth_path = find_codex_auth_file()?;
        let content = fs::read_to_string(&auth_path)
            .map_err(|e| format!("Failed to read Codex auth file: {}", e))?;

        let auth_json: CodexAuthJson = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse Codex auth file: {}", e))?;
        let imported = ImportedCodexAccount::from_external_auth(auth_json)?;
        let auth_dir = self.resolve_backend_auth_dir()?;
        fs::create_dir_all(&auth_dir)
            .map_err(|e| format!("Failed to create Codex auth directory: {}", e))?;

        let file_name = build_codex_credential_filename(
            imported.email.as_deref(),
            imported.plan.as_deref(),
            imported.account_id.as_deref(),
            imported.storage.access_token.as_deref().unwrap_or_default(),
        );
        let target_path = auth_dir.join(&file_name);
        let payload = serde_json::to_string_pretty(&imported.storage)
            .map_err(|e| format!("Failed to serialize Codex auth file: {}", e))?;

        fs::write(&target_path, payload)
            .map_err(|e| format!("Failed to write Codex auth file: {}", e))?;

        self.cleanup_legacy_codex_keys(&imported.cleanup_candidates)
            .await?;

        let summary = imported
            .email
            .clone()
            .or(imported.account_id.clone())
            .unwrap_or(file_name);

        Ok(format!("Codex account imported successfully: {}", summary))
    }

    async fn cleanup_legacy_codex_keys(
        &self,
        cleanup_candidates: &HashSet<String>,
    ) -> Result<(), String> {
        if cleanup_candidates.is_empty() {
            return Ok(());
        }

        let keys = self.get_codex_keys().await?;
        for key in keys {
            if cleanup_candidates.contains(key.api_key.trim()) {
                self.delete_codex_key(&key.api_key).await?;
            }
        }

        Ok(())
    }

    fn resolve_backend_auth_dir(&self) -> Result<PathBuf, String> {
        if let Some(config_path) = self.config_path.as_deref() {
            if let Some(path) = parse_auth_dir_from_config(config_path)? {
                return Ok(path);
            }
        }

        default_backend_auth_dir()
    }

    fn load_managed_codex_accounts(&self) -> Result<Vec<ManagedCodexAccount>, String> {
        let auth_dir = self.resolve_backend_auth_dir()?;
        if !auth_dir.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(&auth_dir)
            .map_err(|e| format!("Failed to read Codex auth directory: {}", e))?;
        let mut accounts = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if !is_json_file(&path) {
                continue;
            }

            let Ok(content) = fs::read_to_string(&path) else {
                log::warn!("Failed to read auth file: {:?}", path);
                continue;
            };

            let Ok(storage) = serde_json::from_str::<ManagedCodexAuthFile>(&content) else {
                continue;
            };

            let type_name = normalize_optional_string(storage.type_name.as_deref())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if type_name != "codex" {
                continue;
            }

            let file_name = path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_default();

            accounts.push(ManagedCodexAccount { file_name, storage });
        }

        accounts.sort_by(|left, right| left.file_name.cmp(&right.file_name));
        Ok(accounts)
    }

    async fn check_backend_connection(&self) -> Result<(), String> {
        let url = format!("http://127.0.0.1:{}/v0/management/config", BACKEND_PORT);
        let result = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", MANAGEMENT_SECRET))
            .timeout(Duration::from_secs(2))
            .send()
            .await;

        match result {
            Ok(response) => {
                if response.status().is_success() || response.status().as_u16() == 401 {
                    Ok(())
                } else {
                    Err(format!("Backend returned error: {}", response.status()))
                }
            }
            Err(_) => Err(format!(
                "Unable to connect to CLIProxyAPI backend on port {}.",
                BACKEND_PORT
            )),
        }
    }
}

impl Default for CodexClient {
    fn default() -> Self {
        Self::new()
    }
}

struct ImportedCodexAccount {
    storage: ManagedCodexAuthFile,
    email: Option<String>,
    plan: Option<String>,
    account_id: Option<String>,
    cleanup_candidates: HashSet<String>,
}

impl ImportedCodexAccount {
    fn from_external_auth(auth_json: CodexAuthJson) -> Result<Self, String> {
        let tokens_ref = auth_json.tokens.as_ref();
        let openai_api_key = normalize_optional_string(auth_json.openai_api_key.as_deref());
        let access_token = openai_api_key.clone().or_else(|| {
            normalize_optional_string(tokens_ref.and_then(|t| t.access_token.as_deref()))
        });
        let access_token =
            access_token.ok_or_else(|| "No access token found in Codex auth file".to_string())?;
        let id_token = normalize_optional_string(tokens_ref.and_then(|t| t.id_token.as_deref()));
        let refresh_token =
            normalize_optional_string(tokens_ref.and_then(|t| t.refresh_token.as_deref()));
        let access_claims = extract_token_claims(&access_token);
        let id_claims = id_token
            .as_deref()
            .map(extract_token_claims)
            .unwrap_or_default();
        let account_id = normalize_optional_string(
            tokens_ref
                .and_then(|t| t.account_id.as_deref())
                .or(access_claims.account_id.as_deref())
                .or(id_claims.account_id.as_deref()),
        );
        let email = normalize_optional_string(auth_json.email.as_deref())
            .or_else(|| access_claims.email.clone())
            .or_else(|| id_claims.email.clone());
        let plan = access_claims
            .plan
            .clone()
            .or_else(|| id_claims.plan.clone());
        let mut cleanup_candidates = HashSet::new();
        cleanup_candidates.insert(access_token.clone());
        if let Some(openai_api_key) = openai_api_key {
            cleanup_candidates.insert(openai_api_key);
        }

        Ok(Self {
            storage: ManagedCodexAuthFile {
                id_token,
                access_token: Some(access_token.clone()),
                refresh_token,
                account_id: account_id.clone(),
                last_refresh: normalize_optional_string(auth_json.last_refresh.as_deref())
                    .or_else(|| Some(Utc::now().to_rfc3339())),
                email: email.clone(),
                type_name: normalize_optional_string(auth_json.type_name.as_deref())
                    .or_else(|| Some("codex".to_string())),
                expired: normalize_optional_string(auth_json.expired.as_deref()),
            },
            email,
            plan,
            account_id,
            cleanup_candidates,
        })
    }
}

fn parse_auth_dir_from_config(config_path: &Path) -> Result<Option<PathBuf>, String> {
    let content = fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read config file: {}", e))?;
    let pattern = Regex::new(r#"(?m)^\s*auth-dir:\s*(?:"([^"]*)"|'([^']*)'|([^\r\n#]+))"#).unwrap();

    let Some(captures) = pattern.captures(&content) else {
        return Ok(None);
    };

    let raw = captures
        .get(1)
        .map(|value| unescape_double_quoted_yaml_scalar(value.as_str()))
        .or_else(|| {
            captures
                .get(2)
                .map(|value| value.as_str().replace("''", "'"))
        })
        .or_else(|| {
            captures
                .get(3)
                .map(|value| value.as_str().trim().to_string())
        })
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    Ok(raw
        .as_deref()
        .map(|value| resolve_config_path_value(value, config_path)))
}

fn default_backend_auth_dir() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|home| home.join(".cli-proxy-api"))
        .ok_or_else(|| "Unable to resolve the default Codex auth directory".to_string())
}

fn resolve_config_path_value(raw_value: &str, config_path: &Path) -> PathBuf {
    let trimmed = raw_value.trim();
    if let Some(stripped) = trimmed.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }

    let candidate = PathBuf::from(trimmed);
    if candidate.is_absolute() || is_windows_absolute_path(trimmed) {
        return candidate;
    }

    config_path
        .parent()
        .map(|parent| parent.join(&candidate))
        .unwrap_or(candidate)
}

fn unescape_double_quoted_yaml_scalar(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }

        match chars.next() {
            Some('\\') => output.push('\\'),
            Some('"') => output.push('"'),
            Some('n') => output.push('\n'),
            Some('r') => output.push('\r'),
            Some('t') => output.push('\t'),
            Some(next) => {
                output.push('\\');
                output.push(next);
            }
            None => output.push('\\'),
        }
    }

    output
}

fn is_windows_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    let has_drive_root = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');

    has_drive_root || value.starts_with(r"\\")
}

fn is_json_file(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .map(|value| value.to_string_lossy().eq_ignore_ascii_case("json"))
            .unwrap_or(false)
}

fn build_codex_credential_filename(
    email: Option<&str>,
    plan_type: Option<&str>,
    account_id: Option<&str>,
    access_token: &str,
) -> String {
    let email = sanitize_filename_component(
        normalize_optional_string(email)
            .unwrap_or_else(|| format!("account-{}", api_key_tail(access_token))),
    );
    let plan = normalize_plan_type_for_filename(plan_type.unwrap_or_default());

    if plan.as_deref() == Some("team") {
        if let Some(account_id) = normalize_optional_string(account_id) {
            return format!("codex-{}-{}-team.json", short_sha256(&account_id), email);
        }
    }

    if let Some(plan) = plan {
        return format!("codex-{}-{}.json", email, sanitize_filename_component(plan));
    }

    format!("codex-{}.json", email)
}

fn normalize_plan_type_for_filename(plan_type: &str) -> Option<String> {
    let mut parts = Vec::new();
    let mut current = String::new();

    for ch in plan_type.trim().chars() {
        if ch.is_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            parts.push(current);
            current = String::new();
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("-"))
    }
}

fn sanitize_filename_component(value: String) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            _ => ch,
        })
        .collect()
}

fn short_sha256(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest
        .iter()
        .take(4)
        .map(|byte| format!("{:02x}", byte))
        .collect()
}

fn api_key_tail(api_key: &str) -> String {
    let tail: String = api_key
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    if tail.is_empty() {
        "unknown".to_string()
    } else {
        tail
    }
}

fn extract_token_claims(token: &str) -> TokenClaims {
    let Some(payload) = parse_jwt_payload(token) else {
        return TokenClaims::default();
    };

    let auth = payload.get("https://api.openai.com/auth");
    let profile = payload.get("https://api.openai.com/profile");

    TokenClaims {
        email: normalize_optional_string(
            payload.get("email").and_then(Value::as_str).or(profile
                .and_then(|value| value.get("email"))
                .and_then(Value::as_str)),
        ),
        plan: normalize_optional_string(
            auth.and_then(|value| value.get("chatgpt_plan_type"))
                .and_then(Value::as_str)
                .or(payload.get("chatgpt_plan_type").and_then(Value::as_str)),
        ),
        account_id: normalize_optional_string(
            auth.and_then(|value| value.get("chatgpt_account_id"))
                .and_then(Value::as_str)
                .or(payload.get("account_id").and_then(Value::as_str)),
        ),
    }
}

fn parse_jwt_payload(token: &str) -> Option<Value> {
    let mut segments = token.split('.');
    let _header = segments.next()?;
    let payload = segments.next()?;
    let decoded = decode_base64_url_segment(payload)?;
    serde_json::from_slice(&decoded).ok()
}

fn decode_base64_url_segment(segment: &str) -> Option<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u8;

    for ch in segment.chars() {
        let value = match ch {
            'A'..='Z' => ch as u32 - 'A' as u32,
            'a'..='z' => 26 + (ch as u32 - 'a' as u32),
            '0'..='9' => 52 + (ch as u32 - '0' as u32),
            '-' => 62,
            '_' => 63,
            '=' => continue,
            _ => return None,
        };

        buffer = (buffer << 6) | value;
        bits += 6;

        while bits >= 8 {
            bits -= 8;
            let byte = ((buffer >> bits) & 0xff) as u8;
            output.push(byte);
            if bits > 0 {
                buffer &= (1 << bits) - 1;
            } else {
                buffer = 0;
            }
        }
    }

    Some(output)
}

fn build_codex_usage_url(base_url: Option<&str>) -> String {
    let normalized = normalize_codex_base_url(base_url);
    let trimmed = normalized.trim_end_matches('/');

    if let Some(index) = trimmed.find("/backend-api") {
        let backend_api_root = &trimmed[..index + "/backend-api".len()];
        return format!("{}{}", backend_api_root, WHAM_USAGE_PATH);
    }

    let root = trimmed.trim_end_matches("/codex");
    format!("{}{}", root, CODEX_USAGE_PATH)
}

fn build_codex_usage_window(window: Option<&CodexUsageWindow>) -> Option<CodexRateWindow> {
    let window = window?;
    let used_percent = clamp_percent(window.used_percent);
    let remaining_percent = 100 - used_percent;

    Some(CodexRateWindow {
        used_percent,
        remaining_percent,
        reset_at: window.reset_at.and_then(timestamp_to_rfc3339),
        reset_in_days: window.reset_at.and_then(days_until_reset),
        window_seconds: window.limit_window_seconds,
        window_label: format_window_label(window.limit_window_seconds),
    })
}

fn timestamp_to_rfc3339(timestamp: i64) -> Option<String> {
    Utc.timestamp_opt(timestamp, 0)
        .single()
        .map(|datetime| datetime.to_rfc3339())
}

fn days_until_reset(timestamp: i64) -> Option<i64> {
    let now = Utc::now().timestamp();
    if timestamp <= now {
        return Some(0);
    }

    let remaining_seconds = timestamp - now;
    Some((remaining_seconds + 86_399) / 86_400)
}

fn clamp_percent(value: i32) -> i32 {
    value.clamp(0, 100)
}

fn format_window_label(window_seconds: i32) -> String {
    match window_seconds {
        18_000 => "5h".to_string(),
        604_800 => "Weekly".to_string(),
        value if value % 86_400 == 0 && value > 0 => format!("{}d", value / 86_400),
        value if value % 3_600 == 0 && value > 0 => format!("{}h", value / 3_600),
        value if value > 0 => format!("{}m", value / 60),
        _ => "Unknown".to_string(),
    }
}

fn mask_api_key(api_key: &str) -> String {
    let tail: String = api_key
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    if tail.is_empty() {
        "N/A".to_string()
    } else {
        format!("****{}", tail)
    }
}

fn normalize_codex_base_url(base_url: Option<&str>) -> String {
    match base_url.map(str::trim) {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => DEFAULT_CODEX_BASE_URL.to_string(),
    }
}

fn normalize_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_i32_from_json_value(value: &Value) -> Option<i32> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .or_else(|| number.as_f64().map(|value| value.round() as i32)),
        Value::String(text) => text
            .trim()
            .parse::<f64>()
            .ok()
            .map(|value| value.round() as i32),
        _ => None,
    }
}

fn parse_i64_from_json_value(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value.round() as i64)),
        Value::String(text) => text
            .trim()
            .parse::<f64>()
            .ok()
            .map(|value| value.round() as i64),
        _ => None,
    }
}

fn parse_f64_from_json_value(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn deserialize_i32_from_any<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    parse_i32_from_json_value(&value)
        .ok_or_else(|| D::Error::custom("expected integer-compatible value"))
}

fn deserialize_optional_i64_from_any<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    if value.is_null() {
        return Ok(None);
    }

    parse_i64_from_json_value(&value)
        .map(Some)
        .ok_or_else(|| D::Error::custom("expected integer-compatible value"))
}

fn deserialize_optional_f64_from_any<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    if value.is_null() {
        return Ok(None);
    }

    parse_f64_from_json_value(&value)
        .map(Some)
        .ok_or_else(|| D::Error::custom("expected number-compatible value"))
}

fn find_codex_auth_file() -> Result<PathBuf, String> {
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        let path = PathBuf::from(&codex_home).join("auth.json");
        if path.exists() {
            return Ok(path);
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let path = PathBuf::from(&home).join(".codex").join("auth.json");
        if path.exists() {
            return Ok(path);
        }
    }

    if let Some(home) = dirs::home_dir() {
        let path = home.join(".codex").join("auth.json");
        if path.exists() {
            return Ok(path);
        }
    }

    Err("Codex auth.json was not found. Please sign in with Codex CLI first.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalize_codex_base_url_uses_default_for_missing_values() {
        assert_eq!(
            normalize_codex_base_url(None),
            DEFAULT_CODEX_BASE_URL.to_string()
        );
        assert_eq!(
            normalize_codex_base_url(Some("   ")),
            DEFAULT_CODEX_BASE_URL.to_string()
        );
        assert_eq!(
            normalize_codex_base_url(Some("https://example.com/codex")),
            "https://example.com/codex"
        );
    }

    #[test]
    fn build_codex_usage_url_prefers_wham_endpoint_for_backend_api_paths() {
        assert_eq!(
            build_codex_usage_url(Some("https://chatgpt.com/backend-api/codex")),
            DEFAULT_CODEX_USAGE_URL
        );
        assert_eq!(
            build_codex_usage_url(Some("https://chatgpt.com/backend-api")),
            DEFAULT_CODEX_USAGE_URL
        );
        assert_eq!(
            build_codex_usage_url(Some("https://gateway.example.com/codex")),
            "https://gateway.example.com/api/codex/usage"
        );
    }

    #[test]
    fn codex_key_list_response_accepts_null_or_missing_list() {
        let null_payload =
            serde_json::from_str::<CodexKeyListResponse>(r#"{"codex-api-key":null}"#).unwrap();
        let missing_payload = serde_json::from_str::<CodexKeyListResponse>(r#"{}"#).unwrap();

        assert!(null_payload.codex_api_key.unwrap_or_default().is_empty());
        assert!(missing_payload.codex_api_key.unwrap_or_default().is_empty());
    }

    #[test]
    fn codex_usage_response_accepts_string_numeric_fields() {
        let payload = r#"
        {
          "plan_type": "plus",
          "rate_limit": {
            "primary_window": {
              "used_percent": "38",
              "reset_at": "1776544740",
              "limit_window_seconds": "18000"
            }
          },
          "credits": {
            "has_credits": true,
            "unlimited": false,
            "balance": "0.00"
          }
        }
        "#;

        let response: CodexUsageResponse = serde_json::from_str(payload).unwrap();
        let primary = response.rate_limit.unwrap().primary_window.unwrap();

        assert_eq!(response.plan_type.as_deref(), Some("plus"));
        assert_eq!(primary.used_percent, 38);
        assert_eq!(primary.limit_window_seconds, 18_000);
        assert_eq!(response.credits.unwrap().balance, Some(0.0));
    }

    #[test]
    fn extract_token_claims_reads_openai_claims() {
        let payload = json!({
            "email": "owner@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "account-123",
                "chatgpt_plan_type": "plus"
            }
        });
        let token = format!(
            "header.{}.signature",
            encode_base64_url(payload.to_string().as_bytes())
        );
        let claims = extract_token_claims(&token);

        assert_eq!(claims.email.as_deref(), Some("owner@example.com"));
        assert_eq!(claims.account_id.as_deref(), Some("account-123"));
        assert_eq!(claims.plan.as_deref(), Some("plus"));
    }

    #[test]
    fn build_codex_usage_window_calculates_remaining_percent() {
        let window = CodexUsageWindow {
            used_percent: 38,
            reset_at: Some(Utc::now().timestamp() + 2 * 86_400),
            limit_window_seconds: 18_000,
        };

        let snapshot = build_codex_usage_window(Some(&window)).unwrap();

        assert_eq!(snapshot.used_percent, 38);
        assert_eq!(snapshot.remaining_percent, 62);
        assert_eq!(snapshot.window_label, "5h");
        assert!(snapshot.reset_in_days.unwrap() >= 1);
    }

    #[test]
    fn normalize_plan_type_for_filename_matches_expected_shape() {
        assert_eq!(
            normalize_plan_type_for_filename("Team Plan"),
            Some("team-plan".to_string())
        );
        assert_eq!(normalize_plan_type_for_filename("  "), None);
    }

    #[test]
    fn build_codex_credential_filename_uses_team_account_hash() {
        let file_name = build_codex_credential_filename(
            Some("owner@example.com"),
            Some("team"),
            Some("account-123"),
            "token-tail",
        );

        assert_eq!(file_name, "codex-725a2fd1-owner@example.com-team.json");
    }

    #[test]
    fn parse_auth_dir_from_config_reads_quoted_values() {
        let temp_dir = std::env::temp_dir().join(format!("codex-auth-dir-{}", std::process::id()));
        let config_path = temp_dir.join("config.yaml");
        fs::create_dir_all(&temp_dir).unwrap();
        fs::write(
            &config_path,
            "port: 8318\nauth-dir: \"C:\\\\Users\\\\demo\\\\.cli-proxy-api\"\n",
        )
        .unwrap();

        let parsed = parse_auth_dir_from_config(&config_path).unwrap().unwrap();
        assert_eq!(parsed, PathBuf::from(r"C:\Users\demo\.cli-proxy-api"));

        let _ = fs::remove_file(&config_path);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    fn encode_base64_url(input: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

        let mut output = String::new();
        let mut index = 0usize;

        while index < input.len() {
            let first = input[index];
            let second = input.get(index + 1).copied();
            let third = input.get(index + 2).copied();

            output.push(ALPHABET[(first >> 2) as usize] as char);
            output.push(
                ALPHABET[((first & 0b0000_0011) << 4 | second.unwrap_or(0) >> 4) as usize] as char,
            );

            if let Some(second) = second {
                output.push(
                    ALPHABET[((second & 0b0000_1111) << 2 | third.unwrap_or(0) >> 6) as usize]
                        as char,
                );
            }

            if let Some(third) = third {
                output.push(ALPHABET[(third & 0b0011_1111) as usize] as char);
            }

            index += 3;
        }

        output
    }
}
