use serde::{Deserialize, Serialize};
use serde_json::json;

use super::AuthError;
use crate::Result;

const API: &str = "https://api.minecraftservices.com";

#[derive(Clone, Deserialize)]
pub struct MinecraftToken {
    pub access_token: String,
    /// Seconds; 24 hours in practice.
    pub expires_in: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// UUID without dashes.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub skins: Vec<Skin>,
    #[serde(default)]
    pub capes: Vec<Cape>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skin {
    pub id: String,
    pub state: String,
    pub url: String,
    #[serde(default)]
    pub variant: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cape {
    pub id: String,
    pub state: String,
    pub url: String,
    #[serde(default)]
    pub alias: Option<String>,
}

#[derive(Deserialize)]
struct Entitlements {
    #[serde(default)]
    items: Vec<serde_json::Value>,
}

/// Exchange the XSTS token for a Minecraft access token.
pub async fn login_with_xbox(
    client: &reqwest::Client,
    user_hash: &str,
    xsts_token: &str,
) -> Result<MinecraftToken> {
    let response = client
        .post(format!("{API}/authentication/login_with_xbox"))
        .header("Accept", "application/json")
        .json(&json!({ "identityToken": format!("XBL3.0 x={user_hash};{xsts_token}") }))
        .send()
        .await?;
    // 403 here means the Azure app itself isn't on Mojang's allow-list; the account is
    // fine. New app registrations stay in this state until Mojang reviews them.
    if response.status() == reqwest::StatusCode::FORBIDDEN {
        return Err(AuthError::AppNotApproved.into());
    }
    Ok(response.error_for_status()?.json().await?)
}

/// Whether the account has any Java Edition entitlement (purchase or Game Pass).
pub async fn owns_game(client: &reqwest::Client, access_token: &str) -> Result<bool> {
    let entitlements: Entitlements = client
        .get(format!("{API}/entitlements/mcstore"))
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(!entitlements.items.is_empty())
}

pub async fn profile(client: &reqwest::Client, access_token: &str) -> Result<Profile> {
    let response = client
        .get(format!("{API}/minecraft/profile"))
        .bearer_auth(access_token)
        .send()
        .await?;
    // Owns the game but never picked a username (typical for new Game Pass accounts).
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(AuthError::NoProfile.into());
    }
    Ok(response.error_for_status()?.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_profile() {
        let json = r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","skins":[{"id":"s","state":"ACTIVE","url":"http://textures.minecraft.net/texture/x","textureKey":"x","variant":"CLASSIC"}],"capes":[],"profileActions":{}}"#;
        let profile: Profile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.name, "Notch");
        assert_eq!(profile.skins[0].variant.as_deref(), Some("CLASSIC"));
    }
}
