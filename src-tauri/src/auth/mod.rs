use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthAccount {
    pub id: String,
    pub email: String,
    pub provider: String,
    pub service: String,
    pub subscription: Option<String>,
    pub usage: Option<UsageData>,
    pub created_at: String,
    pub is_expired: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageData {
    pub used: i32,
    pub limit: i32,
    pub percent: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_days: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bonus_used: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bonus_limit: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceType {
    Claude,
    Codex,
    Gemini,
    Copilot,
    Qwen,
    Kiro,
    Antigravity,
}

impl ServiceType {
    pub fn display_name(&self) -> &'static str {
        match self {
            ServiceType::Claude => "Claude",
            ServiceType::Codex => "Codex",
            ServiceType::Gemini => "Gemini",
            ServiceType::Copilot => "GitHub Copilot",
            ServiceType::Qwen => "Qwen",
            ServiceType::Kiro => "Kiro",
            ServiceType::Antigravity => "Antigravity",
        }
    }
}

pub struct AuthManager {
    auth_dir: PathBuf,
    accounts: HashMap<ServiceType, Vec<AuthAccount>>,
}

impl AuthManager {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        let auth_dir = home.join(".cli-proxy-api");

        Self {
            auth_dir,
            accounts: HashMap::new(),
        }
    }

    pub fn auth_dir(&self) -> &PathBuf {
        &self.auth_dir
    }

    pub fn ensure_auth_dir(&self) -> Result<(), String> {
        if !self.auth_dir.exists() {
            fs::create_dir_all(&self.auth_dir).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn scan_accounts(&mut self) -> Result<(), String> {
        self.ensure_auth_dir()?;
        self.accounts.clear();

        if let Ok(entries) = fs::read_dir(&self.auth_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(type_str) = json.get("type").and_then(|v| v.as_str()) {
                                if let Some(service_type) = Self::parse_service_type(type_str) {
                                    if let Some(account) =
                                        self.parse_auth_file(&path, &json, service_type)
                                    {
                                        self.accounts
                                            .entry(service_type)
                                            .or_insert_with(Vec::new)
                                            .push(account);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn parse_service_type(type_str: &str) -> Option<ServiceType> {
        match type_str.to_lowercase().as_str() {
            "claude" => Some(ServiceType::Claude),
            "codex" => Some(ServiceType::Codex),
            "gemini" => Some(ServiceType::Gemini),
            "github-copilot" | "copilot" => Some(ServiceType::Copilot),
            "qwen" => Some(ServiceType::Qwen),
            "kiro" => Some(ServiceType::Kiro),
            "antigravity" => Some(ServiceType::Antigravity),
            _ => None,
        }
    }

    fn parse_auth_file(
        &self,
        path: &PathBuf,
        json: &serde_json::Value,
        service: ServiceType,
    ) -> Option<AuthAccount> {
        let email = json
            .get("email")
            .or_else(|| json.get("login"))
            .or_else(|| json.get("user"))
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        let id = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        // 检查是否过期
        let is_expired = self.check_expired(json);

        // 获取 access_token (用于查询用量)
        let access_token = json
            .get("access_token")
            .or_else(|| json.get("accessToken"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // 获取 provider
        let provider = json
            .get("provider")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| service.display_name().to_string());

        Some(AuthAccount {
            id,
            email,
            provider,
            service: format!("{:?}", service).to_lowercase(),
            subscription: None,
            usage: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            is_expired,
            access_token,
            file_path: Some(path.to_string_lossy().to_string()),
        })
    }

    fn check_expired(&self, json: &serde_json::Value) -> bool {
        let expired_str = json
            .get("expires_at")
            .or_else(|| json.get("expired"))
            .or_else(|| json.get("expiresAt"))
            .and_then(|v| v.as_str());

        if let Some(exp) = expired_str {
            if let Ok(exp_time) = chrono::DateTime::parse_from_rfc3339(exp) {
                return exp_time < chrono::Utc::now();
            }
        }
        false
    }

    pub fn get_accounts(&self, service: ServiceType) -> Vec<AuthAccount> {
        self.accounts.get(&service).cloned().unwrap_or_default()
    }

    pub fn get_all_accounts(&self) -> Vec<AuthAccount> {
        self.accounts.values().flatten().cloned().collect()
    }

    pub fn remove_account(&mut self, account_id: &str) -> Result<(), String> {
        let file_path = self.auth_dir.join(account_id);
        if file_path.exists() {
            fs::remove_file(&file_path).map_err(|e| e.to_string())?;
        }

        for accounts in self.accounts.values_mut() {
            accounts.retain(|a| a.id != account_id);
        }
        Ok(())
    }

    pub fn is_connected(&self, service: ServiceType) -> bool {
        self.accounts
            .get(&service)
            .map(|a| !a.is_empty())
            .unwrap_or(false)
    }

    /// 获取指定账户的最新 access_token (从文件重新读取)
    pub fn get_fresh_token(&self, account_id: &str) -> Option<String> {
        let file_path = self.auth_dir.join(account_id);
        if let Ok(content) = fs::read_to_string(&file_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                return json
                    .get("access_token")
                    .or_else(|| json.get("accessToken"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
        }
        None
    }

    /// 从 Kiro IDE 导入 token
    /// Kiro IDE 的 token 文件位于 ~/.aws/sso/cache/kiro-auth-token.json
    pub fn import_from_kiro_ide(&mut self) -> Result<AuthAccount, String> {
        let home = dirs::home_dir().ok_or("Cannot find home directory")?;
        let kiro_token_path = home.join(".aws/sso/cache/kiro-auth-token.json");

        if !kiro_token_path.exists() {
            return Err(
                "Kiro IDE token file not found. Please login in Kiro IDE first.".to_string(),
            );
        }

        let content = fs::read_to_string(&kiro_token_path)
            .map_err(|e| format!("Failed to read Kiro token file: {}", e))?;

        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse Kiro token file: {}", e))?;

        let access_token = json
            .get("accessToken")
            .or_else(|| json.get("access_token"))
            .and_then(|v| v.as_str())
            .ok_or("No access token found in Kiro token file")?;

        let refresh_token = json
            .get("refreshToken")
            .or_else(|| json.get("refresh_token"))
            .and_then(|v| v.as_str());

        let provider = json
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or("kiro");

        let expires_at = json
            .get("expiresAt")
            .or_else(|| json.get("expires_at"))
            .and_then(|v| v.as_str());

        // 创建账户文件
        self.ensure_auth_dir()?;

        let account_id = format!("kiro-ide-{}.json", chrono::Utc::now().timestamp());
        let account_path = self.auth_dir.join(&account_id);

        let mut account_data = serde_json::json!({
            "type": "kiro",
            "email": "Imported from Kiro IDE",
            "provider": provider,
            "access_token": access_token,
        });

        if let Some(rt) = refresh_token {
            account_data["refresh_token"] = serde_json::Value::String(rt.to_string());
        }
        if let Some(exp) = expires_at {
            account_data["expires_at"] = serde_json::Value::String(exp.to_string());
        }

        fs::write(
            &account_path,
            serde_json::to_string_pretty(&account_data).unwrap(),
        )
        .map_err(|e| format!("Failed to save account: {}", e))?;

        let account = AuthAccount {
            id: account_id,
            email: "Imported from Kiro IDE".to_string(),
            provider: provider.to_string(),
            service: "kiro".to_string(),
            subscription: None,
            usage: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            is_expired: false,
            access_token: Some(access_token.to_string()),
            file_path: Some(account_path.to_string_lossy().to_string()),
        };

        // 添加到内存中的账户列表
        self.accounts
            .entry(ServiceType::Kiro)
            .or_insert_with(Vec::new)
            .push(account.clone());

        log::info!("[AuthManager] Imported Kiro account from IDE");
        Ok(account)
    }
}
