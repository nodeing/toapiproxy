use crate::auth::UsageData;
use reqwest::Client;
use serde::Deserialize;

/// CodeWhisperer API 响应结构
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageResponse {
    days_until_reset: Option<i32>,
    user_info: Option<UserInfo>,
    subscription_info: Option<SubscriptionInfo>,
    usage_breakdown_list: Option<Vec<UsageBreakdown>>,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionInfo {
    subscription_title: Option<String>,
    #[serde(rename = "type")]
    sub_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageBreakdown {
    usage_limit: Option<i32>,
    current_usage: Option<i32>,
    free_trial_info: Option<FreeTrialInfo>,
    bonuses: Option<Vec<BonusInfo>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FreeTrialInfo {
    usage_limit: Option<i32>,
    current_usage: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BonusInfo {
    usage_limit: Option<f64>,
    current_usage: Option<f64>,
    status: Option<String>,
}

pub struct UsageClient {
    client: Client,
}

impl UsageClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    /// 查询 Kiro 账户用量
    pub async fn fetch_kiro_usage(
        &self,
        access_token: &str,
    ) -> Result<(UsageData, Option<String>, Option<String>), String> {
        let url = "https://codewhisperer.us-east-1.amazonaws.com/getUsageLimits?isEmailRequired=true&origin=AI_EDITOR&resourceType=AGENTIC_REQUEST";

        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("x-amz-user-agent", "aws-sdk-js/1.0.0 TOAPIPROXY")
            .header(
                "user-agent",
                "aws-sdk-js/1.0.0 ua/2.1 os/windows TOAPIPROXY",
            )
            .header("amz-sdk-invocation-id", uuid::Uuid::new_v4().to_string())
            .header("amz-sdk-request", "attempt=1; max=1")
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("API error {}: {}", status, body));
        }

        let data: UsageResponse = response
            .json()
            .await
            .map_err(|e| format!("Parse error: {}", e))?;

        // 解析用量数据
        let mut main_used = 0;
        let mut main_limit = 0;
        let mut bonus_used = 0;
        let mut bonus_limit = 0;
        let mut trial_used = 0;
        let mut trial_limit = 0;

        if let Some(breakdowns) = &data.usage_breakdown_list {
            if let Some(breakdown) = breakdowns.first() {
                main_used = breakdown.current_usage.unwrap_or(0);
                main_limit = breakdown.usage_limit.unwrap_or(0);

                // Trial 信息
                if let Some(trial) = &breakdown.free_trial_info {
                    trial_used = trial.current_usage.unwrap_or(0);
                    trial_limit = trial.usage_limit.unwrap_or(0);
                }

                // Bonus 信息
                if let Some(bonuses) = &breakdown.bonuses {
                    for bonus in bonuses {
                        if bonus.status.as_deref() == Some("ACTIVE") || bonus.status.is_none() {
                            bonus_used += bonus.current_usage.unwrap_or(0.0) as i32;
                            bonus_limit += bonus.usage_limit.unwrap_or(0.0) as i32;
                        }
                    }
                }
            }
        }

        let total_used = main_used + trial_used + bonus_used;
        let total_limit = main_limit + trial_limit + bonus_limit;
        let percent = if total_limit > 0 {
            (total_used * 100 / total_limit) as i32
        } else {
            0
        };

        let usage = UsageData {
            used: total_used,
            limit: total_limit,
            percent,
            reset_days: data.days_until_reset,
            bonus_used: if bonus_limit > 0 {
                Some(bonus_used)
            } else {
                None
            },
            bonus_limit: if bonus_limit > 0 {
                Some(bonus_limit)
            } else {
                None
            },
        };

        let email = data.user_info.and_then(|u| u.email);
        let subscription = data
            .subscription_info
            .and_then(|s| s.subscription_title.or(s.sub_type));

        Ok((usage, email, subscription))
    }
}

impl Default for UsageClient {
    fn default() -> Self {
        Self::new()
    }
}
