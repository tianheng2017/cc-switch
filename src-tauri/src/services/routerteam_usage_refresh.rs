use crate::app_config::AppType;
use crate::database::Database;
use crate::error::AppError;
use crate::provider::{Provider, ProviderMeta, UsageScript};
use futures::future::join_all;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const ROUTERTEAM_PROVIDER_PREFIX: &str = "RouterTeam-";
const ROUTERTEAM_BASE_URL: &str = "https://ai.router.team";
const ROUTERTEAM_LOGIN_URL: &str = "https://ai.router.team/api/auth/login";
const ROUTERTEAM_USAGE_TIMEOUT_SECS: u64 = 10;
const ROUTERTEAM_USAGE_AUTO_INTERVAL_SECS: u64 = 5;
const ROUTERTEAM_USAGE_TEMPLATE_TYPE: &str = "general";
const ROUTERTEAM_QUOTA_PATH: &str = "/api/user/codex-free-quota/reminder";

#[derive(Clone)]
struct RouterTeamTarget {
    provider: Provider,
    account: String,
}

pub enum RouterTeamUsageRefreshOutcome {
    Updated(usize),
    NoTargets,
    SkippedMissingPassword { target_count: usize },
}

fn extract_routerteam_account(name: &str) -> Option<String> {
    let (_, account) = name.split_once(ROUTERTEAM_PROVIDER_PREFIX)?;
    let account = account.trim();
    if account.is_empty() {
        return None;
    }
    Some(account.to_string())
}

fn preserved_auto_query_interval(provider: &Provider) -> Option<u64> {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.usage_script.as_ref())
        .and_then(|usage_script| usage_script.auto_query_interval)
}

fn build_routerteam_usage_script_code(degraded_threshold: f64) -> String {
    let threshold =
        serde_json::to_string(&degraded_threshold).unwrap_or_else(|_| "0.1".to_string());

    format!(
        r#"({{
  request: {{
    url: "{{{{baseUrl}}}}{ROUTERTEAM_QUOTA_PATH}",
    method: "GET",
    headers: {{
      Authorization: "Bearer {{{{apiKey}}}}",
      "User-Agent":
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36",
    }},
  }},
  extractor: function (response) {{
    return {{
      isValid:
        response.quota.windowRemainingQuota > {threshold} &&
        response.quota.weeklyRemainingQuota > {threshold},
      windowRemainingQuota: response.quota.windowRemainingQuota,
      total: response.quota.windowLimit,
      weeklyRemainingQuota: response.quota.weeklyRemainingQuota,
      cycleEndsAt: response.quota.cycleEndsAt,
      windowEndsAt: response.quota.windowEndsAt,
    }};
  }},
}})"#
    )
}

fn build_usage_script(
    access_token: Option<String>,
    enabled: bool,
    auto_query_interval: Option<u64>,
    degraded_threshold: f64,
) -> UsageScript {
    UsageScript {
        enabled,
        language: "javascript".to_string(),
        code: build_routerteam_usage_script_code(degraded_threshold),
        timeout: Some(ROUTERTEAM_USAGE_TIMEOUT_SECS),
        api_key: access_token,
        base_url: Some(ROUTERTEAM_BASE_URL.to_string()),
        access_token: None,
        user_id: None,
        template_type: Some(ROUTERTEAM_USAGE_TEMPLATE_TYPE.to_string()),
        auto_query_interval: Some(
            auto_query_interval.unwrap_or(ROUTERTEAM_USAGE_AUTO_INTERVAL_SECS),
        ),
        coding_plan_provider: None,
    }
}

fn should_manage_routerteam_usage_threshold(provider: &Provider) -> bool {
    let Some(usage_script) = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.usage_script.as_ref())
    else {
        return false;
    };

    extract_routerteam_account(&provider.name).is_some()
        || (usage_script.template_type.as_deref() == Some(ROUTERTEAM_USAGE_TEMPLATE_TYPE)
            && usage_script
                .base_url
                .as_deref()
                .map(|url| url.trim_end_matches('/'))
                == Some(ROUTERTEAM_BASE_URL)
            && usage_script.code.contains(ROUTERTEAM_QUOTA_PATH))
}

fn apply_routerteam_usage_degraded_threshold(
    provider: &mut Provider,
    degraded_threshold: f64,
) -> bool {
    if !should_manage_routerteam_usage_threshold(provider) {
        return false;
    }

    let updated_code = build_routerteam_usage_script_code(degraded_threshold);
    let Some(usage_script) = provider
        .meta
        .as_mut()
        .and_then(|meta| meta.usage_script.as_mut())
    else {
        return false;
    };

    if usage_script.code == updated_code {
        return false;
    }

    usage_script.code = updated_code;
    true
}

fn apply_usage_script(provider: &mut Provider, access_token: String, degraded_threshold: f64) {
    let auto_query_interval = preserved_auto_query_interval(provider);
    let meta = provider.meta.get_or_insert_with(ProviderMeta::default);
    meta.usage_script = Some(build_usage_script(
        Some(access_token),
        true,
        auto_query_interval,
        degraded_threshold,
    ));
}

fn disable_usage_script(provider: &mut Provider, degraded_threshold: f64) {
    let auto_query_interval = preserved_auto_query_interval(provider);
    let meta = provider.meta.get_or_insert_with(ProviderMeta::default);
    meta.usage_script = Some(build_usage_script(
        None,
        false,
        auto_query_interval,
        degraded_threshold,
    ));
}

pub fn load_routerteam_usage_login_password(db: &Database) -> Result<Option<String>, AppError> {
    db.get_routerteam_usage_login_password()
}

pub fn load_routerteam_usage_degraded_threshold(db: &Database) -> Result<f64, AppError> {
    db.get_routerteam_usage_degraded_threshold()
}

pub fn reapply_routerteam_usage_degraded_threshold(db: &Database) -> Result<usize, AppError> {
    let degraded_threshold = load_routerteam_usage_degraded_threshold(db)?;
    let mut updated = 0usize;

    for app_type in AppType::all() {
        let providers = db.get_all_providers(app_type.as_str())?;
        for (_, mut provider) in providers {
            if !apply_routerteam_usage_degraded_threshold(&mut provider, degraded_threshold) {
                continue;
            }
            db.save_provider(app_type.as_str(), &provider)?;
            updated += 1;
        }
    }

    Ok(updated)
}

async fn login_routerteam_account(
    client: reqwest::Client,
    username: String,
    password: String,
) -> (String, Result<String, AppError>) {
    let result = async {
        let response = client
            .post(ROUTERTEAM_LOGIN_URL)
            .json(&serde_json::json!({
                "username": username,
                "password": password,
            }))
            .send()
            .await
            .map_err(|e| AppError::Message(format!("RouterTeam login request failed: {e}")))?;

        let response = response.error_for_status().map_err(|e| {
            AppError::Message(format!("RouterTeam login returned error status: {e}"))
        })?;

        let payload: Value = response.json().await.map_err(|e| {
            AppError::Message(format!("RouterTeam login response parse failed: {e}"))
        })?;

        payload
            .get("accessToken")
            .and_then(Value::as_str)
            .filter(|token| !token.trim().is_empty())
            .map(|token| token.to_string())
            .ok_or_else(|| {
                AppError::Message("RouterTeam login response missing accessToken".to_string())
            })
    }
    .await;

    (username, result)
}

pub async fn refresh_codex_routerteam_usage_scripts(
    db: &Database,
) -> Result<RouterTeamUsageRefreshOutcome, AppError> {
    let degraded_threshold = load_routerteam_usage_degraded_threshold(db)?;
    let providers = db.get_all_providers("codex")?;

    let targets: Vec<RouterTeamTarget> = providers
        .into_values()
        .filter_map(|provider| {
            let account = extract_routerteam_account(&provider.name)?;
            Some(RouterTeamTarget { provider, account })
        })
        .collect();

    if targets.is_empty() {
        log::debug!("○ No RouterTeam codex providers found for usage-script refresh");
        return Ok(RouterTeamUsageRefreshOutcome::NoTargets);
    }

    let Some(password) = load_routerteam_usage_login_password(db)? else {
        return Ok(RouterTeamUsageRefreshOutcome::SkippedMissingPassword {
            target_count: targets.len(),
        });
    };

    let accounts: Vec<String> = targets
        .iter()
        .map(|target| target.account.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(ROUTERTEAM_USAGE_TIMEOUT_SECS))
        .build()
        .map_err(|e| AppError::Message(format!("Failed to build RouterTeam login client: {e}")))?;

    let login_results = join_all(
        accounts
            .into_iter()
            .map(|username| login_routerteam_account(client.clone(), username, password.clone())),
    )
    .await;

    let mut tokens_by_account = HashMap::new();
    for (username, result) in login_results {
        match result {
            Ok(token) => {
                tokens_by_account.insert(username, token);
            }
            Err(e) => {
                log::warn!(
                    "✗ RouterTeam usage refresh login failed for {}: {}",
                    username,
                    e
                );
            }
        }
    }

    let mut updated = 0usize;

    for mut target in targets {
        let Some(token) = tokens_by_account.get(&target.account).cloned() else {
            log::warn!(
                "✗ RouterTeam usage refresh skipped provider '{}' because account '{}' has no accessToken",
                target.provider.name,
                target.account
            );
            disable_usage_script(&mut target.provider, degraded_threshold);
            db.save_provider("codex", &target.provider)?;
            continue;
        };

        apply_usage_script(&mut target.provider, token, degraded_threshold);
        db.save_provider("codex", &target.provider)?;
        updated += 1;
    }

    Ok(RouterTeamUsageRefreshOutcome::Updated(updated))
}

#[cfg(test)]
mod tests {
    use super::{
        apply_routerteam_usage_degraded_threshold, apply_usage_script,
        build_routerteam_usage_script_code, build_usage_script, disable_usage_script,
        extract_routerteam_account, load_routerteam_usage_degraded_threshold,
        load_routerteam_usage_login_password, ROUTERTEAM_BASE_URL,
    };
    use crate::database::Database;
    use crate::provider::{Provider, ProviderMeta};
    use serde_json::json;

    fn provider_with_interval(interval: u64) -> Provider {
        Provider {
            id: "provider-1".to_string(),
            name: "RouterTeam-demo@example.com".to_string(),
            settings_config: json!({}),
            website_url: None,
            category: Some("third_party".to_string()),
            created_at: None,
            sort_index: None,
            notes: None,
            meta: Some(ProviderMeta {
                usage_script: Some(build_usage_script(
                    Some("old-token".to_string()),
                    true,
                    Some(interval),
                    0.1,
                )),
                ..ProviderMeta::default()
            }),
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    #[test]
    fn extracts_routerteam_account_from_provider_name() {
        assert_eq!(
            extract_routerteam_account("RouterTeam-xiao7941688@qq.com").as_deref(),
            Some("xiao7941688@qq.com")
        );
        assert_eq!(
            extract_routerteam_account("foo RouterTeam-bar@example.com").as_deref(),
            Some("bar@example.com")
        );
        assert_eq!(extract_routerteam_account("RouterTeam-"), None);
        assert_eq!(extract_routerteam_account("OtherProvider"), None);
    }

    #[test]
    fn builds_expected_usage_script() {
        let script = build_usage_script(Some("token-123".to_string()), true, None, 0.25);
        assert!(script.enabled);
        assert_eq!(script.api_key.as_deref(), Some("token-123"));
        assert_eq!(script.base_url.as_deref(), Some(ROUTERTEAM_BASE_URL));
        assert_eq!(script.template_type.as_deref(), Some("general"));
        assert_eq!(script.auto_query_interval, Some(5));
        assert!(script.code.contains("codex-free-quota/reminder"));
        assert!(script.code.contains("> 0.25"));
    }

    #[test]
    fn builds_disabled_usage_script_without_token() {
        let script = build_usage_script(None, false, Some(30), 0.1);
        assert!(!script.enabled);
        assert_eq!(script.api_key, None);
        assert_eq!(script.auto_query_interval, Some(30));
    }

    #[test]
    fn apply_usage_script_preserves_existing_interval() {
        let mut provider = provider_with_interval(30);
        apply_usage_script(&mut provider, "new-token".to_string(), 0.1);

        let script = provider
            .meta
            .as_ref()
            .and_then(|meta| meta.usage_script.as_ref())
            .expect("usage script should exist");
        assert!(script.enabled);
        assert_eq!(script.api_key.as_deref(), Some("new-token"));
        assert_eq!(script.auto_query_interval, Some(30));
    }

    #[test]
    fn disable_usage_script_clears_token_and_preserves_interval() {
        let mut provider = provider_with_interval(45);
        disable_usage_script(&mut provider, 0.1);

        let script = provider
            .meta
            .as_ref()
            .and_then(|meta| meta.usage_script.as_ref())
            .expect("usage script should exist");
        assert!(!script.enabled);
        assert_eq!(script.api_key, None);
        assert_eq!(script.auto_query_interval, Some(45));
    }

    #[test]
    fn load_routerteam_usage_login_password_returns_none_when_unset() {
        let db = Database::memory().expect("memory db");

        assert_eq!(
            load_routerteam_usage_login_password(&db).expect("load password"),
            None
        );
    }

    #[test]
    fn load_routerteam_usage_login_password_prefers_saved_setting() {
        let db = Database::memory().expect("memory db");
        db.set_routerteam_usage_login_password(Some("custom-routerteam-password"))
            .expect("save password");

        assert_eq!(
            load_routerteam_usage_login_password(&db).expect("load password"),
            Some("custom-routerteam-password".to_string())
        );
    }

    #[test]
    fn load_routerteam_usage_degraded_threshold_defaults_to_point_one() {
        let db = Database::memory().expect("memory db");

        assert_eq!(
            load_routerteam_usage_degraded_threshold(&db).expect("load threshold"),
            0.1
        );
    }

    #[test]
    fn applies_routerteam_usage_degraded_threshold_to_existing_script() {
        let mut provider = provider_with_interval(30);

        assert!(apply_routerteam_usage_degraded_threshold(
            &mut provider,
            0.25
        ));
        let code = provider
            .meta
            .as_ref()
            .and_then(|meta| meta.usage_script.as_ref())
            .map(|script| script.code.clone())
            .expect("usage script");
        assert_eq!(code, build_routerteam_usage_script_code(0.25));
    }
}
