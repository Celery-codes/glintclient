use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub action: Action,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<HashMap<String, bool>>,
}

/// The machine and launch options that rules are evaluated against.
#[derive(Debug, Clone)]
pub struct Env {
    /// `windows`, `osx` or `linux`, as Mojang names them.
    pub os_name: &'static str,
    /// `x86`, `x86_64` or `arm64`.
    pub arch: &'static str,
    pub os_version: String,
    pub features: HashMap<String, bool>,
}

impl Env {
    pub fn current() -> Self {
        let os_name = if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "macos") {
            "osx"
        } else {
            "linux"
        };
        let arch = if cfg!(target_arch = "x86") {
            "x86"
        } else if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "x86_64"
        };
        // Version rules only ever distinguish Windows 10+ (which Java reports as "10.0",
        // on Windows 11 too) from older releases that current Java doesn't run on anyway.
        let os_version = if cfg!(target_os = "windows") {
            "10.0".to_string()
        } else {
            sysinfo::System::os_version().unwrap_or_default()
        };
        Self {
            os_name,
            arch,
            os_version,
            features: HashMap::new(),
        }
    }

    pub fn with_feature(mut self, name: &str, enabled: bool) -> Self {
        self.features.insert(name.to_string(), enabled);
        self
    }
}

impl Rule {
    fn matches(&self, env: &Env) -> bool {
        if let Some(os) = &self.os {
            if os.name.as_deref().is_some_and(|n| n != env.os_name) {
                return false;
            }
            if os.arch.as_deref().is_some_and(|a| a != env.arch) {
                return false;
            }
            if os
                .version
                .as_deref()
                .is_some_and(|v| !version_matches(v, &env.os_version))
            {
                return false;
            }
        }
        if let Some(features) = &self.features {
            for (name, wanted) in features {
                if env.features.get(name).copied().unwrap_or(false) != *wanted {
                    return false;
                }
            }
        }
        true
    }
}

/// Mojang writes OS versions as anchored regex prefixes such as `^10\.`. Those are the
/// only patterns in use, so this handles them as a prefix match instead of pulling in a
/// regex engine.
fn version_matches(pattern: &str, version: &str) -> bool {
    let literal = pattern.trim_start_matches('^').replace("\\.", ".");
    version.starts_with(&literal)
}

/// No rules means allowed. Otherwise the last matching rule decides, and nothing
/// matching means disallowed.
pub fn allows(rules: Option<&[Rule]>, env: &Env) -> bool {
    let Some(rules) = rules else { return true };
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule.matches(env) {
            allowed = rule.action == Action::Allow;
        }
    }
    allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(os: &'static str, arch: &'static str) -> Env {
        Env {
            os_name: os,
            arch,
            os_version: "10.0".into(),
            features: HashMap::new(),
        }
    }

    fn rules(json: &str) -> Vec<Rule> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn no_rules_allows() {
        assert!(allows(None, &env("windows", "x86_64")));
        assert!(allows(Some(&[]), &env("windows", "x86_64")));
    }

    #[test]
    fn os_allow() {
        let r = rules(r#"[{"action":"allow","os":{"name":"windows"}}]"#);
        assert!(allows(Some(&r), &env("windows", "x86_64")));
        assert!(!allows(Some(&r), &env("linux", "x86_64")));
    }

    #[test]
    fn allow_all_then_disallow_one() {
        let r = rules(r#"[{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]"#);
        assert!(allows(Some(&r), &env("windows", "x86_64")));
        assert!(!allows(Some(&r), &env("osx", "arm64")));
    }

    #[test]
    fn arch_and_version() {
        let r = rules(r#"[{"action":"allow","os":{"arch":"x86"}}]"#);
        assert!(!allows(Some(&r), &env("windows", "x86_64")));
        assert!(allows(Some(&r), &env("windows", "x86")));

        let r = rules(r#"[{"action":"allow","os":{"name":"windows","version":"^10\\."}}]"#);
        assert!(allows(Some(&r), &env("windows", "x86_64")));
        let mut old = env("windows", "x86_64");
        old.os_version = "6.1".into();
        assert!(!allows(Some(&r), &old));
    }

    #[test]
    fn features_default_to_off() {
        let r = rules(r#"[{"action":"allow","features":{"has_custom_resolution":true}}]"#);
        let plain = env("windows", "x86_64");
        assert!(!allows(Some(&r), &plain));
        assert!(allows(
            Some(&r),
            &plain.with_feature("has_custom_resolution", true)
        ));
    }
}
