use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use super::jvm;
use crate::{
    Error, LAUNCHER_NAME, LAUNCHER_VERSION, Result,
    instance::Instance,
    meta::{mojang::VersionInfo, rules::Env},
};

/// Who is playing. Built from a Microsoft sign-in, or as an offline profile in debug
/// builds.
#[derive(Clone)]
pub struct Session {
    pub username: String,
    /// Profile UUID without dashes.
    pub uuid: String,
    pub access_token: String,
    /// `msa` for a signed-in account.
    pub user_type: &'static str,
}

impl Session {
    /// An unauthenticated profile for testing the launch pipeline. Multiplayer on
    /// online-mode servers won't work with it.
    pub fn offline(username: &str) -> Self {
        Self {
            username: username.to_string(),
            uuid: "00000000000000000000000000000000".into(),
            access_token: "0".into(),
            user_type: "legacy",
        }
    }
}

// The access token is a credential: keep it out of debug output.
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("username", &self.username)
            .field("uuid", &self.uuid)
            .field("access_token", &"<redacted>")
            .finish()
    }
}

pub struct LaunchContext<'a> {
    pub version: &'a VersionInfo,
    pub instance: &'a Instance,
    pub session: &'a Session,
    pub java: &'a Path,
    pub java_major: u32,
    pub classpath: &'a [PathBuf],
    pub game_dir: &'a Path,
    pub natives_dir: &'a Path,
    pub assets_dir: &'a Path,
    pub libraries_dir: &'a Path,
}

#[derive(Debug, Clone)]
pub struct LaunchCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

/// The rule environment for launching `instance`.
pub fn launch_env(instance: &Instance) -> Env {
    Env::current().with_feature(
        "has_custom_resolution",
        instance.width.is_some() && instance.height.is_some(),
    )
}

fn substitute(template: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                // Unknown placeholders become empty, as in the official launcher.
                if let Some(value) = vars.get(&after[..end]) {
                    out.push_str(value);
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Drop `--option value` pairs whose value came out empty (for example `--xuid`, which
/// Glint doesn't have), rather than passing the game a blank argument.
fn drop_empty_options(args: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(args.len());
    for arg in args {
        if arg.is_empty() && out.last().is_some_and(|prev| prev.starts_with("--")) {
            out.pop();
        } else {
            out.push(arg);
        }
    }
    out
}

pub fn build(ctx: &LaunchContext) -> Result<LaunchCommand> {
    let arguments = ctx.version.arguments.as_ref().ok_or_else(|| {
        Error::Launch(format!(
            "Minecraft {} uses the pre-1.13 argument format, which Glint doesn't support",
            ctx.version.vanilla_id()
        ))
    })?;
    let env = launch_env(ctx.instance);
    let path = |p: &Path| p.to_string_lossy().into_owned();
    let separator = if cfg!(windows) { ";" } else { ":" };
    let classpath = ctx
        .classpath
        .iter()
        .map(|p| path(p))
        .collect::<Vec<_>>()
        .join(separator);

    let vars: HashMap<&str, String> = HashMap::from([
        ("natives_directory", path(ctx.natives_dir)),
        ("launcher_name", LAUNCHER_NAME.to_string()),
        ("launcher_version", LAUNCHER_VERSION.to_string()),
        ("classpath", classpath),
        ("classpath_separator", separator.to_string()),
        ("library_directory", path(ctx.libraries_dir)),
        ("auth_player_name", ctx.session.username.clone()),
        ("auth_uuid", ctx.session.uuid.clone()),
        ("auth_access_token", ctx.session.access_token.clone()),
        ("user_type", ctx.session.user_type.to_string()),
        ("version_name", ctx.version.id.clone()),
        ("version_type", ctx.version.kind.clone()),
        ("game_directory", path(ctx.game_dir)),
        ("assets_root", path(ctx.assets_dir)),
        ("assets_index_name", ctx.version.asset_index.id.clone()),
        (
            "resolution_width",
            ctx.instance
                .width
                .map(|w| w.to_string())
                .unwrap_or_default(),
        ),
        (
            "resolution_height",
            ctx.instance
                .height
                .map(|h| h.to_string())
                .unwrap_or_default(),
        ),
    ]);
    let expand = |list: &[crate::meta::mojang::Argument]| -> Vec<String> {
        list.iter()
            .flat_map(|a| a.values(&env))
            .map(|v| substitute(v, &vars))
            .collect()
    };

    // Order matters to the JVM: later flags override earlier ones, so the user's own
    // arguments go last among the JVM options.
    let mut args = jvm::memory_args(ctx.instance.memory_mb);
    args.extend(jvm::gc_args(ctx.instance.gc, ctx.java_major));
    args.extend(expand(&arguments.jvm));
    args.extend(jvm::split_args(&ctx.instance.jvm_args));
    args.push(ctx.version.main_class.clone());
    args.extend(drop_empty_options(expand(&arguments.game)));

    Ok(LaunchCommand {
        program: ctx.java.to_path_buf(),
        args,
        cwd: ctx.game_dir.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitution() {
        let vars = HashMap::from([("a", "1".to_string()), ("b", "two".to_string())]);
        assert_eq!(substitute("x=${a}/${b}", &vars), "x=1/two");
        assert_eq!(substitute("${missing}", &vars), "");
        assert_eq!(substitute("plain", &vars), "plain");
        assert_eq!(substitute("broken ${a", &vars), "broken ${a");
    }

    #[test]
    fn empty_option_values_are_dropped() {
        let args = ["--uuid", "abc", "--xuid", "", "--versionType", "release"]
            .map(String::from)
            .to_vec();
        assert_eq!(
            drop_empty_options(args),
            ["--uuid", "abc", "--versionType", "release"]
        );
    }

    #[test]
    fn session_debug_hides_token() {
        let mut s = Session::offline("Dev");
        s.access_token = "secret-token".into();
        assert!(!format!("{s:?}").contains("secret-token"));
    }
}
