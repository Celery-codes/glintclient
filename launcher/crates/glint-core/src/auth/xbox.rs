use serde::Deserialize;
use serde_json::json;

use super::AuthError;
use crate::{Error, Result};

const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";

#[derive(Clone)]
pub struct XboxToken {
    pub token: String,
    /// The "user hash" Minecraft wants alongside the XSTS token.
    pub user_hash: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TokenResponse {
    token: String,
    display_claims: DisplayClaims,
}

#[derive(Deserialize)]
struct DisplayClaims {
    xui: Vec<Xui>,
}

#[derive(Deserialize)]
struct Xui {
    uhs: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XstsError {
    x_err: u64,
}

impl TryFrom<TokenResponse> for XboxToken {
    type Error = Error;

    fn try_from(response: TokenResponse) -> Result<Self> {
        let user_hash = response
            .display_claims
            .xui
            .into_iter()
            .next()
            .map(|x| x.uhs)
            .ok_or_else(|| Error::Other("Xbox Live returned no user hash".into()))?;
        Ok(Self {
            token: response.token,
            user_hash,
        })
    }
}

/// Exchange a Microsoft access token for an Xbox Live token.
pub async fn authenticate(client: &reqwest::Client, msa_access_token: &str) -> Result<XboxToken> {
    let body = json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            // The `d=` prefix marks a token from an Azure app (as opposed to a legacy
            // Live one); Xbox Live rejects it without the prefix.
            "RpsTicket": format!("d={msa_access_token}"),
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT",
    });
    let response: TokenResponse = client
        .post(XBL_URL)
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    response.try_into()
}

/// Exchange the Xbox Live token for an XSTS token scoped to Minecraft services.
pub async fn authorize(client: &reqwest::Client, xbl_token: &str) -> Result<XboxToken> {
    let body = json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token],
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT",
    });
    let response = client
        .post(XSTS_URL)
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await?;
    // XSTS answers 401 with an XErr code for account-level problems.
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        let xerr = response
            .json::<XstsError>()
            .await
            .map(|e| e.x_err)
            .unwrap_or(0);
        return Err(AuthError::Xbox {
            xerr,
            message: xerr_message(xerr),
        }
        .into());
    }
    let response: TokenResponse = response.error_for_status()?.json().await?;
    response.try_into()
}

pub fn xerr_message(xerr: u64) -> String {
    match xerr {
        2148916227 => "This account is banned from Xbox Live.".into(),
        2148916233 => "This Microsoft account has no Xbox profile. Create one at xbox.com, then sign in again.".into(),
        2148916235 => "Xbox Live isn't available in this account's country or region.".into(),
        2148916236 | 2148916237 => "This account needs adult verification on xbox.com before it can sign in.".into(),
        2148916238 => "This is a child account. An adult has to add it to a Microsoft family group before it can sign in.".into(),
        other => format!("Xbox Live refused the sign-in (error {other})."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_token_response() {
        let json = r#"{"IssueInstant":"2026-01-01T00:00:00Z","NotAfter":"2026-01-15T00:00:00Z","Token":"tok","DisplayClaims":{"xui":[{"uhs":"hash"}]}}"#;
        let token: XboxToken = serde_json::from_str::<TokenResponse>(json)
            .unwrap()
            .try_into()
            .unwrap();
        assert_eq!(
            (token.token.as_str(), token.user_hash.as_str()),
            ("tok", "hash")
        );
    }

    #[test]
    fn parses_xsts_error() {
        let json = r#"{"Identity":"0","XErr":2148916238,"Message":"","Redirect":"https://start.ui.xboxlive.com/AddChildToFamily"}"#;
        let error: XstsError = serde_json::from_str(json).unwrap();
        assert!(xerr_message(error.x_err).contains("child account"));
        assert!(xerr_message(1).contains("error 1"));
    }
}
