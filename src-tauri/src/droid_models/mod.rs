use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::env;
use std::fs;
use std::path::PathBuf;

const FACTORY_SETTINGS_DIR: &str = ".factory";
const FACTORY_SETTINGS_FILE: &str = "settings.json";

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8317";
const DEFAULT_API_KEY: &str = "dummy-not-used";
const DEFAULT_PROVIDER: &str = "anthropic";
const ALLOWED_PROVIDERS: &[&str] = &[
    "anthropic",
    "openai",
    "generic-chat-completion-api",
];

#[derive(Debug, Clone, Deserialize)]
pub struct DroidCustomModelUpsertInput {
    pub model: String,
    pub id: String,
    pub index: i64,
    #[serde(rename = "baseUrl", default)]
    pub base_url: String,
    #[serde(rename = "apiKey", default)]
    pub api_key: String,
    #[serde(rename = "displayName", default)]
    pub display_name: String,
    #[serde(rename = "noImageSupport", default)]
    pub no_image_support: bool,
    #[serde(default)]
    pub provider: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DroidCustomModelSummary {
    pub model: String,
    pub id: String,
    pub index: i64,
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    #[serde(rename = "apiKey")]
    pub api_key: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "noImageSupport")]
    pub no_image_support: bool,
    pub provider: String,
    #[serde(rename = "isCurrent")]
    pub is_current: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct DroidCustomModel {
    #[serde(default)]
    model: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    index: i64,
    #[serde(rename = "baseUrl", default = "default_base_url")]
    base_url: String,
    #[serde(rename = "apiKey", default = "default_api_key")]
    api_key: String,
    #[serde(rename = "displayName", default)]
    display_name: String,
    #[serde(rename = "noImageSupport", default)]
    no_image_support: bool,
    #[serde(default = "default_provider")]
    provider: String,
}

pub fn list_droid_custom_models() -> Result<Vec<DroidCustomModelSummary>, String> {
    let settings = load_factory_settings_value()?;
    let current_model_id = current_session_model_id(&settings);
    let mut models = read_custom_models(&settings)?;
    sort_models(&mut models);

    Ok(models
        .into_iter()
        .map(|item| DroidCustomModelSummary {
            model: item.model.clone(),
            id: item.id.clone(),
            index: item.index,
            base_url: item.base_url.clone(),
            api_key: item.api_key.clone(),
            display_name: item.display_name.clone(),
            no_image_support: item.no_image_support,
            provider: item.provider.clone(),
            is_current: current_model_id.as_deref() == Some(item.id.as_str()),
        })
        .collect())
}

pub fn upsert_droid_custom_model(
    input: DroidCustomModelUpsertInput,
    original_id: Option<String>,
) -> Result<String, String> {
    let settings_path = factory_settings_path()?;
    let mut settings = load_factory_settings_value()?;
    let mut models = read_custom_models(&settings)?;
    let normalized = normalize_custom_model_input(input)?;
    let original_id = original_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    if models
        .iter()
        .any(|item| item.id == normalized.id && Some(item.id.as_str()) != original_id.as_deref())
    {
        return Err(format!(
            "已存在相同 ID 的 Droid 自定义模型：{}",
            normalized.id
        ));
    }

    let existing_index = original_id
        .as_deref()
        .and_then(|target_id| models.iter().position(|item| item.id == target_id));

    let action = if let Some(index) = existing_index {
        let previous_id = models[index].id.clone();
        models[index] = normalized.clone();
        if current_session_model_id(&settings).as_deref() == Some(previous_id.as_str()) {
            set_session_model_id(&mut settings, normalized.id.clone())?;
        }
        "更新"
    } else {
        models.push(normalized.clone());
        "创建"
    };

    sort_models(&mut models);
    write_custom_models(&mut settings, &models)?;
    save_factory_settings_value(&settings)?;
    let reloaded = load_factory_settings_value()?;
    let reloaded_models = read_custom_models(&reloaded)?;
    let saved = reloaded_models
        .iter()
        .find(|item| item.id == normalized.id)
        .ok_or_else(|| {
            format!(
                "Droid 自定义模型保存后校验失败：{} 未出现在 {}",
                normalized.display_name,
                settings_path.display()
            )
        })?;

    if saved.display_name != normalized.display_name
        || saved.model != normalized.model
        || saved.base_url != normalized.base_url
        || saved.provider != normalized.provider
    {
        return Err(format!(
            "Droid 自定义模型保存后校验失败：{} 在 {} 中的内容与提交值不一致",
            normalized.display_name,
            settings_path.display()
        ));
    }

    Ok(format!(
        "Droid 自定义模型已{}：{}",
        action, normalized.display_name
    ))
}

pub fn delete_droid_custom_model(model_id: &str) -> Result<String, String> {
    let settings_path = factory_settings_path()?;
    let mut settings = load_factory_settings_value()?;
    let mut models = read_custom_models(&settings)?;
    let index = models
        .iter()
        .position(|item| item.id == model_id)
        .ok_or_else(|| "未找到指定的 Droid 自定义模型".to_string())?;

    let removed = models.remove(index);
    if current_session_model_id(&settings).as_deref() == Some(removed.id.as_str()) {
        set_session_model_id(&mut settings, String::new())?;
    }

    write_custom_models(&mut settings, &models)?;
    save_factory_settings_value(&settings)?;
    let reloaded = load_factory_settings_value()?;
    let reloaded_models = read_custom_models(&reloaded)?;
    if reloaded_models.iter().any(|item| item.id == removed.id) {
        return Err(format!(
            "Droid 自定义模型删除后校验失败：{} 仍然存在于 {}",
            removed.display_name,
            settings_path.display()
        ));
    }

    Ok(format!("Droid 自定义模型已删除：{}", removed.display_name))
}

pub fn duplicate_droid_custom_model(model_id: &str) -> Result<String, String> {
    let settings_path = factory_settings_path()?;
    let mut settings = load_factory_settings_value()?;
    let mut models = read_custom_models(&settings)?;
    let source = models
        .iter()
        .find(|item| item.id == model_id)
        .cloned()
        .ok_or_else(|| "未找到要复制的 Droid 自定义模型".to_string())?;

    let duplicated = DroidCustomModel {
        display_name: next_copy_display_name(&models, &source.display_name),
        id: next_copy_id(&models, &source.id),
        index: next_available_index(&models),
        ..source
    };

    let duplicate_id = duplicated.id.clone();
    let display_name = duplicated.display_name.clone();
    models.push(duplicated);
    sort_models(&mut models);
    write_custom_models(&mut settings, &models)?;
    save_factory_settings_value(&settings)?;
    let reloaded = load_factory_settings_value()?;
    let reloaded_models = read_custom_models(&reloaded)?;
    if !reloaded_models.iter().any(|item| item.id == duplicate_id) {
        return Err(format!(
            "Droid 自定义模型复制后校验失败：{} 未出现在 {}",
            display_name,
            settings_path.display()
        ));
    }

    Ok(format!("Droid 自定义模型已复制：{}", display_name))
}

pub fn set_droid_default_model(model_id: &str) -> Result<String, String> {
    let settings_path = factory_settings_path()?;
    let mut settings = load_factory_settings_value()?;
    let models = read_custom_models(&settings)?;
    let target = models
        .iter()
        .find(|item| item.id == model_id)
        .ok_or_else(|| "未找到要设置为默认的 Droid 自定义模型".to_string())?;

    set_session_model_id(&mut settings, target.id.clone())?;
    save_factory_settings_value(&settings)?;

    let reloaded = load_factory_settings_value()?;
    if current_session_model_id(&reloaded).as_deref() != Some(target.id.as_str()) {
        return Err(format!(
            "默认模型写入校验失败：{} 未生效到 {}",
            target.display_name,
            settings_path.display()
        ));
    }

    Ok(format!("已将默认模型设置为：{}", target.display_name))
}

fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_string()
}

fn default_api_key() -> String {
    DEFAULT_API_KEY.to_string()
}

fn default_provider() -> String {
    DEFAULT_PROVIDER.to_string()
}

fn normalize_custom_model_input(input: DroidCustomModelUpsertInput) -> Result<DroidCustomModel, String> {
    let model = input.model.trim().to_string();
    let id = input.id.trim().to_string();
    let display_name = if input.display_name.trim().is_empty() {
        model.clone()
    } else {
        input.display_name.trim().to_string()
    };
    let base_url = if input.base_url.trim().is_empty() {
        DEFAULT_BASE_URL.to_string()
    } else {
        input.base_url.trim().to_string()
    };
    let api_key = if input.api_key.trim().is_empty() {
        DEFAULT_API_KEY.to_string()
    } else {
        input.api_key.trim().to_string()
    };
    let provider = if input.provider.trim().is_empty() {
        DEFAULT_PROVIDER.to_string()
    } else {
        input.provider.trim().to_ascii_lowercase()
    };

    if model.is_empty() {
        return Err("模型名不能为空".to_string());
    }
    if id.is_empty() {
        return Err("模型 ID 不能为空".to_string());
    }
    if input.index < 0 {
        return Err("索引必须大于等于 0".to_string());
    }
    if base_url.is_empty() {
        return Err("Base URL 不能为空".to_string());
    }
    if provider.is_empty() {
        return Err("Provider 不能为空".to_string());
    }
    if !ALLOWED_PROVIDERS.contains(&provider.as_str()) {
        return Err(
            "Provider 只支持 anthropic、openai、generic-chat-completion-api".to_string(),
        );
    }

    Ok(DroidCustomModel {
        model,
        id,
        index: input.index,
        base_url,
        api_key,
        display_name,
        no_image_support: input.no_image_support,
        provider,
    })
}

fn next_available_index(models: &[DroidCustomModel]) -> i64 {
    models.iter().map(|item| item.index).max().unwrap_or(-1) + 1
}

fn next_copy_display_name(models: &[DroidCustomModel], source_name: &str) -> String {
    let base_name = format!("{} 副本", source_name);
    if !models.iter().any(|item| item.display_name == base_name) {
        return base_name;
    }

    let mut suffix = 2;
    loop {
        let candidate = format!("{} {}", base_name, suffix);
        if !models.iter().any(|item| item.display_name == candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

fn next_copy_id(models: &[DroidCustomModel], source_id: &str) -> String {
    let base_id = format!("{}-copy", source_id);
    if !models.iter().any(|item| item.id == base_id) {
        return base_id;
    }

    let mut suffix = 2;
    loop {
        let candidate = format!("{}-{}", base_id, suffix);
        if !models.iter().any(|item| item.id == candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

fn sort_models(models: &mut [DroidCustomModel]) {
    models.sort_by(|left, right| {
        left.index
            .cmp(&right.index)
            .then_with(|| left.display_name.cmp(&right.display_name))
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn load_factory_settings_value() -> Result<Value, String> {
    let path = factory_settings_path()?;
    if !path.exists() {
        return Ok(Value::Object(Map::new()));
    }

    let raw = fs::read_to_string(&path)
        .map_err(|error| format!("读取 {:?} 失败：{}", path, error))?;
    if raw.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }

    let value: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("解析 {:?} 失败：{}", path, error))?;
    if !value.is_object() {
        return Err(format!("{:?} 的内容不是合法的 JSON 对象", path));
    }

    Ok(value)
}

fn save_factory_settings_value(settings: &Value) -> Result<(), String> {
    let path = factory_settings_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| "无法定位 .factory 配置目录".to_string())?;

    fs::create_dir_all(parent)
        .map_err(|error| format!("创建 {:?} 失败：{}", parent, error))?;

    let serialized = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("序列化 settings.json 失败：{}", error))?;
    fs::write(&path, format!("{}\n", serialized))
        .map_err(|error| format!("写入 {:?} 失败：{}", path, error))?;

    Ok(())
}

fn factory_settings_path() -> Result<PathBuf, String> {
    let home = resolve_user_home_dir()?;
    Ok(home.join(FACTORY_SETTINGS_DIR).join(FACTORY_SETTINGS_FILE))
}

fn resolve_user_home_dir() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("USERPROFILE").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }

    if let Some(path) = env::var_os("HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }

    dirs::home_dir().ok_or_else(|| "无法定位用户目录".to_string())
}

fn read_custom_models(settings: &Value) -> Result<Vec<DroidCustomModel>, String> {
    match settings.get("customModels") {
        Some(value) if !value.is_null() => serde_json::from_value(value.clone())
            .map_err(|error| format!("解析 customModels 失败：{}", error)),
        _ => Ok(Vec::new()),
    }
}

fn write_custom_models(settings: &mut Value, models: &[DroidCustomModel]) -> Result<(), String> {
    let root = settings
        .as_object_mut()
        .ok_or_else(|| "settings.json 根节点不是对象".to_string())?;

    root.insert(
        "customModels".to_string(),
        serde_json::to_value(models).map_err(|error| format!("序列化 customModels 失败：{}", error))?,
    );

    Ok(())
}

fn current_session_model_id(settings: &Value) -> Option<String> {
    settings
        .get("sessionDefaultSettings")
        .and_then(Value::as_object)
        .and_then(|item| item.get("model"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn set_session_model_id(settings: &mut Value, model_id: String) -> Result<(), String> {
    let root = settings
        .as_object_mut()
        .ok_or_else(|| "settings.json 根节点不是对象".to_string())?;

    let session = root
        .entry("sessionDefaultSettings".to_string())
        .or_insert_with(|| Value::Object(Map::new()));

    if !session.is_object() {
        *session = Value::Object(Map::new());
    }

    let session_object = session
        .as_object_mut()
        .ok_or_else(|| "sessionDefaultSettings 不是对象".to_string())?;
    session_object.insert("model".to_string(), Value::String(model_id));

    Ok(())
}
