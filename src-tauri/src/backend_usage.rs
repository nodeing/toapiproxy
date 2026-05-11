use chrono::{DateTime, Local, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use crate::management::MANAGEMENT_SECRET;

const QUEUE_BATCH_SIZE: usize = 5000;
const STORE_DIR_NAME: &str = "usage-statistics";
const STORE_FILE_NAME: &str = "backend-usage.json";

#[derive(Debug, Clone, Serialize)]
pub struct BackendUsageSnapshot {
    pub usage: UsageSnapshot,
    #[serde(rename = "failed_requests")]
    pub failed_requests: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct UsageSnapshot {
    #[serde(rename = "total_requests")]
    pub total_requests: i64,
    #[serde(rename = "success_count")]
    pub success_count: i64,
    #[serde(rename = "failure_count")]
    pub failure_count: i64,
    #[serde(rename = "input_tokens")]
    pub input_tokens: i64,
    #[serde(rename = "output_tokens")]
    pub output_tokens: i64,
    #[serde(rename = "reasoning_tokens")]
    pub reasoning_tokens: i64,
    #[serde(rename = "cached_tokens")]
    pub cached_tokens: i64,
    #[serde(rename = "total_tokens")]
    pub total_tokens: i64,
    pub apis: BTreeMap<String, APIUsage>,
    pub sources: BTreeMap<String, SourceUsage>,
    #[serde(rename = "requests_by_day")]
    pub requests_by_day: BTreeMap<String, i64>,
    #[serde(rename = "requests_by_hour")]
    pub requests_by_hour: BTreeMap<String, i64>,
    #[serde(rename = "tokens_by_day")]
    pub tokens_by_day: BTreeMap<String, i64>,
    #[serde(rename = "tokens_by_hour")]
    pub tokens_by_hour: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct APIUsage {
    #[serde(rename = "total_requests")]
    pub total_requests: i64,
    #[serde(rename = "success_count")]
    pub success_count: i64,
    #[serde(rename = "failure_count")]
    pub failure_count: i64,
    #[serde(rename = "input_tokens")]
    pub input_tokens: i64,
    #[serde(rename = "output_tokens")]
    pub output_tokens: i64,
    #[serde(rename = "reasoning_tokens")]
    pub reasoning_tokens: i64,
    #[serde(rename = "cached_tokens")]
    pub cached_tokens: i64,
    #[serde(rename = "total_tokens")]
    pub total_tokens: i64,
    pub models: BTreeMap<String, ModelUsage>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ModelUsage {
    #[serde(rename = "total_requests")]
    pub total_requests: i64,
    #[serde(rename = "success_count")]
    pub success_count: i64,
    #[serde(rename = "failure_count")]
    pub failure_count: i64,
    #[serde(rename = "input_tokens")]
    pub input_tokens: i64,
    #[serde(rename = "output_tokens")]
    pub output_tokens: i64,
    #[serde(rename = "reasoning_tokens")]
    pub reasoning_tokens: i64,
    #[serde(rename = "cached_tokens")]
    pub cached_tokens: i64,
    #[serde(rename = "total_tokens")]
    pub total_tokens: i64,
    pub details: Vec<RequestDetail>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SourceUsage {
    pub name: String,
    pub provider: String,
    #[serde(rename = "auth_index")]
    pub auth_index: String,
    #[serde(rename = "total_requests")]
    pub total_requests: i64,
    #[serde(rename = "success_count")]
    pub success_count: i64,
    #[serde(rename = "failure_count")]
    pub failure_count: i64,
    #[serde(rename = "input_tokens")]
    pub input_tokens: i64,
    #[serde(rename = "output_tokens")]
    pub output_tokens: i64,
    #[serde(rename = "reasoning_tokens")]
    pub reasoning_tokens: i64,
    #[serde(rename = "cached_tokens")]
    pub cached_tokens: i64,
    #[serde(rename = "total_tokens")]
    pub total_tokens: i64,
    pub models: BTreeMap<String, SourceModelUsage>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SourceModelUsage {
    #[serde(rename = "total_requests")]
    pub total_requests: i64,
    #[serde(rename = "success_count")]
    pub success_count: i64,
    #[serde(rename = "failure_count")]
    pub failure_count: i64,
    #[serde(rename = "input_tokens")]
    pub input_tokens: i64,
    #[serde(rename = "output_tokens")]
    pub output_tokens: i64,
    #[serde(rename = "reasoning_tokens")]
    pub reasoning_tokens: i64,
    #[serde(rename = "cached_tokens")]
    pub cached_tokens: i64,
    #[serde(rename = "total_tokens")]
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestDetail {
    pub timestamp: String,
    #[serde(
        rename = "latency_ms",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub latency_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(rename = "auth_type", default, skip_serializing_if = "Option::is_none")]
    pub auth_type: Option<String>,
    #[serde(rename = "api_key", default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(
        rename = "request_id",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub request_id: Option<String>,
    #[serde(default)]
    pub source: String,
    #[serde(rename = "auth_index", default)]
    pub auth_index: String,
    #[serde(default)]
    pub tokens: TokenUsage,
    #[serde(default)]
    pub failed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fail: Option<FailDetail>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(rename = "input_tokens", default)]
    pub input_tokens: i64,
    #[serde(rename = "output_tokens", default)]
    pub output_tokens: i64,
    #[serde(rename = "reasoning_tokens", default)]
    pub reasoning_tokens: i64,
    #[serde(rename = "cached_tokens", default)]
    pub cached_tokens: i64,
    #[serde(rename = "total_tokens", default)]
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FailDetail {
    #[serde(
        rename = "status_code",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub status_code: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct BackendUsageQueueRecord {
    #[serde(default)]
    timestamp: String,
    #[serde(
        rename = "latency_ms",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    latency_ms: Option<i64>,
    #[serde(default)]
    source: String,
    #[serde(rename = "auth_index", default)]
    auth_index: String,
    #[serde(default)]
    tokens: TokenUsage,
    #[serde(default)]
    failed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fail: Option<FailDetail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    endpoint: Option<String>,
    #[serde(rename = "auth_type", default, skip_serializing_if = "Option::is_none")]
    auth_type: Option<String>,
    #[serde(rename = "api_key", default, skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
    #[serde(
        rename = "request_id",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedBackendUsageStore {
    version: i32,
    #[serde(rename = "exported_at")]
    exported_at: String,
    records: Vec<BackendUsageQueueRecord>,
}

impl Default for PersistedBackendUsageStore {
    fn default() -> Self {
        Self {
            version: 1,
            exported_at: now_rfc3339(),
            records: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendUsageTimeRange {
    Today,
    SevenDays,
    ThirtyDays,
    All,
}

impl BackendUsageTimeRange {
    fn parse(value: Option<&str>) -> Self {
        match value.unwrap_or("all").trim().to_ascii_lowercase().as_str() {
            "today" => Self::Today,
            "7d" | "seven_days" | "sevendays" | "seven-days" => Self::SevenDays,
            "30d" | "thirty_days" | "thirtydays" | "thirty-days" => Self::ThirtyDays,
            _ => Self::All,
        }
    }

    fn contains(self, timestamp: DateTime<Utc>, now: DateTime<Utc>) -> bool {
        if self == Self::All {
            return true;
        }

        let date = timestamp.with_timezone(&Local).date_naive();
        let today = now.with_timezone(&Local).date_naive();
        let days = today.signed_duration_since(date).num_days();
        if days < 0 {
            return false;
        }

        match self {
            Self::Today => days == 0,
            Self::SevenDays => days <= 6,
            Self::ThirtyDays => days <= 29,
            Self::All => true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct UsageTotals {
    total_requests: i64,
    success_count: i64,
    failure_count: i64,
    input_tokens: i64,
    output_tokens: i64,
    reasoning_tokens: i64,
    cached_tokens: i64,
    total_tokens: i64,
}

impl UsageTotals {
    fn add(&mut self, record: &BackendUsageQueueRecord) {
        let tokens = record.normalized_tokens();
        self.total_requests += 1;
        if record.failed {
            self.failure_count += 1;
        } else {
            self.success_count += 1;
        }
        self.input_tokens += tokens.input_tokens;
        self.output_tokens += tokens.output_tokens;
        self.reasoning_tokens += tokens.reasoning_tokens;
        self.cached_tokens += tokens.cached_tokens;
        self.total_tokens += tokens.total_tokens;
    }
}

#[derive(Default)]
struct ModelUsageAccumulator {
    totals: UsageTotals,
    details: Vec<RequestDetail>,
}

impl ModelUsageAccumulator {
    fn add(&mut self, record: &BackendUsageQueueRecord) {
        self.totals.add(record);
        self.details.push(record.request_detail());
    }
}

#[derive(Default)]
struct APIUsageAccumulator {
    totals: UsageTotals,
    models: HashMap<String, ModelUsageAccumulator>,
}

impl APIUsageAccumulator {
    fn add(&mut self, record: &BackendUsageQueueRecord) {
        self.totals.add(record);
        self.models
            .entry(usage_model_name(record))
            .or_default()
            .add(record);
    }
}

struct SourceUsageAccumulator {
    name: String,
    provider: String,
    auth_index: String,
    totals: UsageTotals,
    models: HashMap<String, UsageTotals>,
}

impl SourceUsageAccumulator {
    fn new(record: &BackendUsageQueueRecord) -> Self {
        Self {
            name: normalized_name(Some(&record.source), "unknown source"),
            provider: normalized_name(record.provider.as_deref(), "unknown provider"),
            auth_index: normalized_name(Some(&record.auth_index), ""),
            totals: UsageTotals::default(),
            models: HashMap::new(),
        }
    }

    fn add(&mut self, record: &BackendUsageQueueRecord) {
        self.totals.add(record);
        let mut totals = self
            .models
            .get(&usage_model_name(record))
            .copied()
            .unwrap_or_default();
        totals.add(record);
        self.models.insert(usage_model_name(record), totals);
    }
}

impl BackendUsageQueueRecord {
    fn normalized_tokens(&self) -> TokenUsage {
        let mut tokens = self.tokens;
        if tokens.total_tokens <= 0 {
            tokens.total_tokens =
                tokens.input_tokens + tokens.output_tokens + tokens.reasoning_tokens;
        }
        if tokens.total_tokens <= 0 {
            tokens.total_tokens = tokens.input_tokens
                + tokens.output_tokens
                + tokens.reasoning_tokens
                + tokens.cached_tokens;
        }
        tokens
    }

    fn request_detail(&self) -> RequestDetail {
        RequestDetail {
            timestamp: self.timestamp.clone(),
            latency_ms: self.latency_ms,
            provider: self.provider.clone(),
            model: self.model.clone(),
            alias: self.alias.clone(),
            endpoint: self.endpoint.clone(),
            auth_type: self.auth_type.clone(),
            api_key: self.api_key.clone(),
            request_id: self.request_id.clone(),
            source: self.source.clone(),
            auth_index: self.auth_index.clone(),
            tokens: self.normalized_tokens(),
            failed: self.failed,
            fail: self.fail.clone(),
        }
    }

    fn dedupe_key(&self) -> String {
        if let Some(request_id) = normalized_optional(self.request_id.as_deref()) {
            return format!("request:{}", request_id);
        }

        let tokens = self.normalized_tokens();
        [
            "record".to_string(),
            self.timestamp.clone(),
            self.provider.clone().unwrap_or_default(),
            self.model.clone().unwrap_or_default(),
            self.alias.clone().unwrap_or_default(),
            self.endpoint.clone().unwrap_or_default(),
            self.source.clone(),
            self.auth_index.clone(),
            tokens.input_tokens.to_string(),
            tokens.output_tokens.to_string(),
            tokens.reasoning_tokens.to_string(),
            tokens.cached_tokens.to_string(),
            tokens.total_tokens.to_string(),
            self.failed.to_string(),
            self.latency_ms.unwrap_or(-1).to_string(),
        ]
        .join("|")
    }

    fn normalize(mut self) -> Self {
        if parse_usage_date(&self.timestamp).is_none() {
            self.timestamp = now_rfc3339();
        }
        self.tokens = self.normalized_tokens();
        self.source = self.source.trim().to_string();
        self.auth_index = self.auth_index.trim().to_string();
        self.provider = normalized_optional(self.provider.as_deref());
        self.model = normalized_optional(self.model.as_deref());
        self.alias = normalized_optional(self.alias.as_deref());
        self.endpoint = normalized_optional(self.endpoint.as_deref());
        self.auth_type = normalized_optional(self.auth_type.as_deref());
        self.api_key = normalized_optional(self.api_key.as_deref());
        self.request_id = normalized_optional(self.request_id.as_deref());
        self
    }
}

pub async fn fetch_backend_usage_statistics(
    backend_port: u16,
    app_data_dir: &Path,
    time_range: Option<&str>,
    drain_queue: bool,
) -> Result<BackendUsageSnapshot, String> {
    let range = BackendUsageTimeRange::parse(time_range);
    let store_path = usage_store_path(app_data_dir);
    let mut store = load_usage_store(&store_path)?;

    if drain_queue {
        match drain_backend_usage_queue(backend_port).await {
            Ok(records) => {
                if !records.is_empty() {
                    store = merge_usage_store(store, records);
                    save_usage_store(&store_path, &store)?;
                }
            }
            Err(error) => {
                log::warn!("Failed to drain backend usage queue: {}", error);
            }
        }
    }

    Ok(make_usage_snapshot(&store, range))
}

async fn drain_backend_usage_queue(
    backend_port: u16,
) -> Result<Vec<BackendUsageQueueRecord>, String> {
    let url = format!(
        "http://127.0.0.1:{}/v0/management/usage-queue?count={}",
        backend_port, QUEUE_BATCH_SIZE
    );
    let response = Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .build()
        .map_err(|error| format!("Failed to build usage client: {}", error))?
        .get(url)
        .header("Authorization", format!("Bearer {}", MANAGEMENT_SECRET))
        .send()
        .await
        .map_err(|error| format!("Failed to fetch usage queue: {}", error))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Usage queue request failed with status {}{}",
            status,
            if body.is_empty() {
                String::new()
            } else {
                format!(": {}", body)
            }
        ));
    }

    response
        .json::<Vec<BackendUsageQueueRecord>>()
        .await
        .map(|records| {
            records
                .into_iter()
                .map(BackendUsageQueueRecord::normalize)
                .collect()
        })
        .map_err(|error| format!("Failed to parse usage queue response: {}", error))
}

fn usage_store_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(STORE_DIR_NAME).join(STORE_FILE_NAME)
}

fn load_usage_store(path: &Path) -> Result<PersistedBackendUsageStore, String> {
    if !path.exists() {
        return Ok(PersistedBackendUsageStore::default());
    }

    let data = fs::read(path).map_err(|error| {
        format!(
            "Failed to read usage statistics store {}: {}",
            path.display(),
            error
        )
    })?;

    let mut store: PersistedBackendUsageStore = serde_json::from_slice(&data).map_err(|error| {
        format!(
            "Failed to parse usage statistics store {}: {}",
            path.display(),
            error
        )
    })?;
    store.records = store
        .records
        .into_iter()
        .map(BackendUsageQueueRecord::normalize)
        .collect();
    Ok(store)
}

fn save_usage_store(path: &Path, store: &PersistedBackendUsageStore) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create usage statistics directory {}: {}",
                parent.display(),
                error
            )
        })?;
    }

    let data = serde_json::to_vec_pretty(store)
        .map_err(|error| format!("Failed to encode usage statistics store: {}", error))?;
    fs::write(path, data).map_err(|error| {
        format!(
            "Failed to write usage statistics store {}: {}",
            path.display(),
            error
        )
    })
}

fn merge_usage_store(
    existing: PersistedBackendUsageStore,
    incoming_records: Vec<BackendUsageQueueRecord>,
) -> PersistedBackendUsageStore {
    let mut records_by_key: HashMap<String, BackendUsageQueueRecord> = HashMap::new();

    for record in existing.records.into_iter().chain(incoming_records) {
        records_by_key.insert(record.dedupe_key(), record);
    }

    let mut records: Vec<BackendUsageQueueRecord> = records_by_key.into_values().collect();
    records.sort_by(|left, right| {
        record_timestamp(left)
            .cmp(&record_timestamp(right))
            .then_with(|| left.dedupe_key().cmp(&right.dedupe_key()))
    });

    PersistedBackendUsageStore {
        version: 1,
        exported_at: now_rfc3339(),
        records,
    }
}

fn make_usage_snapshot(
    store: &PersistedBackendUsageStore,
    time_range: BackendUsageTimeRange,
) -> BackendUsageSnapshot {
    let now = Utc::now();
    let records: Vec<&BackendUsageQueueRecord> = store
        .records
        .iter()
        .filter(|record| time_range.contains(record_timestamp(record), now))
        .collect();

    let usage = aggregate_usage(&records);
    BackendUsageSnapshot {
        failed_requests: usage.failure_count,
        usage,
    }
}

fn aggregate_usage(records: &[&BackendUsageQueueRecord]) -> UsageSnapshot {
    let mut totals = UsageTotals::default();
    let mut apis: HashMap<String, APIUsageAccumulator> = HashMap::new();
    let mut sources: HashMap<String, SourceUsageAccumulator> = HashMap::new();
    let mut requests_by_day: BTreeMap<String, i64> = BTreeMap::new();
    let mut requests_by_hour: BTreeMap<String, i64> = BTreeMap::new();
    let mut tokens_by_day: BTreeMap<String, i64> = BTreeMap::new();
    let mut tokens_by_hour: BTreeMap<String, i64> = BTreeMap::new();

    for record in records {
        let timestamp = record_timestamp(record);
        let tokens = record.normalized_tokens();
        totals.add(record);

        apis.entry(usage_api_name(record)).or_default().add(record);

        sources
            .entry(usage_source_key(record))
            .or_insert_with(|| SourceUsageAccumulator::new(record))
            .add(record);

        let day_key = usage_day_key(timestamp);
        let hour_key = usage_hour_key(timestamp);
        *requests_by_day.entry(day_key.clone()).or_default() += 1;
        *requests_by_hour.entry(hour_key.clone()).or_default() += 1;
        *tokens_by_day.entry(day_key).or_default() += tokens.total_tokens;
        *tokens_by_hour.entry(hour_key).or_default() += tokens.total_tokens;
    }

    UsageSnapshot {
        total_requests: totals.total_requests,
        success_count: totals.success_count,
        failure_count: totals.failure_count,
        input_tokens: totals.input_tokens,
        output_tokens: totals.output_tokens,
        reasoning_tokens: totals.reasoning_tokens,
        cached_tokens: totals.cached_tokens,
        total_tokens: totals.total_tokens,
        apis: apis
            .into_iter()
            .map(|(key, value)| (key, make_api_usage(value)))
            .collect(),
        sources: sources
            .into_iter()
            .map(|(key, value)| (key, make_source_usage(value)))
            .collect(),
        requests_by_day,
        requests_by_hour,
        tokens_by_day,
        tokens_by_hour,
    }
}

fn make_api_usage(accumulator: APIUsageAccumulator) -> APIUsage {
    APIUsage {
        total_requests: accumulator.totals.total_requests,
        success_count: accumulator.totals.success_count,
        failure_count: accumulator.totals.failure_count,
        input_tokens: accumulator.totals.input_tokens,
        output_tokens: accumulator.totals.output_tokens,
        reasoning_tokens: accumulator.totals.reasoning_tokens,
        cached_tokens: accumulator.totals.cached_tokens,
        total_tokens: accumulator.totals.total_tokens,
        models: accumulator
            .models
            .into_iter()
            .map(|(key, value)| (key, make_model_usage(value)))
            .collect(),
    }
}

fn make_model_usage(mut accumulator: ModelUsageAccumulator) -> ModelUsage {
    accumulator.details.sort_by(|left, right| {
        parse_usage_date(&right.timestamp)
            .unwrap_or_else(Utc::now)
            .cmp(&parse_usage_date(&left.timestamp).unwrap_or_else(Utc::now))
    });

    ModelUsage {
        total_requests: accumulator.totals.total_requests,
        success_count: accumulator.totals.success_count,
        failure_count: accumulator.totals.failure_count,
        input_tokens: accumulator.totals.input_tokens,
        output_tokens: accumulator.totals.output_tokens,
        reasoning_tokens: accumulator.totals.reasoning_tokens,
        cached_tokens: accumulator.totals.cached_tokens,
        total_tokens: accumulator.totals.total_tokens,
        details: accumulator.details,
    }
}

fn make_source_usage(accumulator: SourceUsageAccumulator) -> SourceUsage {
    SourceUsage {
        name: accumulator.name,
        provider: accumulator.provider,
        auth_index: accumulator.auth_index,
        total_requests: accumulator.totals.total_requests,
        success_count: accumulator.totals.success_count,
        failure_count: accumulator.totals.failure_count,
        input_tokens: accumulator.totals.input_tokens,
        output_tokens: accumulator.totals.output_tokens,
        reasoning_tokens: accumulator.totals.reasoning_tokens,
        cached_tokens: accumulator.totals.cached_tokens,
        total_tokens: accumulator.totals.total_tokens,
        models: accumulator
            .models
            .into_iter()
            .map(|(key, value)| (key, make_source_model_usage(value)))
            .collect(),
    }
}

fn make_source_model_usage(totals: UsageTotals) -> SourceModelUsage {
    SourceModelUsage {
        total_requests: totals.total_requests,
        success_count: totals.success_count,
        failure_count: totals.failure_count,
        input_tokens: totals.input_tokens,
        output_tokens: totals.output_tokens,
        reasoning_tokens: totals.reasoning_tokens,
        cached_tokens: totals.cached_tokens,
        total_tokens: totals.total_tokens,
    }
}

fn usage_api_name(record: &BackendUsageQueueRecord) -> String {
    if let Some(endpoint) = normalized_optional(record.endpoint.as_deref()) {
        return endpoint;
    }
    normalized_name(record.provider.as_deref(), "unknown api")
}

fn usage_model_name(record: &BackendUsageQueueRecord) -> String {
    if let Some(model) = normalized_optional(record.model.as_deref()) {
        return model;
    }
    normalized_name(record.alias.as_deref(), "unknown model")
}

fn usage_source_key(record: &BackendUsageQueueRecord) -> String {
    [
        normalized_name(record.provider.as_deref(), "unknown provider"),
        normalized_name(Some(&record.source), "unknown source"),
        normalized_name(Some(&record.auth_index), ""),
    ]
    .join("|")
}

fn normalized_name(value: Option<&str>, fallback: &str) -> String {
    normalized_optional(value).unwrap_or_else(|| fallback.to_string())
}

fn normalized_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn record_timestamp(record: &BackendUsageQueueRecord) -> DateTime<Utc> {
    parse_usage_date(&record.timestamp).unwrap_or_else(Utc::now)
}

fn parse_usage_date(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&Utc))
        .ok()
}

fn usage_day_key(timestamp: DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .format("%Y-%m-%d")
        .to_string()
}

fn usage_hour_key(timestamp: DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:00")
        .to_string()
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage_record(
        timestamp: &str,
        provider: &str,
        model: &str,
        endpoint: &str,
        source: &str,
        failed: bool,
        tokens: TokenUsage,
    ) -> BackendUsageQueueRecord {
        BackendUsageQueueRecord {
            timestamp: timestamp.to_string(),
            provider: Some(provider.to_string()),
            model: Some(model.to_string()),
            endpoint: Some(endpoint.to_string()),
            source: source.to_string(),
            failed,
            tokens,
            ..BackendUsageQueueRecord::default()
        }
        .normalize()
    }

    #[test]
    fn aggregate_usage_groups_by_api_source_and_model() {
        let first = usage_record(
            "2026-05-11T10:00:00Z",
            "codex",
            "gpt-5",
            "/v1/responses",
            "codex-cli",
            false,
            TokenUsage {
                input_tokens: 10,
                output_tokens: 20,
                reasoning_tokens: 3,
                cached_tokens: 2,
                total_tokens: 0,
            },
        );
        let second = usage_record(
            "2026-05-11T11:00:00Z",
            "codex",
            "gpt-5",
            "/v1/responses",
            "codex-cli",
            true,
            TokenUsage {
                input_tokens: 8,
                output_tokens: 4,
                reasoning_tokens: 0,
                cached_tokens: 1,
                total_tokens: 12,
            },
        );

        let usage = aggregate_usage(&[&first, &second]);

        assert_eq!(usage.total_requests, 2);
        assert_eq!(usage.success_count, 1);
        assert_eq!(usage.failure_count, 1);
        assert_eq!(usage.total_tokens, 45);
        assert_eq!(usage.apis["/v1/responses"].models["gpt-5"].details.len(), 2);
        assert_eq!(
            usage.sources["codex|codex-cli|"].models["gpt-5"].total_requests,
            2
        );
    }

    #[test]
    fn time_range_matches_expected_local_days() {
        let now = parse_usage_date("2026-05-11T12:00:00Z").unwrap();
        let today = parse_usage_date("2026-05-11T01:00:00Z").unwrap();
        let seven_days = parse_usage_date("2026-05-05T01:00:00Z").unwrap();
        let too_old = parse_usage_date("2026-05-04T01:00:00Z").unwrap();

        assert!(BackendUsageTimeRange::Today.contains(today, now));
        assert!(BackendUsageTimeRange::SevenDays.contains(seven_days, now));
        assert!(!BackendUsageTimeRange::SevenDays.contains(too_old, now));
        assert!(BackendUsageTimeRange::All.contains(too_old, now));
    }
}
