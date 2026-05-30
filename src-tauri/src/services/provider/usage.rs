//! Usage script execution
//!
//! Handles executing and formatting usage query results.

use crate::app_config::AppType;
use crate::database::FailoverQueueItem;
use crate::error::AppError;
use crate::provider::{UsageData, UsageResult, UsageScript};
use crate::settings;
use crate::store::AppState;
use crate::usage_script;

const ROUTERTEAM_BASE_URL: &str = "https://ai.router.team";
const ROUTERTEAM_QUOTA_PATH: &str = "/api/user/codex-free-quota/reminder";
const FAILOVER_MIN_ACCOUNT_BALANCE: f64 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq)]
enum RouterTeamAccountBalanceOutcome {
    NotApplicable,
    Success(f64),
    Failed,
}

/// Execute usage script and format result (private helper method)
pub(crate) async fn execute_and_format_usage_result(
    script_code: &str,
    api_key: &str,
    base_url: &str,
    timeout: u64,
    access_token: Option<&str>,
    user_id: Option<&str>,
    template_type: Option<&str>,
) -> Result<UsageResult, AppError> {
    match usage_script::execute_usage_script(
        script_code,
        api_key,
        base_url,
        timeout,
        access_token,
        user_id,
        template_type,
    )
    .await
    {
        Ok(data) => {
            let usage_list: Vec<UsageData> = if data.is_array() {
                serde_json::from_value(data).map_err(|e| {
                    AppError::localized(
                        "usage_script.data_format_error",
                        format!("数据格式错误: {e}"),
                        format!("Data format error: {e}"),
                    )
                })?
            } else {
                let single: UsageData = serde_json::from_value(data).map_err(|e| {
                    AppError::localized(
                        "usage_script.data_format_error",
                        format!("数据格式错误: {e}"),
                        format!("Data format error: {e}"),
                    )
                })?;
                vec![single]
            };

            Ok(UsageResult {
                success: true,
                data: Some(usage_list),
                error: None,
                account_balance: None,
                account_balance_failed: None,
            })
        }
        Err(err) => {
            let lang = settings::get_settings()
                .language
                .unwrap_or_else(|| "zh".to_string());

            let msg = match err {
                AppError::Localized { zh, en, .. } => {
                    if lang == "en" {
                        en
                    } else {
                        zh
                    }
                }
                other => other.to_string(),
            };

            Ok(UsageResult {
                success: false,
                data: None,
                error: Some(msg),
                account_balance: None,
                account_balance_failed: None,
            })
        }
    }
}

pub(crate) fn usage_result_failover_reason(result: &UsageResult) -> Option<&'static str> {
    if !result.success {
        return None;
    }

    let (saw_invalid, saw_non_invalid) = result
        .data
        .as_ref()
        .map(|data| {
            (
                data.iter().any(|item| item.is_valid == Some(false)),
                data.iter().any(|item| item.is_valid != Some(false)),
            )
        })
        .unwrap_or((false, false));

    let invalid_only = saw_invalid && !saw_non_invalid;
    let account_balance_failed = result.account_balance_failed == Some(true);
    let low_balance = result
        .account_balance
        .is_some_and(|balance| balance <= FAILOVER_MIN_ACCOUNT_BALANCE);

    match (invalid_only, low_balance, account_balance_failed) {
        (true, true, _) => Some("isValid=false and accountBalance<=0.1"),
        (true, false, true) => Some("isValid=false and accountBalance query failed"),
        (true, false, false) => Some("isValid=false"),
        (false, true, _) => Some("accountBalance<=0.1"),
        (false, false, true) => Some("accountBalance query failed"),
        (false, false, false) => None,
    }
}

pub(crate) fn next_failover_provider_id(
    queue: &[FailoverQueueItem],
    current_provider_id: &str,
) -> Option<String> {
    if let Some(index) = queue
        .iter()
        .position(|item| item.provider_id == current_provider_id)
    {
        return queue
            .iter()
            .skip(index + 1)
            .find(|item| item.provider_id != current_provider_id)
            .map(|item| item.provider_id.clone());
    }

    queue
        .iter()
        .find(|item| item.provider_id != current_provider_id)
        .map(|item| item.provider_id.clone())
}

/// Extract API key from provider configuration
fn extract_api_key_from_provider(provider: &crate::provider::Provider) -> Option<String> {
    let settings = &provider.settings_config;

    crate::codex_config::extract_codex_api_key(
        settings.get("auth"),
        settings.get("config").and_then(|value| value.as_str()),
    )
    .or_else(|| {
        provider
            .meta
            .as_ref()
            .and_then(|meta| meta.api_key_field.as_deref())
            .and_then(|field| settings.get("env").and_then(|env| env.get(field)))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    })
    .or_else(|| {
        settings.get("env").and_then(|env| {
            env.get("ANTHROPIC_AUTH_TOKEN")
                .or_else(|| env.get("ANTHROPIC_API_KEY"))
                .or_else(|| env.get("OPENAI_API_KEY"))
                .or_else(|| env.get("GEMINI_API_KEY"))
                .or_else(|| env.get("OPENROUTER_API_KEY"))
                .or_else(|| env.get("GOOGLE_API_KEY"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
    })
    .or_else(|| {
        settings
            .get("options")
            .and_then(|options| options.get("apiKey"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    })
    .or_else(|| {
        settings
            .get("apiKey")
            .or_else(|| settings.get("api_key"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    })
}

fn should_query_routerteam_account_balance(usage_script: &UsageScript, base_url: &str) -> bool {
    if usage_script.template_type.as_deref() != Some("general") {
        return false;
    }

    if base_url.trim_end_matches('/') != ROUTERTEAM_BASE_URL {
        return false;
    }

    usage_script.code.contains(ROUTERTEAM_QUOTA_PATH)
}

async fn query_routerteam_account_balance(
    provider_api_key: Option<String>,
) -> RouterTeamAccountBalanceOutcome {
    let Some(provider_api_key) = provider_api_key.filter(|key| !key.trim().is_empty()) else {
        return RouterTeamAccountBalanceOutcome::Failed;
    };

    match crate::services::balance::get_routerteam_cc_switch_balance(&provider_api_key).await {
        Ok(balance) => RouterTeamAccountBalanceOutcome::Success(balance),
        Err(err) => {
            log::warn!("[Usage] RouterTeam account balance query failed: {err}");
            RouterTeamAccountBalanceOutcome::Failed
        }
    }
}

fn attach_routerteam_account_balance(
    usage_result: &mut UsageResult,
    balance_outcome: RouterTeamAccountBalanceOutcome,
) {
    match balance_outcome {
        RouterTeamAccountBalanceOutcome::NotApplicable => {}
        RouterTeamAccountBalanceOutcome::Success(balance) => {
            usage_result.account_balance = Some(balance);
            usage_result.account_balance_failed = Some(false);
        }
        RouterTeamAccountBalanceOutcome::Failed => {
            usage_result.account_balance = None;
            usage_result.account_balance_failed = Some(true);
        }
    }
}

/// Extract base URL from provider configuration
fn extract_base_url_from_provider(provider: &crate::provider::Provider) -> Option<String> {
    if let Some(env) = provider.settings_config.get("env") {
        // Try multiple possible base URL fields
        env.get("ANTHROPIC_BASE_URL")
            .or_else(|| env.get("GOOGLE_GEMINI_BASE_URL"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim_end_matches('/').to_string())
    } else {
        None
    }
}

/// Query provider usage (using saved script configuration)
pub async fn query_usage(
    state: &AppState,
    app_type: AppType,
    provider_id: &str,
) -> Result<UsageResult, AppError> {
    let (
        script_code,
        timeout,
        api_key,
        base_url,
        access_token,
        user_id,
        template_type,
        should_query_account_balance,
        provider_api_key,
    ) = {
        let providers = state.db.get_all_providers(app_type.as_str())?;
        let provider = providers.get(provider_id).ok_or_else(|| {
            AppError::localized(
                "provider.not_found",
                format!("供应商不存在: {provider_id}"),
                format!("Provider not found: {provider_id}"),
            )
        })?;

        let usage_script = provider
            .meta
            .as_ref()
            .and_then(|m| m.usage_script.as_ref())
            .ok_or_else(|| {
                AppError::localized(
                    "provider.usage.script.missing",
                    "未配置用量查询脚本",
                    "Usage script is not configured",
                )
            })?;
        if !usage_script.enabled {
            return Err(AppError::localized(
                "provider.usage.disabled",
                "用量查询未启用",
                "Usage query is disabled",
            ));
        }

        // Get credentials: prioritize UsageScript values, fallback to provider config
        let api_key = usage_script
            .api_key
            .clone()
            .filter(|k| !k.is_empty())
            .or_else(|| extract_api_key_from_provider(provider))
            .unwrap_or_default();

        let base_url = usage_script
            .base_url
            .clone()
            .filter(|u| !u.is_empty())
            .or_else(|| extract_base_url_from_provider(provider))
            .unwrap_or_default();

        let should_query_account_balance =
            should_query_routerteam_account_balance(usage_script, &base_url);
        let provider_api_key = should_query_account_balance
            .then(|| extract_api_key_from_provider(provider))
            .flatten();

        (
            usage_script.code.clone(),
            usage_script.timeout.unwrap_or(10),
            api_key,
            base_url,
            usage_script.access_token.clone(),
            usage_script.user_id.clone(),
            usage_script.template_type.clone(),
            should_query_account_balance,
            provider_api_key,
        )
    };

    let usage_future = execute_and_format_usage_result(
        &script_code,
        &api_key,
        &base_url,
        timeout,
        access_token.as_deref(),
        user_id.as_deref(),
        template_type.as_deref(),
    );
    let balance_future = async move {
        if should_query_account_balance {
            query_routerteam_account_balance(provider_api_key).await
        } else {
            RouterTeamAccountBalanceOutcome::NotApplicable
        }
    };

    let (usage_result, balance_outcome) = tokio::join!(usage_future, balance_future);
    let mut usage_result = usage_result?;
    attach_routerteam_account_balance(&mut usage_result, balance_outcome);
    Ok(usage_result)
}

/// Test usage script (using temporary script content, not saved)
#[allow(clippy::too_many_arguments)]
pub async fn test_usage_script(
    _state: &AppState,
    _app_type: AppType,
    _provider_id: &str,
    script_code: &str,
    timeout: u64,
    api_key: Option<&str>,
    base_url: Option<&str>,
    access_token: Option<&str>,
    user_id: Option<&str>,
    template_type: Option<&str>,
) -> Result<UsageResult, AppError> {
    // Use provided credential parameters directly for testing
    execute_and_format_usage_result(
        script_code,
        api_key.unwrap_or(""),
        base_url.unwrap_or(""),
        timeout,
        access_token,
        user_id,
        template_type,
    )
    .await
}

/// Validate UsageScript configuration (boundary checks)
pub(crate) fn validate_usage_script(script: &UsageScript) -> Result<(), AppError> {
    // Validate auto query interval (0-86400 seconds, max 24 hours)
    if let Some(interval) = script.auto_query_interval {
        if interval > 86_400 {
            return Err(AppError::localized(
                "usage_script.interval_too_large",
                format!("自动查询间隔不能超过 86400 秒（24小时），当前值: {interval}"),
                format!(
                    "Auto query interval cannot exceed 86400 seconds (24 hours), current: {interval}"
                ),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{next_failover_provider_id, usage_result_failover_reason};
    use crate::database::FailoverQueueItem;
    use crate::provider::{Provider, ProviderMeta, UsageData, UsageResult, UsageScript};
    use serde_json::json;

    fn queue(ids: &[&str]) -> Vec<FailoverQueueItem> {
        ids.iter()
            .map(|id| FailoverQueueItem {
                provider_id: (*id).to_string(),
                provider_name: format!("provider-{id}"),
                sort_index: None,
                provider_notes: None,
            })
            .collect()
    }

    #[test]
    fn invalid_single_result_requires_failover() {
        let result = UsageResult {
            success: true,
            data: Some(vec![UsageData {
                is_valid: Some(false),
                invalid_message: Some("expired".to_string()),
                plan_name: None,
                extra: None,
                total: None,
                used: None,
                remaining: None,
                window_remaining_quota: None,
                weekly_remaining_quota: None,
                cycle_ends_at: None,
                window_ends_at: None,
                unit: None,
            }]),
            error: None,
            account_balance: None,
            account_balance_failed: None,
        };

        assert!(usage_result_failover_reason(&result).is_some());
    }

    #[test]
    fn mixed_validity_does_not_require_failover() {
        let result = UsageResult {
            success: true,
            data: Some(vec![
                UsageData {
                    is_valid: Some(false),
                    invalid_message: Some("expired".to_string()),
                    plan_name: None,
                    extra: None,
                    total: None,
                    used: None,
                    remaining: None,
                    window_remaining_quota: None,
                    weekly_remaining_quota: None,
                    cycle_ends_at: None,
                    window_ends_at: None,
                    unit: None,
                },
                UsageData {
                    is_valid: Some(true),
                    invalid_message: None,
                    plan_name: None,
                    extra: None,
                    total: None,
                    used: None,
                    remaining: Some(1.0),
                    window_remaining_quota: None,
                    weekly_remaining_quota: None,
                    cycle_ends_at: None,
                    window_ends_at: None,
                    unit: None,
                },
            ]),
            error: None,
            account_balance: None,
            account_balance_failed: None,
        };

        assert!(usage_result_failover_reason(&result).is_none());
    }

    #[test]
    fn low_account_balance_requires_failover() {
        let result = UsageResult {
            success: true,
            data: Some(vec![UsageData {
                is_valid: Some(true),
                invalid_message: None,
                plan_name: None,
                extra: None,
                total: None,
                used: None,
                remaining: Some(1.0),
                window_remaining_quota: None,
                weekly_remaining_quota: None,
                cycle_ends_at: None,
                window_ends_at: None,
                unit: None,
            }]),
            error: None,
            account_balance: Some(0.1),
            account_balance_failed: Some(false),
        };

        assert!(usage_result_failover_reason(&result).is_some());
        assert_eq!(
            super::usage_result_failover_reason(&result),
            Some("accountBalance<=0.1")
        );
    }

    #[test]
    fn account_balance_query_failure_requires_failover() {
        let result = UsageResult {
            success: true,
            data: Some(vec![UsageData {
                is_valid: Some(true),
                invalid_message: None,
                plan_name: None,
                extra: None,
                total: None,
                used: None,
                remaining: None,
                window_remaining_quota: Some(2.0),
                weekly_remaining_quota: Some(8.0),
                cycle_ends_at: None,
                window_ends_at: None,
                unit: None,
            }]),
            error: None,
            account_balance: None,
            account_balance_failed: Some(true),
        };

        assert_eq!(
            super::usage_result_failover_reason(&result),
            Some("accountBalance query failed")
        );
    }

    #[test]
    fn healthy_balance_does_not_require_failover_when_usage_is_valid() {
        let result = UsageResult {
            success: true,
            data: Some(vec![UsageData {
                is_valid: Some(true),
                invalid_message: None,
                plan_name: None,
                extra: None,
                total: None,
                used: None,
                remaining: Some(1.0),
                window_remaining_quota: None,
                weekly_remaining_quota: None,
                cycle_ends_at: None,
                window_ends_at: None,
                unit: None,
            }]),
            error: None,
            account_balance: Some(0.11),
            account_balance_failed: Some(false),
        };

        assert!(usage_result_failover_reason(&result).is_none());
    }

    #[test]
    fn failover_target_is_next_queue_item() {
        let items = queue(&["p1", "p2", "p3"]);
        assert_eq!(
            next_failover_provider_id(&items, "p1").as_deref(),
            Some("p2")
        );
        assert_eq!(
            next_failover_provider_id(&items, "p2").as_deref(),
            Some("p3")
        );
        assert_eq!(next_failover_provider_id(&items, "p3"), None);
    }

    #[test]
    fn failover_target_falls_back_to_first_queue_item_when_current_missing() {
        let items = queue(&["p1", "p2"]);
        assert_eq!(
            next_failover_provider_id(&items, "missing").as_deref(),
            Some("p1")
        );
    }

    #[test]
    fn routerteam_balance_query_detection_requires_general_routerteam_quota_script() {
        let script = UsageScript {
            enabled: true,
            language: "javascript".to_string(),
            code: format!(
                "({{ request: {{ url: \"{{{{baseUrl}}}}{}\" }} }})",
                super::ROUTERTEAM_QUOTA_PATH
            ),
            timeout: Some(10),
            api_key: Some("quota-key".to_string()),
            base_url: Some(super::ROUTERTEAM_BASE_URL.to_string()),
            access_token: None,
            user_id: None,
            template_type: Some("general".to_string()),
            auto_query_interval: Some(5),
            coding_plan_provider: None,
        };

        assert!(super::should_query_routerteam_account_balance(
            &script,
            super::ROUTERTEAM_BASE_URL
        ));

        let mut custom = script.clone();
        custom.template_type = Some("custom".to_string());
        assert!(!super::should_query_routerteam_account_balance(
            &custom,
            super::ROUTERTEAM_BASE_URL
        ));
    }

    #[test]
    fn extracts_provider_api_key_from_codex_auth_config() {
        let provider = Provider {
            id: "p1".to_string(),
            name: "RouterTeam-demo@example.com".to_string(),
            settings_config: json!({
                "auth": {
                    "OPENAI_API_KEY": "provider-edit-key"
                },
                "config": ""
            }),
            website_url: None,
            category: None,
            created_at: None,
            sort_index: None,
            notes: None,
            meta: Some(ProviderMeta::default()),
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        };

        assert_eq!(
            super::extract_api_key_from_provider(&provider).as_deref(),
            Some("provider-edit-key")
        );
    }
}
