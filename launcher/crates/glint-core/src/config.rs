use serde::{Deserialize, Serialize};

use crate::{Result, fs_util, paths::Paths};

pub const ENV_AZURE_CLIENT_ID: &str = "GLINT_AZURE_CLIENT_ID";
pub const ENV_CURSEFORGE_API_KEY: &str = "GLINT_CURSEFORGE_API_KEY";

/// User-editable launcher settings, stored in `config.json`.
///
/// Nothing here is committed to the repo. The Azure client ID and CurseForge key can
/// also come from the environment or be baked in at build time (see [`resolve`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub azure_client_id: Option<String>,
    pub curseforge_api_key: Option<String>,
    pub max_concurrent_downloads: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            azure_client_id: None,
            curseforge_api_key: None,
            max_concurrent_downloads: 16,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Env,
    File,
    Builtin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub value: String,
    pub source: Source,
}

impl Config {
    pub async fn load(paths: &Paths) -> Result<Self> {
        Ok(fs_util::read_json_opt(&paths.config_file())
            .await?
            .unwrap_or_default())
    }

    pub async fn save(&self, paths: &Paths) -> Result<()> {
        fs_util::write_json(&paths.config_file(), self).await
    }

    pub fn azure_client_id(&self) -> Option<Resolved> {
        resolve(
            std::env::var(ENV_AZURE_CLIENT_ID).ok(),
            self.azure_client_id.clone(),
            option_env!("GLINT_AZURE_CLIENT_ID"),
        )
    }

    pub fn curseforge_api_key(&self) -> Option<Resolved> {
        resolve(
            std::env::var(ENV_CURSEFORGE_API_KEY).ok(),
            self.curseforge_api_key.clone(),
            option_env!("GLINT_CURSEFORGE_API_KEY"),
        )
    }
}

/// Precedence: environment variable, then `config.json`, then a value compiled into the
/// binary. The last one lets a public build ship a default without it living in the repo.
pub fn resolve(
    env: Option<String>,
    file: Option<String>,
    builtin: Option<&'static str>,
) -> Option<Resolved> {
    let pick = |value: Option<String>, source| {
        value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .map(|value| Resolved { value, source })
    };
    pick(env, Source::Env)
        .or_else(|| pick(file, Source::File))
        .or_else(|| pick(builtin.map(str::to_string), Source::Builtin))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_beats_file_beats_builtin() {
        let r = resolve(Some("env".into()), Some("file".into()), Some("built"));
        assert_eq!(r.unwrap().source, Source::Env);
        let r = resolve(None, Some("file".into()), Some("built"));
        assert_eq!(r.unwrap().source, Source::File);
        let r = resolve(None, None, Some("built")).unwrap();
        assert_eq!((r.value.as_str(), r.source), ("built", Source::Builtin));
        assert_eq!(resolve(None, None, None), None);
    }

    #[test]
    fn blank_values_are_skipped() {
        let r = resolve(Some("  ".into()), Some(" id ".into()), None).unwrap();
        assert_eq!((r.value.as_str(), r.source), ("id", Source::File));
    }

    #[tokio::test]
    async fn missing_file_gives_defaults_and_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let mut cfg = Config::load(&paths).await.unwrap();
        assert_eq!(cfg.max_concurrent_downloads, 16);
        cfg.azure_client_id = Some("abc".into());
        cfg.save(&paths).await.unwrap();
        let again = Config::load(&paths).await.unwrap();
        assert_eq!(again.azure_client_id.as_deref(), Some("abc"));
    }
}
