use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::AuthError;
use crate::Result;

/// Minecraft accounts are personal Microsoft accounts, so sign-in goes through the
/// `consumers` tenant. `common` or a directory tenant fails at the Xbox step.
const AUTHORITY: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
/// `offline_access` is what makes Microsoft return a refresh token.
const SCOPE: &str = "XboxLive.signin offline_access";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// What the user needs to finish signing in from a browser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Clone, Deserialize)]
pub struct MsaTokens {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Default, Deserialize)]
struct OAuthError {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_description: String,
}

impl OAuthError {
    fn into_auth_error(self) -> AuthError {
        // Descriptions end with trace IDs and timestamps, on new lines or inline.
        let summary = self.error_description.lines().next().unwrap_or_default();
        let summary = summary
            .split_once(" Trace ID:")
            .map_or(summary, |(text, _)| text)
            .trim();
        if self.error == "unauthorized_client" || self.error == "invalid_client" {
            // Microsoft's own text for these is long and about tenants; say what to fix
            // and keep just its error code for searching.
            let code = summary
                .split_once(':')
                .map(|(code, _)| code)
                .filter(|code| code.starts_with("AADSTS"))
                .map(|code| format!(" ({code})"))
                .unwrap_or_default();
            AuthError::Microsoft(format!(
                "Microsoft didn't accept this Azure client ID{code}. Check that it's copied correctly, that the app allows personal Microsoft accounts, and that \"Allow public client flows\" is on."
            ))
        } else if summary.is_empty() {
            AuthError::Microsoft(self.error)
        } else {
            AuthError::Microsoft(summary.to_string())
        }
    }
}

async fn oauth_error(response: reqwest::Response) -> OAuthError {
    let status = response.status();
    response.json().await.unwrap_or_else(|_| OAuthError {
        error: format!("HTTP {status}"),
        error_description: String::new(),
    })
}

pub async fn request_device_code(client: &reqwest::Client, client_id: &str) -> Result<DeviceCode> {
    let response = client
        .post(format!("{AUTHORITY}/devicecode"))
        .form(&[("client_id", client_id), ("scope", SCOPE)])
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(oauth_error(response).await.into_auth_error().into());
    }
    Ok(response.json().await?)
}

#[derive(Debug, PartialEq, Eq)]
enum Poll {
    Wait,
    SlowDown,
    Fail,
}

fn classify_poll_error(error: &str) -> Poll {
    match error {
        "authorization_pending" => Poll::Wait,
        "slow_down" => Poll::SlowDown,
        _ => Poll::Fail,
    }
}

/// Wait for the user to enter the code in their browser. Returns when they finish,
/// decline, or the code expires. Cancel by dropping the future.
pub async fn poll_for_tokens(
    client: &reqwest::Client,
    client_id: &str,
    code: &DeviceCode,
) -> Result<MsaTokens> {
    let deadline = Instant::now() + Duration::from_secs(code.expires_in);
    let mut interval = Duration::from_secs(code.interval.max(1));
    loop {
        tokio::time::sleep(interval).await;
        if Instant::now() >= deadline {
            return Err(AuthError::Expired.into());
        }
        let response = client
            .post(format!("{AUTHORITY}/token"))
            .form(&[
                ("grant_type", DEVICE_GRANT),
                ("client_id", client_id),
                ("device_code", &code.device_code),
            ])
            .send()
            .await?;
        if response.status().is_success() {
            return Ok(response.json().await?);
        }
        let error = oauth_error(response).await;
        match classify_poll_error(&error.error) {
            Poll::Wait => {}
            Poll::SlowDown => interval += Duration::from_secs(5),
            Poll::Fail => {
                return Err(match error.error.as_str() {
                    "authorization_declined" | "access_denied" => AuthError::Declined,
                    "expired_token" | "bad_verification_code" => AuthError::Expired,
                    _ => error.into_auth_error(),
                }
                .into());
            }
        }
    }
}

pub async fn refresh(
    client: &reqwest::Client,
    client_id: &str,
    refresh_token: &str,
) -> Result<MsaTokens> {
    let response = client
        .post(format!("{AUTHORITY}/token"))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("scope", SCOPE),
        ])
        .send()
        .await?;
    if response.status().is_success() {
        return Ok(response.json().await?);
    }
    let error = oauth_error(response).await;
    // `invalid_grant`: the refresh token was revoked or is older than 90 days.
    Err(if error.error == "invalid_grant" {
        AuthError::SessionExpired
    } else {
        error.into_auth_error()
    }
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_errors() {
        assert_eq!(classify_poll_error("authorization_pending"), Poll::Wait);
        assert_eq!(classify_poll_error("slow_down"), Poll::SlowDown);
        assert_eq!(classify_poll_error("authorization_declined"), Poll::Fail);
        assert_eq!(classify_poll_error("expired_token"), Poll::Fail);
    }

    #[test]
    fn error_descriptions_are_trimmed_to_one_line() {
        let e = OAuthError {
            error: "invalid_request".into(),
            error_description: "AADSTS90023: Bad thing.\r\nTrace ID: abc\r\nTimestamp: now".into(),
        };
        assert_eq!(
            e.into_auth_error().to_string(),
            "Microsoft sign-in failed: AADSTS90023: Bad thing."
        );
    }

    #[test]
    fn rejected_app_explains_the_fix() {
        let e = OAuthError {
            error: "unauthorized_client".into(),
            error_description: "AADSTS700016: Application with identifier 'x' was not found in the directory 'y'. Trace ID: abc Correlation ID: def Timestamp: now".into(),
        };
        let message = e.into_auth_error().to_string();
        assert!(message.contains("Allow public client flows"));
        assert!(message.contains("(AADSTS700016)"));
        assert!(!message.contains("Trace ID") && !message.contains("directory"));
    }

    #[test]
    fn inline_trace_ids_are_cut() {
        let e = OAuthError {
            error: "invalid_request".into(),
            error_description: "AADSTS90023: Bad thing. Trace ID: abc Timestamp: now".into(),
        };
        assert_eq!(
            e.into_auth_error().to_string(),
            "Microsoft sign-in failed: AADSTS90023: Bad thing."
        );
    }
}
