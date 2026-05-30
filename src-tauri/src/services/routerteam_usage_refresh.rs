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
const ROUTERTEAM_FIXED_PASSWORD: &str = "a123654!@#";
const ROUTERTEAM_USAGE_TIMEOUT_SECS: u64 = 10;
const ROUTERTEAM_USAGE_AUTO_INTERVAL_SECS: u64 = 5;
const ROUTERTEAM_USAGE_TEMPLATE_TYPE: &str = "general";
const ROUTERTEAM_USAGE_SCRIPT_CODE: &str = r#"({
  request: {
    url: "{{baseUrl}}/api/user/codex-free-quota/reminder",
    method: "GET",
    headers: {
      Authorization: "Bearer {{apiKey}}",
      "User-Agent":
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36",
    },
  },
  extractor: function (response) {
    return {
      isValid:
        response.quota.windowRemainingQuota > 0.1 &&
        response.quota.weeklyRemainingQuota > 0.1,
      windowRemainingQuota: response.quota.windowRemainingQuota,
      total: response.quota.windowLimit,
      weeklyRemainingQuota: response.quota.weeklyRemainingQuota,
      cycleEndsAt: response.quota.cycleEndsAt,
      windowEndsAt: response.quota.windowEndsAt,
    };
  },
})"#;

#[derive(Clone)]
struct RouterTeamTarget {
    provider: Provider,
    account: String,
}

fn extract_routerteam_account(name: &str) -> Option<String> {
    let (_, account) = name.split_once(ROUTERTEAM_PROVIDER_PREFIX)?;
    let account = account.trim();
    if account.is_empty() {
        return None;
    }
    Some(account.to_string())
}

fn build_usage_script(access_token: String) -> UsageScript {
    UsageScript {
        enabled: true,
        language: "javascript".to_string(),
        code: ROUTERTEAM_USAGE_SCRIPT_CODE.to_string(),
        timeout: Some(ROUTERTEAM_USAGE_TIMEOUT_SECS),
        api_key: Some(access_token),
        base_url: Some(ROUTERTEAM_BASE_URL.to_string()),
        access_token: None,
        user_id: None,
        template_type: Some(ROUTERTEAM_USAGE_TEMPLATE_TYPE.to_string()),
        auto_query_interval: Some(ROUTERTEAM_USAGE_AUTO_INTERVAL_SECS),
        coding_plan_provider: None,
    }
}

fn apply_usage_script(provider: &mut Provider, access_token: String) {
    let meta = provider.meta.get_or_insert_with(ProviderMeta::default);
    meta.usage_script = Some(build_usage_script(access_token));
}

async fn login_routerteam_account(
    client: reqwest::Client,
    username: String,
) -> (String, Result<String, AppError>) {
    let result = async {
        let response = client
            .post(ROUTERTEAM_LOGIN_URL)
            .json(&serde_json::json!({
                "username": username,
                "password": ROUTERTEAM_FIXED_PASSWORD,
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

pub async fn refresh_codex_routerteam_usage_scripts(db: &Database) -> Result<usize, AppError> {
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
        return Ok(0);
    }

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
            .map(|username| login_routerteam_account(client.clone(), username)),
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
            continue;
        };

        apply_usage_script(&mut target.provider, token);
        db.save_provider("codex", &target.provider)?;
        updated += 1;
    }

    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::{build_usage_script, extract_routerteam_account, ROUTERTEAM_BASE_URL};

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
        let script = build_usage_script("token-123".to_string());
        assert!(script.enabled);
        assert_eq!(script.api_key.as_deref(), Some("token-123"));
        assert_eq!(script.base_url.as_deref(), Some(ROUTERTEAM_BASE_URL));
        assert_eq!(script.template_type.as_deref(), Some("general"));
        assert_eq!(script.auto_query_interval, Some(5));
        assert!(script.code.contains("codex-free-quota/reminder"));
    }
}
