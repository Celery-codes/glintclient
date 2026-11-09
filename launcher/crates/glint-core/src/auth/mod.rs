//! Microsoft account sign-in: device code → Xbox Live → XSTS → Minecraft services.

pub mod minecraft;
pub mod msa;
pub mod store;
pub mod xbox;

use serde::Serialize;

use crate::{Result, fs_util, launch::Session, paths::Paths};
use store::{Account, Accounts, SecretStore};

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("No Azure client ID is set. Add one in Settings to sign in.")]
    MissingClientId,
    #[error("Sign-in was declined.")]
    Declined,
    #[error("The sign-in code expired before it was used. Try again.")]
    Expired,
    #[error("{message}")]
    Xbox { xerr: u64, message: String },
    #[error(
        "Mojang hasn't approved this Azure app for the Minecraft API yet. Submit its client ID at https://aka.ms/mce-reviewappid, then sign in again once it's approved."
    )]
    AppNotApproved,
    #[error("This Microsoft account doesn't own Minecraft: Java Edition.")]
    NoGameOwnership,
    #[error(
        "This account has no Minecraft profile yet. Sign in to the official launcher once to pick a username."
    )]
    NoProfile,
    #[error("Sign in to play.")]
    NotSignedIn,
    #[error("Your sign-in has expired. Sign in again.")]
    SessionExpired,
    #[error("Microsoft sign-in failed: {0}")]
    Microsoft(String),
    #[error("Couldn't use the system credential store: {0}")]
    Keyring(String),
}

impl AuthError {
    pub fn kind(&self) -> &'static str {
        match self {
            AuthError::MissingClientId => "auth_missing_client_id",
            AuthError::Declined => "auth_declined",
            AuthError::Expired => "auth_expired",
            AuthError::Xbox { .. } => "auth_xbox",
            AuthError::AppNotApproved => "auth_app_not_approved",
            AuthError::NoGameOwnership => "auth_no_game",
            AuthError::NoProfile => "auth_no_profile",
            AuthError::NotSignedIn => "auth_not_signed_in",
            AuthError::SessionExpired => "auth_session_expired",
            AuthError::Microsoft(_) => "auth_microsoft",
            AuthError::Keyring(_) => "auth_keyring",
        }
    }
}

/// A usable Minecraft login for one account.
#[derive(Debug, Clone, Serialize)]
pub struct SignedIn {
    pub account: Account,
    #[serde(skip)]
    pub session: Session,
    /// Unix time after which the Minecraft token must be refreshed.
    pub expires_at: u64,
}

impl SignedIn {
    /// Still valid with a margin, so a token can't lapse while the game is starting.
    pub fn is_fresh(&self) -> bool {
        self.expires_at > fs_util::now_unix() + 300
    }
}

/// Turn Microsoft tokens into a Minecraft session and remember the account.
///
/// Only the Microsoft refresh token is persisted (in the OS credential store); the Xbox
/// and Minecraft tokens are short-lived and re-derived from it.
pub async fn sign_in(
    client: &reqwest::Client,
    paths: &Paths,
    secrets: &dyn SecretStore,
    tokens: msa::MsaTokens,
) -> Result<SignedIn> {
    let xbl = xbox::authenticate(client, &tokens.access_token).await?;
    let xsts = xbox::authorize(client, &xbl.token).await?;
    let minecraft = minecraft::login_with_xbox(client, &xsts.user_hash, &xsts.token).await?;
    if !minecraft::owns_game(client, &minecraft.access_token).await? {
        return Err(AuthError::NoGameOwnership.into());
    }
    let profile = minecraft::profile(client, &minecraft.access_token).await?;

    let mut accounts = Accounts::load(paths).await?;
    let account = accounts.upsert(&profile.id, &profile.name);
    store::save_refresh_token(secrets, &account.uuid, &tokens.refresh_token)?;
    accounts.save(paths).await?;

    Ok(SignedIn {
        session: Session {
            username: profile.name,
            uuid: profile.id,
            access_token: minecraft.access_token,
            user_type: "msa",
        },
        account,
        expires_at: fs_util::now_unix() + minecraft.expires_in,
    })
}

/// Get a fresh session for a remembered account without user interaction.
pub async fn refresh(
    client: &reqwest::Client,
    paths: &Paths,
    secrets: &dyn SecretStore,
    client_id: &str,
    uuid: &str,
) -> Result<SignedIn> {
    let refresh_token =
        store::load_refresh_token(secrets, uuid)?.ok_or(AuthError::SessionExpired)?;
    let tokens = msa::refresh(client, client_id, &refresh_token).await?;
    // Microsoft rotates the refresh token on use; `sign_in` stores the new one.
    sign_in(client, paths, secrets, tokens).await
}

pub async fn sign_out(paths: &Paths, secrets: &dyn SecretStore, uuid: &str) -> Result<Accounts> {
    let mut accounts = Accounts::load(paths).await?;
    accounts.remove(uuid);
    accounts.save(paths).await?;
    store::delete_refresh_token(secrets, uuid)?;
    Ok(accounts)
}
