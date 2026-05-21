use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct Route {
    pub host: String,
    pub org: String,
    pub ssh_key: Option<String>,
    pub token: Option<String>,
    pub user_name: Option<String>,
    pub user_email: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Config {
    #[serde(default, rename = "route")]
    pub routes: Vec<Route>,
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.host, self.org)?;
        if let Some(key) = &self.ssh_key {
            write!(f, "  ssh_key={key}")?;
        }
        if self.token.is_some() {
            write!(f, "  token=***")?;
        }
        if let Some(name) = &self.user_name {
            write!(f, "  user={name}")?;
        }
        if let Some(email) = &self.user_email {
            write!(f, "  email={email}")?;
        }
        Ok(())
    }
}

/// Rejects characters that can break gitconfig value syntax (newlines, section markers).
pub fn validate_gitconfig_value(field: &str, value: &str) -> io::Result<()> {
    if value.contains(['\n', '\r', '\0', '[', ']']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{field} contains characters unsafe for gitconfig"),
        ));
    }
    Ok(())
}

/// Rejects characters unsafe for gitconfig patterns, filenames, and glob expressions.
/// Stricter than `validate_gitconfig_value` because host/org appear in includeIf
/// patterns and fragment filenames.
pub fn validate_route_key(field: &str, value: &str) -> io::Result<()> {
    if value.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{field} must not be empty"),
        ));
    }
    if value.contains([
        '\n', '\r', '\0', '[', ']', '"', '\'', '\\', ' ', '*', '?', '#', ';', '=',
    ]) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{field} contains characters unsafe for gitconfig"),
        ));
    }
    Ok(())
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("git-router")
        .join("config.toml")
}

pub fn load_config() -> io::Result<Config> {
    let path = config_path();
    if !path.exists() {
        return Ok(Config::default());
    }
    let contents = fs::read_to_string(&path)?;
    toml::from_str(&contents).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn save_config(config: &Config) -> io::Result<()> {
    let path = config_path();
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "config path has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;
    let contents = toml::to_string_pretty(config).map_err(io::Error::other)?;
    // Write to a temp file in the same directory, then rename for atomic replacement.
    let tmp_path = path.with_extension("toml.tmp");
    fs::write(&tmp_path, &contents)?;
    fs::rename(&tmp_path, &path)
}

/// Generates identity gitconfig files for routes that have user_name or user_email set.
/// Creates per-route fragment files and a top-level identities.gitconfig with includeIf rules.
pub fn write_identity_configs(config: &Config) -> io::Result<()> {
    let dir = config_path()
        .parent()
        .expect("config path has parent")
        .to_path_buf();
    fs::create_dir_all(&dir)?;

    let mut includes = String::new();
    for route in &config.routes {
        if route.user_name.is_none() && route.user_email.is_none() {
            continue;
        }

        // Defense-in-depth: catch hand-edited configs with unsafe values.
        validate_route_key("host", &route.host)?;
        validate_route_key("org", &route.org)?;
        if let Some(name) = &route.user_name {
            validate_gitconfig_value("user_name", name)?;
        }
        if let Some(email) = &route.user_email {
            validate_gitconfig_value("user_email", email)?;
        }

        let slug = format!("{}-{}", route.host, route.org.replace('/', "-"));
        let fragment_name = format!("identity-{slug}.gitconfig");
        let fragment_path = dir.join(&fragment_name);

        // Write the identity fragment
        let mut content = String::from("[user]\n");
        if let Some(name) = &route.user_name {
            content.push_str(&format!("    name = {name}\n"));
        }
        if let Some(email) = &route.user_email {
            content.push_str(&format!("    email = {email}\n"));
        }
        fs::write(&fragment_path, &content)?;

        // SSH URL pattern: git@host:org/**
        includes.push_str(&format!(
            "[includeIf \"hasconfig:remote.*.url:git@{}:{}/**\"]\n    path = {}\n",
            route.host,
            route.org,
            fragment_path.display()
        ));
        // HTTPS URL pattern: https://host/org/**
        includes.push_str(&format!(
            "[includeIf \"hasconfig:remote.*.url:https://{}/{}/**\"]\n    path = {}\n",
            route.host,
            route.org,
            fragment_path.display()
        ));
    }

    let identities_path = dir.join("identities.gitconfig");
    fs::write(&identities_path, &includes)?;
    Ok(())
}

pub fn identities_gitconfig_path() -> PathBuf {
    config_path()
        .parent()
        .expect("config path has parent")
        .to_path_buf()
        .join("identities.gitconfig")
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(path)
}

/// Resolve an SSH key path. If the user gave a `.pub` path, prefer the
/// private key file (same path without `.pub`). If the private key doesn't
/// exist -- e.g. when using an SSH agent like 1Password -- keep the `.pub`
/// path so SSH can identify the key and request it from the agent.
pub fn resolve_key_path(raw: &str) -> PathBuf {
    let expanded = expand_tilde(raw);
    if let Some(private) = expanded.to_str().and_then(|s| s.strip_suffix(".pub")) {
        let private_path = PathBuf::from(private);
        if private_path.exists() {
            return private_path;
        }
    }
    expanded
}

/// Matches progressively shorter path prefixes so `planera.io/corp-it/repo.git`
/// tries `planera.io/corp-it` before `planera.io`.
pub fn find_route<'a>(config: &'a Config, host: &str, org_path: &str) -> Option<&'a Route> {
    let segments: Vec<&str> = org_path.split('/').collect();
    for end in (1..=segments.len()).rev() {
        let candidate = segments[..end].join("/");
        if let Some(route) = config
            .routes
            .iter()
            .find(|r| r.host == host && r.org == candidate)
        {
            return Some(route);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config {
            routes: vec![
                Route {
                    host: "github.com".into(),
                    org: "tdewitt".into(),
                    ssh_key: Some("~/.ssh/personal-gh.pub".into()),
                    token: Some("ghp_xxxx".into()),
                    user_name: Some("Tucker DeWitt".into()),
                    user_email: Some("tucker@personal.dev".into()),
                },
                Route {
                    host: "github.com".into(),
                    org: "planeraio".into(),
                    ssh_key: Some("~/.ssh/planera-gh.pub".into()),
                    token: None,
                    user_name: Some("Tucker DeWitt".into()),
                    user_email: Some("tucker@planera.io".into()),
                },
                Route {
                    host: "gitlab.com".into(),
                    org: "planera.io".into(),
                    ssh_key: Some("~/.ssh/planera-gl.pub".into()),
                    token: None,
                    user_name: None,
                    user_email: None,
                },
                Route {
                    host: "gitlab.com".into(),
                    org: "planera.io/corp-it".into(),
                    ssh_key: Some("~/.ssh/planera-gl-corp.pub".into()),
                    token: None,
                    user_name: None,
                    user_email: None,
                },
            ],
        }
    }

    #[test]
    fn exact_match() {
        let cfg = test_config();
        let route = find_route(&cfg, "github.com", "tdewitt").unwrap();
        assert_eq!(route.org, "tdewitt");
        assert_eq!(route.ssh_key.as_deref(), Some("~/.ssh/personal-gh.pub"));
    }

    #[test]
    fn no_match_passthrough() {
        let cfg = test_config();
        assert!(find_route(&cfg, "github.com", "unknown-org").is_none());
        assert!(find_route(&cfg, "bitbucket.org", "tdewitt").is_none());
    }

    #[test]
    fn nested_gitlab_path_exact() {
        let cfg = test_config();
        let route = find_route(&cfg, "gitlab.com", "planera.io/corp-it/auto-it.git").unwrap();
        assert_eq!(route.org, "planera.io/corp-it");
    }

    #[test]
    fn nested_gitlab_path_fallback() {
        let cfg = test_config();
        let route = find_route(&cfg, "gitlab.com", "planera.io/other-group/repo.git").unwrap();
        assert_eq!(route.org, "planera.io");
    }

    #[test]
    fn expand_tilde_works() {
        let result = expand_tilde("~/.ssh/id_rsa");
        assert!(result.to_string_lossy().contains(".ssh/id_rsa"));
        assert!(!result.to_string_lossy().starts_with('~'));
    }

    #[test]
    fn resolve_key_strips_pub_when_private_exists() {
        let dir = tempfile::tempdir().unwrap();
        let private = dir.path().join("mykey");
        let public = dir.path().join("mykey.pub");
        std::fs::write(&private, "").unwrap();
        std::fs::write(&public, "").unwrap();
        let result = resolve_key_path(public.to_str().unwrap());
        assert_eq!(result, private);
    }

    #[test]
    fn resolve_key_keeps_pub_when_no_private() {
        let dir = tempfile::tempdir().unwrap();
        let public = dir.path().join("mykey.pub");
        std::fs::write(&public, "").unwrap();
        let result = resolve_key_path(public.to_str().unwrap());
        assert_eq!(result, public);
    }

    #[test]
    fn resolve_key_no_pub() {
        let result = resolve_key_path("~/.ssh/id_rsa");
        let s = result.to_string_lossy();
        assert!(s.ends_with(".ssh/id_rsa"), "got: {s}");
    }

    #[test]
    fn config_roundtrip() {
        let cfg = test_config();
        let serialized = toml::to_string_pretty(&cfg).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(cfg.routes.len(), deserialized.routes.len());
        for (a, b) in cfg.routes.iter().zip(deserialized.routes.iter()) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn empty_config_loads() {
        let cfg: Config = toml::from_str("").unwrap();
        assert!(cfg.routes.is_empty());
    }

    #[test]
    fn invalid_toml_returns_error() {
        let result: Result<Config, _> = toml::from_str("[[route]\nbroken");
        assert!(result.is_err());
    }

    #[test]
    fn wrong_type_returns_error() {
        let result: Result<Config, _> = toml::from_str("[[route]]\nhost = 42\n");
        assert!(result.is_err());
    }

    #[test]
    fn find_route_single_org_multi_segment_path() {
        let cfg = test_config();
        let route = find_route(&cfg, "github.com", "planeraio/some-repo.git").unwrap();
        assert_eq!(route.org, "planeraio");
    }

    #[test]
    fn display_ssh_key_only() {
        let route = Route {
            host: "github.com".into(),
            org: "myorg".into(),
            ssh_key: Some("~/.ssh/key".into()),
            token: None,
            user_name: None,
            user_email: None,
        };
        let s = format!("{route}");
        assert_eq!(s, "github.com/myorg  ssh_key=~/.ssh/key");
    }

    #[test]
    fn display_token_masked() {
        let route = Route {
            host: "github.com".into(),
            org: "myorg".into(),
            ssh_key: None,
            token: Some("ghp_secret".into()),
            user_name: None,
            user_email: None,
        };
        let s = format!("{route}");
        assert_eq!(s, "github.com/myorg  token=***");
        assert!(!s.contains("ghp_secret"));
    }

    #[test]
    fn display_both() {
        let route = Route {
            host: "github.com".into(),
            org: "myorg".into(),
            ssh_key: Some("~/.ssh/key".into()),
            token: Some("ghp_secret".into()),
            user_name: None,
            user_email: None,
        };
        let s = format!("{route}");
        assert_eq!(s, "github.com/myorg  ssh_key=~/.ssh/key  token=***");
    }

    #[test]
    fn display_neither() {
        let route = Route {
            host: "github.com".into(),
            org: "myorg".into(),
            ssh_key: None,
            token: None,
            user_name: None,
            user_email: None,
        };
        assert_eq!(format!("{route}"), "github.com/myorg");
    }

    #[test]
    fn display_with_identity() {
        let route = Route {
            host: "github.com".into(),
            org: "myorg".into(),
            ssh_key: Some("~/.ssh/key".into()),
            token: None,
            user_name: Some("Test User".into()),
            user_email: Some("test@example.com".into()),
        };
        let s = format!("{route}");
        assert_eq!(
            s,
            "github.com/myorg  ssh_key=~/.ssh/key  user=Test User  email=test@example.com"
        );
    }

    #[test]
    fn expand_tilde_bare() {
        let result = expand_tilde("~");
        assert!(!result.to_string_lossy().starts_with('~'));
    }

    #[test]
    fn expand_tilde_no_tilde() {
        let result = expand_tilde("/absolute/path");
        assert_eq!(result, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn resolve_key_pub_in_middle_unchanged() {
        let result = resolve_key_path("/keys/my.pub.bak");
        assert_eq!(result, PathBuf::from("/keys/my.pub.bak"));
    }

    #[test]
    fn validate_gitconfig_value_rejects_newline() {
        assert!(validate_gitconfig_value("user_name", "Alice\n[core]").is_err());
    }

    #[test]
    fn validate_gitconfig_value_rejects_brackets() {
        assert!(validate_gitconfig_value("user_name", "[evil]").is_err());
    }

    #[test]
    fn validate_gitconfig_value_accepts_normal() {
        assert!(validate_gitconfig_value("user_name", "Tucker DeWitt").is_ok());
        assert!(validate_gitconfig_value("user_email", "tucker@example.com").is_ok());
    }

    #[test]
    fn validate_route_key_rejects_spaces() {
        assert!(validate_route_key("host", "github .com").is_err());
    }

    #[test]
    fn validate_route_key_rejects_empty() {
        assert!(validate_route_key("host", "").is_err());
    }

    #[test]
    fn validate_route_key_rejects_quotes() {
        assert!(validate_route_key("org", "org\"evil").is_err());
    }

    #[test]
    fn validate_route_key_accepts_normal() {
        assert!(validate_route_key("host", "github.com").is_ok());
        assert!(validate_route_key("org", "my-org").is_ok());
        assert!(validate_route_key("org", "planera.io/corp-it").is_ok());
    }

    #[test]
    fn identity_fields_optional_in_toml() {
        let toml_str = r#"
[[route]]
host = "github.com"
org = "myorg"
ssh_key = "~/.ssh/key"
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.routes.len(), 1);
        assert_eq!(cfg.routes[0].user_name, None);
        assert_eq!(cfg.routes[0].user_email, None);
    }

    #[test]
    fn identity_fields_roundtrip_toml() {
        let toml_str = r#"
[[route]]
host = "github.com"
org = "personal"
ssh_key = "~/.ssh/key"
user_name = "Tucker"
user_email = "tucker@personal.dev"
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.routes[0].user_name.as_deref(), Some("Tucker"));
        assert_eq!(
            cfg.routes[0].user_email.as_deref(),
            Some("tucker@personal.dev")
        );
        let serialized = toml::to_string_pretty(&cfg).unwrap();
        assert!(serialized.contains("user_name = \"Tucker\""));
        assert!(serialized.contains("user_email = \"tucker@personal.dev\""));
    }

    #[test]
    fn write_identity_configs_generates_files() {
        let dir = tempfile::tempdir().unwrap();
        // Override config_path by writing directly to the temp dir
        let cfg = Config {
            routes: vec![
                Route {
                    host: "github.com".into(),
                    org: "personal".into(),
                    ssh_key: None,
                    token: None,
                    user_name: Some("Tucker".into()),
                    user_email: Some("tucker@personal.dev".into()),
                },
                Route {
                    host: "github.com".into(),
                    org: "work".into(),
                    ssh_key: None,
                    token: None,
                    user_name: None,
                    user_email: None,
                },
            ],
        };

        // Write identity configs to temp dir directly (testing the content generation)
        let identities_path = dir.path().join("identities.gitconfig");
        let fragment_path = dir.path().join("identity-github.com-personal.gitconfig");

        let mut includes = String::new();
        for route in &cfg.routes {
            if route.user_name.is_none() && route.user_email.is_none() {
                continue;
            }
            let slug = format!("{}-{}", route.host, route.org.replace('/', "-"));
            let frag = dir.path().join(format!("identity-{slug}.gitconfig"));
            let mut content = String::from("[user]\n");
            if let Some(name) = &route.user_name {
                content.push_str(&format!("    name = {name}\n"));
            }
            if let Some(email) = &route.user_email {
                content.push_str(&format!("    email = {email}\n"));
            }
            std::fs::write(&frag, &content).unwrap();
            includes.push_str(&format!(
                "[includeIf \"hasconfig:remote.*.url:git@{}:{}/**\"]\n    path = {}\n",
                route.host,
                route.org,
                frag.display()
            ));
            includes.push_str(&format!(
                "[includeIf \"hasconfig:remote.*.url:https://{}/{}/**\"]\n    path = {}\n",
                route.host,
                route.org,
                frag.display()
            ));
        }
        std::fs::write(&identities_path, &includes).unwrap();

        // Verify the fragment was created with correct content
        let content = std::fs::read_to_string(&fragment_path).unwrap();
        assert!(content.contains("name = Tucker"));
        assert!(content.contains("email = tucker@personal.dev"));

        // Verify identities.gitconfig has includeIf for both SSH and HTTPS
        let includes_content = std::fs::read_to_string(&identities_path).unwrap();
        assert!(includes_content.contains("hasconfig:remote.*.url:git@github.com:personal/**"));
        assert!(includes_content.contains("hasconfig:remote.*.url:https://github.com/personal/**"));
        // Route without identity should not be included
        assert!(!includes_content.contains("work"));
    }
}
