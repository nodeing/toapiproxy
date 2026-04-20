use reqwest::{redirect::Policy, Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

const CLAUDE_SETTINGS_DIR: &str = ".claude";
const CLAUDE_SETTINGS_FILE: &str = "settings.json";
const CLAUDE_PROFILE_STORE_FILE: &str = "claude-config-profiles.json";
const LEGACY_CLAUDE_PROFILE_STORE_FILE: &str = "claude-provider-profiles.json";

const DEFAULT_CLAUDE_BASE_URL: &str = "http://127.0.0.1:8317";
const DEFAULT_AUTH_FIELD: &str = "ANTHROPIC_AUTH_TOKEN";
const DEFAULT_API_FORMAT: &str = API_FORMAT_ANTHROPIC_MESSAGES;
const CLAUDE_DUMMY_TOKEN: &str = "dummy-not-used";

const API_FORMAT_ANTHROPIC_MESSAGES: &str = "anthropic-messages";
const API_FORMAT_OPENAI_RESPONSES: &str = "openai-responses";

const COMMON_AUTH_FIELDS: &[&str] = &[
    "ANTHROPIC_AUTH_TOKEN",
    "OPENAI_API_KEY",
    "API_KEY",
    "MINIMAX_API_KEY",
    "ARK_API_KEY",
    "DASHSCOPE_API_KEY",
    "GEMINI_API_KEY",
];

const LEGACY_CURRENT_PROFILE_NAMES: &[&str] = &[
    "褰撳墠 Claude 閰嶇疆",
    "瑜版挸澧?Claude 闁板秶鐤?",
    "鐟滅増鎸告晶?Claude 闂佹澘绉堕悿?",
];

#[derive(Debug, Clone, Deserialize)]
pub struct ClaudeProviderUpsertInput {
    pub name: String,
    #[serde(rename = "baseUrl", default)]
    pub base_url: String,
    #[serde(rename = "apiFormat", default)]
    pub api_format: String,
    #[serde(rename = "authField", default)]
    pub auth_field: String,
    #[serde(rename = "apiKey", default)]
    pub api_key: String,
    #[serde(rename = "mainModel", default)]
    pub main_model: String,
    #[serde(rename = "reasoningModel", default)]
    pub reasoning_model: String,
    #[serde(rename = "haikuModel", default)]
    pub haiku_model: String,
    #[serde(rename = "sonnetModel", default)]
    pub sonnet_model: String,
    #[serde(rename = "opusModel", default)]
    pub opus_model: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaudeProviderSummary {
    pub id: String,
    pub name: String,
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    #[serde(rename = "apiFormat")]
    pub api_format: String,
    #[serde(rename = "authField")]
    pub auth_field: String,
    #[serde(rename = "apiKey")]
    pub api_key: String,
    pub enabled: bool,
    #[serde(rename = "isCurrent")]
    pub is_current: bool,
    #[serde(rename = "mainModel")]
    pub main_model: String,
    #[serde(rename = "reasoningModel")]
    pub reasoning_model: String,
    #[serde(rename = "haikuModel")]
    pub haiku_model: String,
    #[serde(rename = "sonnetModel")]
    pub sonnet_model: String,
    #[serde(rename = "opusModel")]
    pub opus_model: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct ClaudeProfileStore {
    #[serde(rename = "currentProfileId", default)]
    current_profile_id: Option<String>,
    #[serde(default)]
    profiles: Vec<ClaudeProfile>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct ClaudeProfile {
    id: String,
    name: String,
    #[serde(rename = "baseUrl", default = "default_base_url")]
    base_url: String,
    #[serde(rename = "apiFormat", default = "default_api_format")]
    api_format: String,
    #[serde(rename = "authField", default = "default_auth_field")]
    auth_field: String,
    #[serde(rename = "apiKey", default)]
    api_key: String,
    #[serde(default)]
    enabled: bool,
    #[serde(rename = "mainModel")]
    main_model: String,
    #[serde(rename = "reasoningModel")]
    reasoning_model: String,
    #[serde(rename = "haikuModel")]
    haiku_model: String,
    #[serde(rename = "sonnetModel")]
    sonnet_model: String,
    #[serde(rename = "opusModel")]
    opus_model: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct ClaudeSettingsFile {
    #[serde(default)]
    env: Map<String, Value>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

pub async fn list_claude_providers(
    _backend_port: u16,
    app_handle: &AppHandle,
) -> Result<Vec<ClaudeProviderSummary>, String> {
    let store = load_or_seed_store(app_handle)?;

    Ok(store
        .profiles
        .iter()
        .map(|profile| ClaudeProviderSummary {
            id: profile.id.clone(),
            name: profile.name.clone(),
            base_url: profile.base_url.clone(),
            api_format: profile.api_format.clone(),
            auth_field: profile.auth_field.clone(),
            api_key: profile.api_key.clone(),
            enabled: profile.enabled,
            is_current: store.current_profile_id.as_deref() == Some(profile.id.as_str()),
            main_model: profile.main_model.clone(),
            reasoning_model: profile.reasoning_model.clone(),
            haiku_model: profile.haiku_model.clone(),
            sonnet_model: profile.sonnet_model.clone(),
            opus_model: profile.opus_model.clone(),
        })
        .collect())
}

pub async fn upsert_claude_provider(
    _backend_port: u16,
    app_handle: &AppHandle,
    input: ClaudeProviderUpsertInput,
    original_id: Option<String>,
) -> Result<String, String> {
    let mut store = load_or_seed_store(app_handle)?;
    let normalized = normalize_profile_input(input)?;
    let requested_id = original_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let existing_index = requested_id
        .as_deref()
        .and_then(|profile_id| store.profiles.iter().position(|item| item.id == profile_id));
    let profile_id = requested_id.unwrap_or_else(|| Uuid::new_v4().to_string());

    let has_enabled_profile = store.profiles.iter().any(|item| item.enabled);
    let enabled = existing_index
        .and_then(|index| store.profiles.get(index))
        .map(|profile| profile.enabled)
        .unwrap_or(!has_enabled_profile);

    let next_profile = ClaudeProfile {
        id: profile_id.clone(),
        enabled,
        ..normalized
    };

    let created = existing_index.is_none();
    if let Some(index) = existing_index {
        store.profiles[index] = next_profile;
    } else {
        store.profiles.push(next_profile);
    }

    let changed = sanitize_store(&mut store);
    if !changed && enabled && store.current_profile_id.as_deref() != Some(profile_id.as_str()) {
        store.current_profile_id = Some(profile_id.clone());
    }

    save_store(app_handle, &store)?;
    sync_live_settings_from_store(&store)?;

    let target =
        find_profile(&store, &profile_id).ok_or_else(|| "保存后无法读取配置档案".to_string())?;
    let action = if created { "已创建" } else { "已保存" };
    Ok(format!("Claude 配置档案{}：{}", action, target.name))
}

pub async fn apply_claude_provider(
    _backend_port: u16,
    app_handle: &AppHandle,
    provider_id: &str,
) -> Result<String, String> {
    let mut store = load_or_seed_store(app_handle)?;
    let target_name = enable_single_profile(&mut store, provider_id)?;
    save_store(app_handle, &store)?;
    sync_live_settings_from_store(&store)?;

    Ok(format!("已启用并生效：{}", target_name))
}

pub async fn delete_claude_provider(
    _backend_port: u16,
    app_handle: &AppHandle,
    provider_id: &str,
) -> Result<String, String> {
    let mut store = load_or_seed_store(app_handle)?;
    let index = store
        .profiles
        .iter()
        .position(|item| item.id == provider_id)
        .ok_or_else(|| "未找到指定的 Claude 配置档案".to_string())?;

    let removed = store.profiles.remove(index);
    sanitize_store(&mut store);
    save_store(app_handle, &store)?;
    sync_live_settings_from_store(&store)?;

    Ok(format!("Claude 配置档案已删除：{}", removed.name))
}

pub async fn duplicate_claude_provider(
    _backend_port: u16,
    app_handle: &AppHandle,
    provider_id: &str,
) -> Result<String, String> {
    let mut store = load_or_seed_store(app_handle)?;
    let source = find_profile(&store, provider_id)
        .cloned()
        .ok_or_else(|| "未找到需要复制的配置档案".to_string())?;

    let copy_name = next_copy_name(&store, &source.name);
    store.profiles.push(ClaudeProfile {
        id: Uuid::new_v4().to_string(),
        name: copy_name.clone(),
        enabled: false,
        ..source
    });

    sanitize_store(&mut store);
    save_store(app_handle, &store)?;

    Ok(format!("已复制配置档案：{}", copy_name))
}

pub async fn set_claude_provider_enabled(
    _backend_port: u16,
    app_handle: &AppHandle,
    provider_id: &str,
    enabled: bool,
) -> Result<String, String> {
    let mut store = load_or_seed_store(app_handle)?;

    let profile_name = if enabled {
        enable_single_profile(&mut store, provider_id)?
    } else {
        disable_single_profile(&mut store, provider_id)?
    };

    sanitize_store(&mut store);
    save_store(app_handle, &store)?;
    sync_live_settings_from_store(&store)?;

    if enabled {
        Ok(format!("已启用并生效：{}", profile_name))
    } else {
        Ok(format!("已停用：{}", profile_name))
    }
}

pub async fn test_claude_provider_connectivity(
    _backend_port: u16,
    app_handle: &AppHandle,
    provider_id: &str,
) -> Result<String, String> {
    let store = load_or_seed_store(app_handle)?;
    let profile = find_profile(&store, provider_id)
        .ok_or_else(|| "未找到指定的 Claude 配置档案".to_string())?;

    let probe_url = build_probe_url(&profile.base_url, &profile.api_format)?;
    let client = Client::builder()
        .timeout(Duration::from_secs(8))
        .redirect(Policy::limited(3))
        .build()
        .map_err(|error| format!("创建测试客户端失败: {}", error))?;

    let response = build_probe_request(&client, profile, &probe_url)
        .send()
        .await
        .map_err(|error| format!("连通性测试失败: {}", error))?;
    let status = response.status();

    Ok(format!(
        "连通性测试完成：{}（{}）检测 POST {} 返回 HTTP {}{}",
        profile.name,
        api_format_label(&profile.api_format),
        probe_url,
        status,
        probe_status_note(status)
    ))
}

fn normalize_profile_input(input: ClaudeProviderUpsertInput) -> Result<ClaudeProfile, String> {
    let name = normalize_profile_name(input.name.trim());
    let base_url = normalize_base_url(&input.base_url)?;
    let api_format = normalize_api_format(&input.api_format)?;
    let auth_field = normalize_auth_field(&input.auth_field)?;
    let main_model = input.main_model.trim().to_string();

    if name.is_empty() {
        return Err("配置名称不能为空".to_string());
    }
    if main_model.is_empty() {
        return Err("主模型不能为空".to_string());
    }

    let reasoning_model = fallback_model(input.reasoning_model, &main_model);
    let haiku_model = fallback_model(input.haiku_model, &main_model);
    let sonnet_model = fallback_model(input.sonnet_model, &main_model);
    let opus_model = fallback_model(input.opus_model, &main_model);

    Ok(ClaudeProfile {
        id: String::new(),
        name,
        base_url,
        api_format,
        auth_field,
        api_key: input.api_key.trim().to_string(),
        enabled: false,
        main_model,
        reasoning_model,
        haiku_model,
        sonnet_model,
        opus_model,
    })
}

fn normalize_profile_name(candidate: &str) -> String {
    let trimmed = candidate.trim();
    if LEGACY_CURRENT_PROFILE_NAMES.contains(&trimmed) {
        "当前 Claude 配置".to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_base_url(candidate: &str) -> Result<String, String> {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return Err("Base URL 不能为空".to_string());
    }

    let parsed = Url::parse(trimmed).map_err(|error| format!("Base URL 格式无效: {}", error))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("Base URL 仅支持 http 或 https".to_string());
    }

    Ok(trimmed.trim_end_matches('/').to_string())
}

fn normalize_api_format(candidate: &str) -> Result<String, String> {
    let value = candidate.trim().to_ascii_lowercase();
    match value.as_str() {
        "" | "anthropic" | "anthropic-native" | "anthropic-messages" => {
            Ok(API_FORMAT_ANTHROPIC_MESSAGES.to_string())
        }
        "openai" | "openai-response" | "openai-responses" | "responses" => {
            Ok(API_FORMAT_OPENAI_RESPONSES.to_string())
        }
        _ => Err("API 格式仅支持 Anthropic 原生 或 OpenAI Responses API".to_string()),
    }
}

fn normalize_auth_field(candidate: &str) -> Result<String, String> {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return Ok(DEFAULT_AUTH_FIELD.to_string());
    }

    let valid = trimmed
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_');
    if !valid {
        return Err("认证字段仅支持大写字母、数字和下划线".to_string());
    }

    Ok(trimmed.to_string())
}

fn fallback_model(candidate: String, fallback: &str) -> String {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn default_base_url() -> String {
    DEFAULT_CLAUDE_BASE_URL.to_string()
}

fn default_api_format() -> String {
    DEFAULT_API_FORMAT.to_string()
}

fn default_auth_field() -> String {
    DEFAULT_AUTH_FIELD.to_string()
}

fn api_format_label(api_format: &str) -> &'static str {
    match api_format {
        API_FORMAT_OPENAI_RESPONSES => "OpenAI Responses API",
        _ => "Anthropic Messages（原生）",
    }
}

fn infer_api_format(base_url: &str) -> String {
    let lower = base_url.to_ascii_lowercase();
    if lower.contains("/v1/responses") || lower.contains("/responses") {
        return API_FORMAT_OPENAI_RESPONSES.to_string();
    }
    if lower.contains("/v1/messages") || lower.contains("/anthropic") {
        return API_FORMAT_ANTHROPIC_MESSAGES.to_string();
    }
    if is_local_base_url(base_url) {
        return API_FORMAT_OPENAI_RESPONSES.to_string();
    }
    API_FORMAT_ANTHROPIC_MESSAGES.to_string()
}

fn find_profile<'a>(store: &'a ClaudeProfileStore, provider_id: &str) -> Option<&'a ClaudeProfile> {
    store.profiles.iter().find(|item| item.id == provider_id)
}

fn enable_single_profile(
    store: &mut ClaudeProfileStore,
    provider_id: &str,
) -> Result<String, String> {
    let target_index = store
        .profiles
        .iter()
        .position(|item| item.id == provider_id)
        .ok_or_else(|| "未找到指定的 Claude 配置档案".to_string())?;

    for (index, profile) in store.profiles.iter_mut().enumerate() {
        profile.enabled = index == target_index;
    }
    let profile_name = store.profiles[target_index].name.clone();
    store.current_profile_id = Some(store.profiles[target_index].id.clone());
    Ok(profile_name)
}

fn disable_single_profile(
    store: &mut ClaudeProfileStore,
    provider_id: &str,
) -> Result<String, String> {
    let profile = store
        .profiles
        .iter_mut()
        .find(|item| item.id == provider_id)
        .ok_or_else(|| "未找到指定的 Claude 配置档案".to_string())?;

    profile.enabled = false;
    let profile_name = profile.name.clone();
    if store.current_profile_id.as_deref() == Some(provider_id) {
        store.current_profile_id = None;
    }
    Ok(profile_name)
}

fn load_or_seed_store(app_handle: &AppHandle) -> Result<ClaudeProfileStore, String> {
    let mut store = load_store(app_handle)?;
    let mut changed = hydrate_store_from_live_settings(&mut store)?;
    changed = sanitize_store(&mut store) || changed;

    if store.profiles.is_empty() {
        if let Some(profile) = import_profile_from_live_settings()? {
            store.current_profile_id = Some(profile.id.clone());
            store.profiles.push(profile);
            changed = true;
        }
    }

    if changed {
        save_store(app_handle, &store)?;
    }

    Ok(store)
}

fn load_store(app_handle: &AppHandle) -> Result<ClaudeProfileStore, String> {
    let path = profile_store_path(app_handle)?;
    if path.exists() {
        return read_store_file(&path);
    }

    let legacy_path = legacy_profile_store_path(app_handle)?;
    if legacy_path.exists() {
        return read_store_file(&legacy_path);
    }

    Ok(ClaudeProfileStore::default())
}

fn read_store_file(path: &Path) -> Result<ClaudeProfileStore, String> {
    let contents =
        fs::read_to_string(path).map_err(|error| format!("读取 Claude 配置档案失败: {}", error))?;
    serde_json::from_str(strip_utf8_bom(&contents))
        .map_err(|error| format!("解析 Claude 配置档案失败: {}", error))
}

fn save_store(app_handle: &AppHandle, store: &ClaudeProfileStore) -> Result<(), String> {
    let path = profile_store_path(app_handle)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败: {}", error))?;
    }

    let contents = serde_json::to_string_pretty(store)
        .map_err(|error| format!("序列化配置档案失败: {}", error))?;
    fs::write(&path, contents).map_err(|error| format!("写入配置档案失败: {}", error))
}

fn profile_store_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app_handle
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("获取应用数据目录失败: {}", error))?;
    Ok(app_data_dir.join(CLAUDE_PROFILE_STORE_FILE))
}

fn legacy_profile_store_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app_handle
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("获取应用数据目录失败: {}", error))?;
    Ok(app_data_dir.join(LEGACY_CLAUDE_PROFILE_STORE_FILE))
}

fn hydrate_store_from_live_settings(store: &mut ClaudeProfileStore) -> Result<bool, String> {
    if store.profiles.is_empty() {
        return Ok(false);
    }

    let settings = load_live_settings()?;
    let live_base_url = read_env_string(&settings.env, "ANTHROPIC_BASE_URL");
    let live_main_model = read_env_string(&settings.env, "ANTHROPIC_MODEL");
    let live_reasoning_model = read_env_string(&settings.env, "ANTHROPIC_REASONING_MODEL");
    let live_haiku_model = read_env_string(&settings.env, "ANTHROPIC_DEFAULT_HAIKU_MODEL");
    let live_sonnet_model = read_env_string(&settings.env, "ANTHROPIC_DEFAULT_SONNET_MODEL");
    let live_opus_model = read_env_string(&settings.env, "ANTHROPIC_DEFAULT_OPUS_MODEL");
    let detected_auth = detect_auth_pair(&settings.env);

    let current_id = store
        .current_profile_id
        .clone()
        .or_else(|| {
            store
                .profiles
                .iter()
                .find(|item| item.enabled)
                .map(|item| item.id.clone())
        })
        .or_else(|| store.profiles.first().map(|item| item.id.clone()));

    let Some(current_id) = current_id else {
        return Ok(false);
    };

    let Some(profile) = store.profiles.iter_mut().find(|item| item.id == current_id) else {
        return Ok(false);
    };

    let mut changed = false;
    if profile.base_url.trim().is_empty() && !live_base_url.is_empty() {
        profile.base_url = live_base_url.clone();
        changed = true;
    }
    if profile.api_format.trim().is_empty() {
        profile.api_format =
            infer_api_format(&non_empty_or(live_base_url.clone(), &profile.base_url));
        changed = true;
    }
    if profile.auth_field.trim().is_empty() {
        profile.auth_field = detected_auth
            .as_ref()
            .map(|(field, _)| field.clone())
            .unwrap_or_else(default_auth_field);
        changed = true;
    }
    if profile.api_key.trim().is_empty() {
        if let Some((field, value)) = detected_auth {
            if profile.auth_field == field {
                let imported = sanitize_imported_api_key(&profile.base_url, &field, &value);
                if !imported.is_empty() {
                    profile.api_key = imported;
                    changed = true;
                }
            }
        }
    }
    if profile.main_model.trim().is_empty() && !live_main_model.is_empty() {
        profile.main_model = live_main_model.clone();
        changed = true;
    }
    if profile.reasoning_model.trim().is_empty() && !live_reasoning_model.is_empty() {
        profile.reasoning_model = live_reasoning_model;
        changed = true;
    }
    if profile.haiku_model.trim().is_empty() && !live_haiku_model.is_empty() {
        profile.haiku_model = live_haiku_model;
        changed = true;
    }
    if profile.sonnet_model.trim().is_empty() && !live_sonnet_model.is_empty() {
        profile.sonnet_model = live_sonnet_model;
        changed = true;
    }
    if profile.opus_model.trim().is_empty() && !live_opus_model.is_empty() {
        profile.opus_model = live_opus_model;
        changed = true;
    }

    Ok(changed)
}

fn sanitize_store(store: &mut ClaudeProfileStore) -> bool {
    let mut changed = false;

    for profile in &mut store.profiles {
        let normalized_name = normalize_profile_name(&profile.name);
        if profile.name != normalized_name {
            profile.name = normalized_name;
            changed = true;
        }
        if profile.base_url.trim().is_empty() {
            profile.base_url = default_base_url();
            changed = true;
        }
        let normalized_format = infer_api_format(&profile.base_url);
        if profile.api_format.trim().is_empty() {
            profile.api_format = normalized_format;
            changed = true;
        } else {
            let next_format = normalize_api_format(&profile.api_format)
                .unwrap_or_else(|_| infer_api_format(&profile.base_url));
            if profile.api_format != next_format {
                profile.api_format = next_format;
                changed = true;
            }
        }
        if profile.auth_field.trim().is_empty() {
            profile.auth_field = default_auth_field();
            changed = true;
        }
    }

    let enabled_ids: Vec<String> = store
        .profiles
        .iter()
        .filter(|item| item.enabled)
        .map(|item| item.id.clone())
        .collect();

    let desired_current_id = if enabled_ids.is_empty() {
        None
    } else if let Some(current_id) = store.current_profile_id.clone() {
        if enabled_ids.contains(&current_id) {
            Some(current_id)
        } else {
            enabled_ids.first().cloned()
        }
    } else {
        enabled_ids.first().cloned()
    };

    for profile in &mut store.profiles {
        let should_enable = desired_current_id
            .as_deref()
            .map(|id| id == profile.id)
            .unwrap_or(false);
        if profile.enabled != should_enable {
            profile.enabled = should_enable;
            changed = true;
        }
    }

    if store.current_profile_id != desired_current_id {
        store.current_profile_id = desired_current_id;
        changed = true;
    }

    changed
}

fn next_copy_name(store: &ClaudeProfileStore, source_name: &str) -> String {
    let mut suffix = 1usize;
    loop {
        let candidate = if suffix == 1 {
            format!("{} 副本", source_name)
        } else {
            format!("{} 副本 {}", source_name, suffix)
        };

        if store.profiles.iter().all(|item| item.name != candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

fn claude_settings_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法定位用户目录".to_string())?;
    Ok(home.join(CLAUDE_SETTINGS_DIR).join(CLAUDE_SETTINGS_FILE))
}

fn load_live_settings() -> Result<ClaudeSettingsFile, String> {
    let path = claude_settings_path()?;
    if !path.exists() {
        return Ok(ClaudeSettingsFile::default());
    }

    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("读取 Claude 全局设置失败: {}", error))?;
    serde_json::from_str(strip_utf8_bom(&contents))
        .map_err(|error| format!("解析 Claude 全局设置失败: {}", error))
}

fn save_live_settings(settings: &ClaudeSettingsFile) -> Result<(), String> {
    let path = claude_settings_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建 Claude 配置目录失败: {}", error))?;
    }

    let contents = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("序列化 Claude 全局设置失败: {}", error))?;
    fs::write(&path, contents).map_err(|error| format!("写入 Claude 全局设置失败: {}", error))
}

fn sync_live_settings_from_store(store: &ClaudeProfileStore) -> Result<(), String> {
    let current_id = store.current_profile_id.as_deref();
    if let Some(profile_id) = current_id {
        if let Some(profile) = find_profile(store, profile_id) {
            if profile.enabled {
                return write_live_settings(store, profile);
            }
        }
    }

    clear_live_settings(store)
}

fn write_live_settings(store: &ClaudeProfileStore, profile: &ClaudeProfile) -> Result<(), String> {
    let mut settings = load_live_settings()?;
    clear_managed_auth_fields(&mut settings, store);

    let auth_value = if !profile.api_key.trim().is_empty() {
        profile.api_key.trim().to_string()
    } else if profile.auth_field == DEFAULT_AUTH_FIELD && is_local_base_url(&profile.base_url) {
        CLAUDE_DUMMY_TOKEN.to_string()
    } else {
        String::new()
    };

    if !auth_value.is_empty() {
        settings
            .env
            .insert(profile.auth_field.clone(), Value::String(auth_value));
    }

    settings.env.insert(
        "ANTHROPIC_BASE_URL".to_string(),
        Value::String(profile.base_url.clone()),
    );
    settings.env.insert(
        "ANTHROPIC_MODEL".to_string(),
        Value::String(profile.main_model.clone()),
    );
    settings.env.insert(
        "ANTHROPIC_REASONING_MODEL".to_string(),
        Value::String(profile.reasoning_model.clone()),
    );
    settings.env.insert(
        "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
        Value::String(profile.haiku_model.clone()),
    );
    settings.env.insert(
        "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
        Value::String(profile.sonnet_model.clone()),
    );
    settings.env.insert(
        "ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(),
        Value::String(profile.opus_model.clone()),
    );

    save_live_settings(&settings)
}

fn clear_live_settings(store: &ClaudeProfileStore) -> Result<(), String> {
    let mut settings = load_live_settings()?;
    clear_managed_auth_fields(&mut settings, store);

    for key in [
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_MODEL",
        "ANTHROPIC_REASONING_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
    ] {
        settings.env.remove(key);
    }

    save_live_settings(&settings)
}

fn clear_managed_auth_fields(settings: &mut ClaudeSettingsFile, store: &ClaudeProfileStore) {
    for key in collect_managed_auth_fields(store) {
        settings.env.remove(&key);
    }
}

fn collect_managed_auth_fields(store: &ClaudeProfileStore) -> HashSet<String> {
    let mut keys = HashSet::new();
    for key in COMMON_AUTH_FIELDS {
        keys.insert((*key).to_string());
    }
    for profile in &store.profiles {
        if !profile.auth_field.trim().is_empty() {
            keys.insert(profile.auth_field.clone());
        }
    }
    keys
}

fn import_profile_from_live_settings() -> Result<Option<ClaudeProfile>, String> {
    let settings = load_live_settings()?;
    let main_model = read_env_string(&settings.env, "ANTHROPIC_MODEL");
    if main_model.is_empty() {
        return Ok(None);
    }

    let base_url = non_empty_or(
        read_env_string(&settings.env, "ANTHROPIC_BASE_URL"),
        DEFAULT_CLAUDE_BASE_URL,
    );
    let (auth_field, api_key) = detect_auth_pair(&settings.env)
        .map(|(field, value)| {
            let sanitized = sanitize_imported_api_key(&base_url, &field, &value);
            (field, sanitized)
        })
        .unwrap_or_else(|| (DEFAULT_AUTH_FIELD.to_string(), String::new()));

    Ok(Some(ClaudeProfile {
        id: Uuid::new_v4().to_string(),
        name: "当前 Claude 配置".to_string(),
        base_url: base_url.clone(),
        api_format: infer_api_format(&base_url),
        auth_field,
        api_key,
        enabled: true,
        main_model: main_model.clone(),
        reasoning_model: non_empty_or(
            read_env_string(&settings.env, "ANTHROPIC_REASONING_MODEL"),
            &main_model,
        ),
        haiku_model: non_empty_or(
            read_env_string(&settings.env, "ANTHROPIC_DEFAULT_HAIKU_MODEL"),
            &main_model,
        ),
        sonnet_model: non_empty_or(
            read_env_string(&settings.env, "ANTHROPIC_DEFAULT_SONNET_MODEL"),
            &main_model,
        ),
        opus_model: non_empty_or(
            read_env_string(&settings.env, "ANTHROPIC_DEFAULT_OPUS_MODEL"),
            &main_model,
        ),
    }))
}

fn detect_auth_pair(env: &Map<String, Value>) -> Option<(String, String)> {
    for key in COMMON_AUTH_FIELDS {
        let value = read_env_string(env, key);
        if !value.is_empty() {
            return Some(((*key).to_string(), value));
        }
    }

    env.iter().find_map(|(key, value)| {
        let key_upper = key.trim().to_ascii_uppercase();
        if [
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_MODEL",
            "ANTHROPIC_REASONING_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
        ]
        .contains(&key_upper.as_str())
        {
            return None;
        }

        let is_auth_like = key_upper.ends_with("_API_KEY")
            || key_upper.ends_with("_AUTH_TOKEN")
            || key_upper.ends_with("_TOKEN");
        if !is_auth_like {
            return None;
        }

        value
            .as_str()
            .map(str::trim)
            .filter(|candidate| !candidate.is_empty())
            .map(|candidate| (key.clone(), candidate.to_string()))
    })
}

fn sanitize_imported_api_key(base_url: &str, auth_field: &str, api_key: &str) -> String {
    if auth_field == DEFAULT_AUTH_FIELD
        && api_key == CLAUDE_DUMMY_TOKEN
        && is_local_base_url(base_url)
    {
        String::new()
    } else {
        api_key.to_string()
    }
}

fn is_local_base_url(base_url: &str) -> bool {
    let Ok(url) = Url::parse(base_url) else {
        return false;
    };

    matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("::1")
    )
}

fn build_probe_request(
    client: &Client,
    profile: &ClaudeProfile,
    probe_url: &str,
) -> reqwest::RequestBuilder {
    let payload = match profile.api_format.as_str() {
        API_FORMAT_OPENAI_RESPONSES => json!({
            "model": profile.main_model.trim(),
            "input": "ping",
            "max_output_tokens": 1,
        }),
        _ => json!({
            "model": profile.main_model.trim(),
            "max_tokens": 1,
            "messages": [
                {
                    "role": "user",
                    "content": "ping",
                }
            ],
        }),
    };

    let mut request = client.post(probe_url).json(&payload);
    let api_key = probe_api_key(profile);
    if !api_key.is_empty() {
        request = request
            .header("x-api-key", api_key.as_str())
            .header("api-key", api_key.as_str())
            .header("Authorization", format!("Bearer {}", api_key));
    }

    if profile.api_format != API_FORMAT_OPENAI_RESPONSES {
        request = request.header("anthropic-version", "2023-06-01");
    }

    request
}

fn probe_api_key(profile: &ClaudeProfile) -> String {
    let trimmed = profile.api_key.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }

    if profile.auth_field == DEFAULT_AUTH_FIELD && is_local_base_url(&profile.base_url) {
        return CLAUDE_DUMMY_TOKEN.to_string();
    }

    String::new()
}

fn probe_status_note(status: StatusCode) -> &'static str {
    match status {
        StatusCode::OK | StatusCode::CREATED | StatusCode::ACCEPTED => " (request accepted)",
        StatusCode::BAD_REQUEST => " (endpoint reachable; request format or model rejected)",
        StatusCode::UNAUTHORIZED => " (endpoint reachable; auth missing or invalid)",
        StatusCode::FORBIDDEN => " (endpoint reachable; auth forbidden)",
        StatusCode::NOT_FOUND => " (endpoint missing; check Base URL or API format)",
        StatusCode::METHOD_NOT_ALLOWED => " (endpoint reachable; method not allowed)",
        StatusCode::UNPROCESSABLE_ENTITY => " (endpoint reachable; request parameters invalid)",
        StatusCode::TOO_MANY_REQUESTS => " (endpoint reachable; rate limited)",
        _ if status.is_server_error() => " (endpoint reachable; upstream error)",
        _ => "",
    }
}

fn build_probe_url(base_url: &str, api_format: &str) -> Result<String, String> {
    let mut url = Url::parse(base_url)
        .map_err(|error| format!("Base URL 格式无效，无法生成测试地址: {}", error))?;

    let desired_tail = match api_format {
        API_FORMAT_OPENAI_RESPONSES => "responses",
        _ => "messages",
    };

    let current_path = url.path().trim_end_matches('/').to_string();
    let next_path =
        if current_path.ends_with("/v1/messages") || current_path.ends_with("/v1/responses") {
            current_path
        } else if current_path.is_empty() || current_path == "/" {
            format!("/v1/{}", desired_tail)
        } else if current_path.ends_with("/v1") {
            format!("{}/{}", current_path, desired_tail)
        } else {
            format!("{}/v1/{}", current_path, desired_tail)
        };

    url.set_path(&next_path);
    Ok(url.to_string())
}

fn read_env_string(env: &Map<String, Value>, key: &str) -> String {
    env.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
        .to_string()
}

fn strip_utf8_bom(contents: &str) -> &str {
    contents.strip_prefix('\u{feff}').unwrap_or(contents)
}

fn non_empty_or(value: String, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_probe_request, build_probe_url, fallback_model, infer_api_format,
        normalize_api_format, normalize_auth_field, normalize_base_url, normalize_profile_input,
        probe_api_key, sanitize_imported_api_key, ClaudeProfile, ClaudeProviderUpsertInput,
        API_FORMAT_ANTHROPIC_MESSAGES, API_FORMAT_OPENAI_RESPONSES, CLAUDE_DUMMY_TOKEN,
        DEFAULT_AUTH_FIELD,
    };
    use reqwest::{Client, Method};

    #[test]
    fn empty_secondary_models_fall_back_to_main_model() {
        let profile = normalize_profile_input(ClaudeProviderUpsertInput {
            name: "GPT-5.4".to_string(),
            base_url: "http://127.0.0.1:8317".to_string(),
            api_format: API_FORMAT_OPENAI_RESPONSES.to_string(),
            auth_field: "ANTHROPIC_AUTH_TOKEN".to_string(),
            api_key: String::new(),
            main_model: "gpt-5.4".to_string(),
            reasoning_model: String::new(),
            haiku_model: String::new(),
            sonnet_model: String::new(),
            opus_model: String::new(),
        })
        .expect("profile should be valid");

        assert_eq!(profile.reasoning_model, "gpt-5.4");
        assert_eq!(profile.haiku_model, "gpt-5.4");
        assert_eq!(profile.sonnet_model, "gpt-5.4");
        assert_eq!(profile.opus_model, "gpt-5.4");
    }

    #[test]
    fn fallback_model_prefers_non_empty_value() {
        assert_eq!(fallback_model("gpt-4.1".to_string(), "gpt-5.4"), "gpt-4.1");
        assert_eq!(fallback_model("".to_string(), "gpt-5.4"), "gpt-5.4");
    }

    #[test]
    fn normalize_base_url_trims_trailing_slash() {
        assert_eq!(
            normalize_base_url("http://127.0.0.1:8317/").expect("url should be valid"),
            "http://127.0.0.1:8317"
        );
    }

    #[test]
    fn normalize_auth_field_accepts_uppercase_env_names() {
        assert_eq!(
            normalize_auth_field("OPENAI_API_KEY").expect("auth field should be valid"),
            "OPENAI_API_KEY"
        );
    }

    #[test]
    fn imported_dummy_token_is_hidden_for_local_proxy() {
        assert_eq!(
            sanitize_imported_api_key(
                "http://127.0.0.1:8317",
                "ANTHROPIC_AUTH_TOKEN",
                CLAUDE_DUMMY_TOKEN
            ),
            ""
        );
    }

    #[test]
    fn normalize_api_format_accepts_supported_values() {
        assert_eq!(
            normalize_api_format("anthropic-native").expect("anthropic should be valid"),
            API_FORMAT_ANTHROPIC_MESSAGES
        );
        assert_eq!(
            normalize_api_format("openai-responses").expect("responses should be valid"),
            API_FORMAT_OPENAI_RESPONSES
        );
    }

    #[test]
    fn infer_api_format_marks_local_proxy_as_openai_responses() {
        assert_eq!(
            infer_api_format("http://127.0.0.1:8317"),
            API_FORMAT_OPENAI_RESPONSES
        );
        assert_eq!(
            infer_api_format("https://api.minimaxi.com/anthropic"),
            API_FORMAT_ANTHROPIC_MESSAGES
        );
    }

    #[test]
    fn build_probe_url_appends_matching_endpoint_path() {
        assert_eq!(
            build_probe_url("http://127.0.0.1:8317", API_FORMAT_OPENAI_RESPONSES)
                .expect("probe url should be valid"),
            "http://127.0.0.1:8317/v1/responses"
        );
        assert_eq!(
            build_probe_url(
                "https://api.minimaxi.com/anthropic",
                API_FORMAT_ANTHROPIC_MESSAGES
            )
            .expect("probe url should be valid"),
            "https://api.minimaxi.com/anthropic/v1/messages"
        );
    }

    #[test]
    fn probe_api_key_uses_dummy_token_for_local_default_auth() {
        let profile = ClaudeProfile {
            base_url: "http://127.0.0.1:8317".to_string(),
            auth_field: DEFAULT_AUTH_FIELD.to_string(),
            ..Default::default()
        };

        assert_eq!(probe_api_key(&profile), CLAUDE_DUMMY_TOKEN);
    }

    #[test]
    fn build_probe_request_posts_anthropic_payload() {
        let profile = ClaudeProfile {
            base_url: "https://api.minimaxi.com/anthropic".to_string(),
            api_format: API_FORMAT_ANTHROPIC_MESSAGES.to_string(),
            auth_field: DEFAULT_AUTH_FIELD.to_string(),
            api_key: "test-key".to_string(),
            main_model: "MiniMax-M2.7".to_string(),
            ..Default::default()
        };

        let request = build_probe_request(
            &Client::new(),
            &profile,
            "https://api.minimaxi.com/anthropic/v1/messages",
        )
        .build()
        .expect("request should build");

        assert_eq!(request.method(), Method::POST);
        assert_eq!(
            request
                .headers()
                .get("anthropic-version")
                .and_then(|value| value.to_str().ok()),
            Some("2023-06-01")
        );
        assert_eq!(
            request
                .headers()
                .get("x-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("test-key")
        );

        let body = request
            .body()
            .and_then(|body| body.as_bytes())
            .expect("request body should be buffered");
        let body_text = std::str::from_utf8(body).expect("body should be utf-8");
        assert!(body_text.contains("\"messages\""));
        assert!(body_text.contains("\"max_tokens\":1"));
    }

    #[test]
    fn build_probe_request_posts_openai_responses_payload() {
        let profile = ClaudeProfile {
            base_url: "http://127.0.0.1:8317".to_string(),
            api_format: API_FORMAT_OPENAI_RESPONSES.to_string(),
            auth_field: DEFAULT_AUTH_FIELD.to_string(),
            main_model: "gpt-5.4".to_string(),
            ..Default::default()
        };

        let request = build_probe_request(
            &Client::new(),
            &profile,
            "http://127.0.0.1:8317/v1/responses",
        )
        .build()
        .expect("request should build");

        assert_eq!(request.method(), Method::POST);
        assert!(
            request.headers().get("anthropic-version").is_none(),
            "responses probe should not send anthropic-version"
        );
        assert_eq!(
            request
                .headers()
                .get("x-api-key")
                .and_then(|value| value.to_str().ok()),
            Some(CLAUDE_DUMMY_TOKEN)
        );

        let body = request
            .body()
            .and_then(|body| body.as_bytes())
            .expect("request body should be buffered");
        let body_text = std::str::from_utf8(body).expect("body should be utf-8");
        assert!(body_text.contains("\"input\":\"ping\""));
        assert!(body_text.contains("\"max_output_tokens\":1"));
    }
}
