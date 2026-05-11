use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

const CODEX_DIR: &str = ".codex";
const CODEX_AUTH_FILE: &str = "auth.json";
const CODEX_CONFIG_FILE: &str = "config.toml";
const CODEX_PROFILE_STORE_FILE: &str = "codex-config-profiles.json";

const DEFAULT_PROVIDER_ID: &str = "toapiproxy";
const DEFAULT_PROVIDER_NAME: &str = "ToapiProxy";
const DEFAULT_API_KEY: &str = "dummy-not-used";
const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8317/v1";
const DEFAULT_MODEL: &str = "gpt-5.4";
const DEFAULT_REASONING_EFFORT: &str = "xhigh";
const DEFAULT_WIRE_API: &str = "responses";

#[derive(Debug, Clone, Deserialize)]
pub struct CodexConfigUpsertInput {
    pub name: String,
    #[serde(rename = "providerId", default)]
    pub provider_id: String,
    #[serde(rename = "providerName", default)]
    pub provider_name: String,
    #[serde(rename = "apiKey", default)]
    pub api_key: String,
    #[serde(rename = "baseUrl", default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(rename = "reasoningEffort", default)]
    pub reasoning_effort: String,
    #[serde(rename = "wireApi", default)]
    pub wire_api: String,
    #[serde(rename = "requiresOpenAIAuth", default = "default_true")]
    pub requires_openai_auth: bool,
    #[serde(rename = "disableResponseStorage", default = "default_true")]
    pub disable_response_storage: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexConfigProfileSummary {
    pub id: String,
    pub name: String,
    #[serde(rename = "providerId")]
    pub provider_id: String,
    #[serde(rename = "providerName")]
    pub provider_name: String,
    #[serde(rename = "apiKey")]
    pub api_key: String,
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    pub model: String,
    #[serde(rename = "reasoningEffort")]
    pub reasoning_effort: String,
    #[serde(rename = "wireApi")]
    pub wire_api: String,
    #[serde(rename = "requiresOpenAIAuth")]
    pub requires_openai_auth: bool,
    #[serde(rename = "disableResponseStorage")]
    pub disable_response_storage: bool,
    pub enabled: bool,
    #[serde(rename = "isCurrent")]
    pub is_current: bool,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct CodexProfileStore {
    #[serde(rename = "currentProfileId", default)]
    current_profile_id: Option<String>,
    #[serde(default)]
    profiles: Vec<CodexConfigProfile>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct CodexConfigProfile {
    id: String,
    name: String,
    #[serde(rename = "providerId", default = "default_provider_id")]
    provider_id: String,
    #[serde(rename = "providerName", default = "default_provider_name")]
    provider_name: String,
    #[serde(rename = "apiKey", default)]
    api_key: String,
    #[serde(rename = "baseUrl", default = "default_base_url")]
    base_url: String,
    #[serde(default = "default_model")]
    model: String,
    #[serde(rename = "reasoningEffort", default = "default_reasoning_effort")]
    reasoning_effort: String,
    #[serde(rename = "wireApi", default = "default_wire_api")]
    wire_api: String,
    #[serde(rename = "requiresOpenAIAuth", default = "default_true")]
    requires_openai_auth: bool,
    #[serde(rename = "disableResponseStorage", default = "default_true")]
    disable_response_storage: bool,
    #[serde(default)]
    enabled: bool,
    #[serde(rename = "createdAt", default = "now_rfc3339")]
    created_at: String,
    #[serde(rename = "updatedAt", default = "now_rfc3339")]
    updated_at: String,
}

#[derive(Debug, Clone, Default)]
struct CodexLiveConfig {
    provider_id: String,
    provider_name: String,
    api_key: String,
    base_url: String,
    model: String,
    reasoning_effort: String,
    wire_api: String,
    requires_openai_auth: bool,
    disable_response_storage: bool,
}

pub fn list_codex_config_profiles(
    app_handle: &AppHandle,
) -> Result<Vec<CodexConfigProfileSummary>, String> {
    let store = load_or_seed_store(app_handle)?;
    Ok(store
        .profiles
        .iter()
        .map(|profile| summarize_profile(profile, &store))
        .collect())
}

pub fn upsert_codex_config_profile(
    app_handle: &AppHandle,
    input: CodexConfigUpsertInput,
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
    let now = now_rfc3339();

    let enabled = existing_index
        .and_then(|index| store.profiles.get(index))
        .map(|profile| profile.enabled)
        .unwrap_or(false);
    let created_at = existing_index
        .and_then(|index| store.profiles.get(index))
        .map(|profile| profile.created_at.clone())
        .unwrap_or_else(|| now.clone());

    let next_profile = CodexConfigProfile {
        id: profile_id.clone(),
        enabled,
        created_at,
        updated_at: now,
        ..normalized
    };

    let created = existing_index.is_none();
    if let Some(index) = existing_index {
        store.profiles[index] = next_profile;
    } else {
        store.profiles.push(next_profile);
    }

    sanitize_store(&mut store);
    save_store(app_handle, &store)?;

    let action = if created { "已创建" } else { "已保存" };
    let target = find_profile(&store, &profile_id)
        .ok_or_else(|| "保存后无法读取 Codex 配置档案".to_string())?;
    Ok(format!("Codex 配置档案{}：{}", action, target.name))
}

pub fn apply_codex_config_profile(
    app_handle: &AppHandle,
    profile_id: &str,
) -> Result<String, String> {
    let mut store = load_or_seed_store(app_handle)?;
    let target = find_profile(&store, profile_id)
        .cloned()
        .ok_or_else(|| "未找到指定的 Codex 配置档案".to_string())?;

    write_live_config(&target)?;

    for profile in &mut store.profiles {
        profile.enabled = profile.id == target.id;
    }
    store.current_profile_id = Some(target.id.clone());
    save_store(app_handle, &store)?;

    Ok(format!("Codex 配置已应用：{}", target.name))
}

pub fn delete_codex_config_profile(
    app_handle: &AppHandle,
    profile_id: &str,
) -> Result<String, String> {
    let mut store = load_or_seed_store(app_handle)?;
    let index = store
        .profiles
        .iter()
        .position(|item| item.id == profile_id)
        .ok_or_else(|| "未找到指定的 Codex 配置档案".to_string())?;

    let removed = store.profiles.remove(index);
    if store.current_profile_id.as_deref() == Some(profile_id) {
        store.current_profile_id = None;
    }
    sanitize_store(&mut store);
    save_store(app_handle, &store)?;

    Ok(format!("Codex 配置档案已删除：{}", removed.name))
}

pub fn duplicate_codex_config_profile(
    app_handle: &AppHandle,
    profile_id: &str,
) -> Result<String, String> {
    let mut store = load_or_seed_store(app_handle)?;
    let source = find_profile(&store, profile_id)
        .cloned()
        .ok_or_else(|| "未找到需要复制的 Codex 配置档案".to_string())?;
    let copy_name = next_copy_name(&store, &source.name);
    let now = now_rfc3339();

    store.profiles.push(CodexConfigProfile {
        id: Uuid::new_v4().to_string(),
        name: copy_name.clone(),
        enabled: false,
        created_at: now.clone(),
        updated_at: now,
        ..source
    });
    sanitize_store(&mut store);
    save_store(app_handle, &store)?;

    Ok(format!("已复制 Codex 配置档案：{}", copy_name))
}

fn summarize_profile(
    profile: &CodexConfigProfile,
    store: &CodexProfileStore,
) -> CodexConfigProfileSummary {
    CodexConfigProfileSummary {
        id: profile.id.clone(),
        name: profile.name.clone(),
        provider_id: profile.provider_id.clone(),
        provider_name: profile.provider_name.clone(),
        api_key: profile.api_key.clone(),
        base_url: profile.base_url.clone(),
        model: profile.model.clone(),
        reasoning_effort: profile.reasoning_effort.clone(),
        wire_api: profile.wire_api.clone(),
        requires_openai_auth: profile.requires_openai_auth,
        disable_response_storage: profile.disable_response_storage,
        enabled: profile.enabled,
        is_current: store.current_profile_id.as_deref() == Some(profile.id.as_str()),
        updated_at: profile.updated_at.clone(),
    }
}

fn load_or_seed_store(app_handle: &AppHandle) -> Result<CodexProfileStore, String> {
    let mut store = load_store(app_handle)?;
    if store.profiles.is_empty() {
        let live = load_live_config()?;
        if live_config_has_values(&live) {
            let profile = profile_from_live_config(live);
            store.current_profile_id = Some(profile.id.clone());
            store.profiles.push(profile);
            save_store(app_handle, &store)?;
        }
    }

    sanitize_store(&mut store);
    save_store(app_handle, &store)?;
    Ok(store)
}

fn load_store(app_handle: &AppHandle) -> Result<CodexProfileStore, String> {
    let path = profile_store_path(app_handle)?;
    if !path.exists() {
        return Ok(CodexProfileStore::default());
    }

    let contents =
        fs::read_to_string(&path).map_err(|error| format!("读取 Codex 配置档案失败: {}", error))?;
    serde_json::from_str(strip_utf8_bom(&contents))
        .map_err(|error| format!("解析 Codex 配置档案失败: {}", error))
}

fn save_store(app_handle: &AppHandle, store: &CodexProfileStore) -> Result<(), String> {
    let path = profile_store_path(app_handle)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败: {}", error))?;
    }

    let contents = serde_json::to_string_pretty(store)
        .map_err(|error| format!("序列化 Codex 配置档案失败: {}", error))?;
    fs::write(&path, contents).map_err(|error| format!("写入 Codex 配置档案失败: {}", error))
}

fn profile_store_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app_handle
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("获取应用数据目录失败: {}", error))?;
    Ok(app_data_dir.join(CODEX_PROFILE_STORE_FILE))
}

fn profile_from_live_config(live: CodexLiveConfig) -> CodexConfigProfile {
    let now = now_rfc3339();
    CodexConfigProfile {
        id: Uuid::new_v4().to_string(),
        name: "当前 Codex 配置".to_string(),
        provider_id: non_empty_or(live.provider_id, DEFAULT_PROVIDER_ID),
        provider_name: non_empty_or(live.provider_name, DEFAULT_PROVIDER_NAME),
        api_key: live.api_key,
        base_url: non_empty_or(live.base_url, DEFAULT_BASE_URL),
        model: non_empty_or(live.model, DEFAULT_MODEL),
        reasoning_effort: non_empty_or(live.reasoning_effort, DEFAULT_REASONING_EFFORT),
        wire_api: non_empty_or(live.wire_api, DEFAULT_WIRE_API),
        requires_openai_auth: live.requires_openai_auth,
        disable_response_storage: live.disable_response_storage,
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn live_config_has_values(live: &CodexLiveConfig) -> bool {
    [
        live.provider_id.as_str(),
        live.provider_name.as_str(),
        live.api_key.as_str(),
        live.base_url.as_str(),
        live.model.as_str(),
        live.reasoning_effort.as_str(),
        live.wire_api.as_str(),
    ]
    .iter()
    .any(|value| !value.trim().is_empty())
}

fn normalize_profile_input(input: CodexConfigUpsertInput) -> Result<CodexConfigProfile, String> {
    let name = normalize_profile_name(&input.name);
    if name.is_empty() {
        return Err("配置名称不能为空".to_string());
    }

    let provider_id = sanitize_provider_id(&input.provider_id);
    let provider_name = non_empty_or(input.provider_name, &provider_id);
    let base_url = non_empty_or(input.base_url, DEFAULT_BASE_URL);
    let model = non_empty_or(input.model, DEFAULT_MODEL);
    let reasoning_effort = normalize_reasoning_effort(&input.reasoning_effort);
    let wire_api = normalize_wire_api(&input.wire_api);

    Ok(CodexConfigProfile {
        id: String::new(),
        name,
        provider_id,
        provider_name,
        api_key: input.api_key.trim().to_string(),
        base_url,
        model,
        reasoning_effort,
        wire_api,
        requires_openai_auth: input.requires_openai_auth,
        disable_response_storage: input.disable_response_storage,
        enabled: false,
        created_at: now_rfc3339(),
        updated_at: now_rfc3339(),
    })
}

fn sanitize_store(store: &mut CodexProfileStore) {
    for profile in &mut store.profiles {
        profile.name = normalize_profile_name(&profile.name);
        profile.provider_id = sanitize_provider_id(&profile.provider_id);
        profile.provider_name = non_empty_or(profile.provider_name.clone(), &profile.provider_id);
        profile.base_url = non_empty_or(profile.base_url.clone(), DEFAULT_BASE_URL);
        profile.model = non_empty_or(profile.model.clone(), DEFAULT_MODEL);
        profile.reasoning_effort = normalize_reasoning_effort(&profile.reasoning_effort);
        profile.wire_api = normalize_wire_api(&profile.wire_api);
    }

    let current_id = store.current_profile_id.clone();
    let current_exists = current_id
        .as_deref()
        .map(|id| store.profiles.iter().any(|profile| profile.id == id))
        .unwrap_or(false);
    if !current_exists {
        store.current_profile_id = store
            .profiles
            .iter()
            .find(|profile| profile.enabled)
            .map(|profile| profile.id.clone());
    }

    let desired_current_id = store.current_profile_id.clone();
    for profile in &mut store.profiles {
        profile.enabled = desired_current_id
            .as_deref()
            .map(|id| id == profile.id)
            .unwrap_or(false);
    }
}

fn find_profile<'a>(
    store: &'a CodexProfileStore,
    profile_id: &str,
) -> Option<&'a CodexConfigProfile> {
    store
        .profiles
        .iter()
        .find(|profile| profile.id == profile_id)
}

fn next_copy_name(store: &CodexProfileStore, source_name: &str) -> String {
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

fn load_live_config() -> Result<CodexLiveConfig, String> {
    let auth = load_json_dictionary(&codex_auth_path()?)?.unwrap_or_default();
    let api_key = auth
        .get("OPENAI_API_KEY")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let toml = fs::read_to_string(codex_config_path()?).unwrap_or_default();
    let provider_id = parse_top_level_string("model_provider", &toml).unwrap_or_default();

    Ok(CodexLiveConfig {
        provider_name: parse_section_string(
            "name",
            &format!("model_providers.{}", provider_id),
            &toml,
        )
        .unwrap_or_default(),
        base_url: parse_section_string(
            "base_url",
            &format!("model_providers.{}", provider_id),
            &toml,
        )
        .or_else(|| parse_top_level_string("base_url", &toml))
        .unwrap_or_default(),
        model: parse_top_level_string("model", &toml).unwrap_or_default(),
        reasoning_effort: parse_top_level_string("model_reasoning_effort", &toml)
            .unwrap_or_default(),
        wire_api: parse_section_string(
            "wire_api",
            &format!("model_providers.{}", provider_id),
            &toml,
        )
        .unwrap_or_default(),
        requires_openai_auth: parse_section_bool(
            "requires_openai_auth",
            &format!("model_providers.{}", provider_id),
            &toml,
        )
        .unwrap_or(true),
        disable_response_storage: parse_top_level_bool("disable_response_storage", &toml)
            .unwrap_or(true),
        provider_id,
        api_key,
    })
}

fn write_live_config(profile: &CodexConfigProfile) -> Result<(), String> {
    let mut auth = load_json_dictionary(&codex_auth_path()?)?.unwrap_or_default();
    auth.insert(
        "OPENAI_API_KEY".to_string(),
        Value::String(non_empty_or(profile.api_key.clone(), DEFAULT_API_KEY)),
    );
    auth.insert("auth_mode".to_string(), Value::String("apikey".to_string()));
    write_json_dictionary(&codex_auth_path()?, &auth)?;

    let config_path = codex_config_path()?;
    let existing_toml = fs::read_to_string(&config_path).unwrap_or_default();
    let updated_toml = update_codex_toml(
        &existing_toml,
        &profile.provider_id,
        &profile.provider_name,
        &profile.base_url,
        &profile.model,
        &profile.reasoning_effort,
        &profile.wire_api,
        profile.requires_openai_auth,
        profile.disable_response_storage,
    );
    write_text(&config_path, &updated_toml)
}

fn codex_auth_path() -> Result<PathBuf, String> {
    Ok(resolve_user_home_dir()?
        .join(CODEX_DIR)
        .join(CODEX_AUTH_FILE))
}

fn codex_config_path() -> Result<PathBuf, String> {
    Ok(resolve_user_home_dir()?
        .join(CODEX_DIR)
        .join(CODEX_CONFIG_FILE))
}

fn resolve_user_home_dir() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "无法定位用户目录".to_string())
}

fn load_json_dictionary(path: &Path) -> Result<Option<Map<String, Value>>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let contents =
        fs::read_to_string(path).map_err(|error| format!("读取 JSON 失败: {}", error))?;
    serde_json::from_str::<Map<String, Value>>(strip_utf8_bom(&contents))
        .map(Some)
        .map_err(|error| format!("解析 JSON 失败: {}", error))
}

fn write_json_dictionary(path: &Path, value: &Map<String, Value>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建 Codex 配置目录失败: {}", error))?;
    }
    let contents = serde_json::to_string_pretty(value)
        .map_err(|error| format!("序列化 JSON 失败: {}", error))?;
    fs::write(path, contents).map_err(|error| format!("写入 Codex 认证文件失败: {}", error))
}

fn write_text(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建 Codex 配置目录失败: {}", error))?;
    }
    fs::write(path, contents).map_err(|error| format!("写入 Codex 配置文件失败: {}", error))
}

fn update_codex_toml(
    existing: &str,
    provider_id: &str,
    provider_name: &str,
    base_url: &str,
    model: &str,
    reasoning_effort: &str,
    wire_api: &str,
    requires_openai_auth: bool,
    disable_response_storage: bool,
) -> String {
    let mut lines: Vec<String> = existing.split('\n').map(ToOwned::to_owned).collect();

    lines = set_top_level_string(lines, "model_provider", provider_id);
    lines = set_top_level_string(lines, "model", model);
    lines = set_top_level_string(lines, "model_reasoning_effort", reasoning_effort);
    lines = set_top_level_bool(lines, "disable_response_storage", disable_response_storage);

    let section = format!("model_providers.{}", provider_id);
    lines = set_section_string(lines, &section, "name", provider_name);
    lines = set_section_string(lines, &section, "base_url", base_url);
    lines = set_section_string(lines, &section, "wire_api", wire_api);
    lines = set_section_bool(
        lines,
        &section,
        "requires_openai_auth",
        requires_openai_auth,
    );

    format!("{}\n", lines.join("\n").trim())
}

fn parse_top_level_string(key: &str, toml: &str) -> Option<String> {
    parse_top_level_value(key, toml).and_then(|value| value.string_value())
}

fn parse_top_level_bool(key: &str, toml: &str) -> Option<bool> {
    parse_top_level_value(key, toml).and_then(|value| value.bool_value())
}

fn parse_section_string(key: &str, section: &str, toml: &str) -> Option<String> {
    parse_section_value(key, section, toml).and_then(|value| value.string_value())
}

fn parse_section_bool(key: &str, section: &str, toml: &str) -> Option<bool> {
    parse_section_value(key, section, toml).and_then(|value| value.bool_value())
}

fn parse_top_level_value(key: &str, toml: &str) -> Option<ParsedTomlValue> {
    for raw_line in toml.split('\n') {
        let trimmed = raw_line.trim();
        if trimmed.starts_with('[') {
            return None;
        }
        if let Some((assignment_key, value)) = parse_assignment(raw_line) {
            if assignment_key == key {
                return Some(ParsedTomlValue { raw: value });
            }
        }
    }
    None
}

fn parse_section_value(key: &str, section: &str, toml: &str) -> Option<ParsedTomlValue> {
    let mut in_section = false;
    let target_header = format!("[{}]", section);
    for raw_line in toml.split('\n') {
        let trimmed = raw_line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_section = trimmed == target_header;
            continue;
        }
        if in_section {
            if let Some((assignment_key, value)) = parse_assignment(raw_line) {
                if assignment_key == key {
                    return Some(ParsedTomlValue { raw: value });
                }
            }
        }
    }
    None
}

fn set_top_level_string(lines: Vec<String>, key: &str, value: &str) -> Vec<String> {
    set_top_level_line(lines, key, &format!("{} = \"{}\"", key, escape_toml(value)))
}

fn set_top_level_bool(lines: Vec<String>, key: &str, value: bool) -> Vec<String> {
    set_top_level_line(lines, key, &format!("{} = {}", key, value))
}

fn set_top_level_line(mut lines: Vec<String>, key: &str, line: &str) -> Vec<String> {
    let first_section_index = lines
        .iter()
        .position(|item| item.trim_start().starts_with('['))
        .unwrap_or(lines.len());
    for index in 0..first_section_index {
        if parse_assignment(&lines[index])
            .map(|(assignment_key, _)| assignment_key == key)
            .unwrap_or(false)
        {
            lines[index] = line.to_string();
            return lines;
        }
    }
    lines.insert(first_section_index, line.to_string());
    lines
}

fn set_section_string(lines: Vec<String>, section: &str, key: &str, value: &str) -> Vec<String> {
    set_section_line(
        lines,
        section,
        key,
        &format!("{} = \"{}\"", key, escape_toml(value)),
    )
}

fn set_section_bool(lines: Vec<String>, section: &str, key: &str, value: bool) -> Vec<String> {
    set_section_line(lines, section, key, &format!("{} = {}", key, value))
}

fn set_section_line(mut lines: Vec<String>, section: &str, key: &str, line: &str) -> Vec<String> {
    let header = format!("[{}]", section);
    let mut section_start = None;
    let mut section_end = lines.len();

    for (index, raw_line) in lines.iter().enumerate() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if section_start.is_some() {
                section_end = index;
                break;
            }
            if trimmed == header {
                section_start = Some(index);
            }
        }
    }

    let Some(start) = section_start else {
        if lines.last().map(|line| !line.is_empty()).unwrap_or(false) {
            lines.push(String::new());
        }
        lines.push(header);
        lines.push(line.to_string());
        return lines;
    };

    for index in (start + 1)..section_end {
        if parse_assignment(&lines[index])
            .map(|(assignment_key, _)| assignment_key == key)
            .unwrap_or(false)
        {
            lines[index] = line.to_string();
            return lines;
        }
    }
    lines.insert(section_end, line.to_string());
    lines
}

fn parse_assignment(line: &str) -> Option<(String, String)> {
    let stripped = strip_inline_comment(line).trim().to_string();
    if stripped.is_empty() || stripped.starts_with('#') {
        return None;
    }
    let equals = stripped.find('=')?;
    let key = stripped[..equals].trim();
    let value = stripped[(equals + 1)..].trim();
    if key.is_empty() {
        return None;
    }
    Some((key.to_string(), value.to_string()))
}

fn strip_inline_comment(line: &str) -> String {
    let mut in_string = false;
    let mut escaped = false;
    let mut result = String::new();

    for character in line.chars() {
        if character == '\\' && in_string {
            escaped = !escaped;
            result.push(character);
            continue;
        }
        if character == '"' && !escaped {
            in_string = !in_string;
        }
        escaped = false;
        if character == '#' && !in_string {
            break;
        }
        result.push(character);
    }

    result
}

fn escape_toml(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[derive(Debug, Clone)]
struct ParsedTomlValue {
    raw: String,
}

impl ParsedTomlValue {
    fn string_value(&self) -> Option<String> {
        let trimmed = self.raw.trim();
        if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
            return Some(
                trimmed[1..trimmed.len() - 1]
                    .replace("\\\"", "\"")
                    .replace("\\\\", "\\"),
            );
        }
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    fn bool_value(&self) -> Option<bool> {
        match self.raw.trim().to_ascii_lowercase().as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }
}

fn normalize_profile_name(value: &str) -> String {
    value.trim().to_string()
}

fn sanitize_provider_id(value: &str) -> String {
    let trimmed = value.trim().to_ascii_lowercase();
    let mut sanitized = String::with_capacity(trimmed.len());
    let mut previous_dash = false;
    for character in trimmed.chars() {
        let next = if character.is_ascii_alphanumeric() || character == '_' {
            previous_dash = false;
            character
        } else {
            if previous_dash {
                continue;
            }
            previous_dash = true;
            '-'
        };
        sanitized.push(next);
    }
    let sanitized = sanitized.trim_matches('-').to_string();
    if sanitized.is_empty() {
        DEFAULT_PROVIDER_ID.to_string()
    } else {
        sanitized
    }
}

fn normalize_reasoning_effort(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "minimal" | "low" | "medium" | "high" | "xhigh" => value.trim().to_ascii_lowercase(),
        _ => DEFAULT_REASONING_EFFORT.to_string(),
    }
}

fn normalize_wire_api(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "responses" => "responses".to_string(),
        _ => DEFAULT_WIRE_API.to_string(),
    }
}

fn non_empty_or(value: String, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn strip_utf8_bom(contents: &str) -> &str {
    contents.strip_prefix('\u{feff}').unwrap_or(contents)
}

fn default_true() -> bool {
    true
}

fn default_provider_id() -> String {
    DEFAULT_PROVIDER_ID.to_string()
}

fn default_provider_name() -> String {
    DEFAULT_PROVIDER_NAME.to_string()
}

fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_string()
}

fn default_model() -> String {
    DEFAULT_MODEL.to_string()
}

fn default_reasoning_effort() -> String {
    DEFAULT_REASONING_EFFORT.to_string()
}

fn default_wire_api() -> String {
    DEFAULT_WIRE_API.to_string()
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_codex_toml_inserts_and_updates_expected_values() {
        let existing = r#"model = "old"

[model_providers.old]
name = "Old"
base_url = "https://old.example.com"
"#;

        let updated = update_codex_toml(
            existing,
            "toapiproxy",
            "ToapiProxy",
            "http://127.0.0.1:8317/v1",
            "gpt-5.4",
            "xhigh",
            "responses",
            true,
            true,
        );

        assert_eq!(
            parse_top_level_string("model_provider", &updated).as_deref(),
            Some("toapiproxy")
        );
        assert_eq!(
            parse_top_level_string("model", &updated).as_deref(),
            Some("gpt-5.4")
        );
        assert_eq!(
            parse_section_string("base_url", "model_providers.toapiproxy", &updated).as_deref(),
            Some("http://127.0.0.1:8317/v1")
        );
        assert_eq!(
            parse_section_bool(
                "requires_openai_auth",
                "model_providers.toapiproxy",
                &updated
            ),
            Some(true)
        );
    }

    #[test]
    fn sanitize_provider_id_keeps_codex_safe_identifier() {
        assert_eq!(sanitize_provider_id(" ToAPI Proxy!! "), "toapi-proxy");
        assert_eq!(sanitize_provider_id(""), "toapiproxy");
    }
}
