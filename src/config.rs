use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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
fn validate_route_key(field: &str, value: &str) -> io::Result<()> {
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

/// A host is a single path segment: it becomes part of a fragment filename
/// verbatim, so `/` would let it escape the config directory.
pub fn validate_host(field: &str, value: &str) -> io::Result<()> {
    validate_route_key(field, value)?;
    if value.contains('/') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{field} must not contain '/'"),
        ));
    }
    Ok(())
}

/// An org may contain `/` for nested namespaces; `..` is rejected so the
/// slug cannot walk out of the config directory.
pub fn validate_org(field: &str, value: &str) -> io::Result<()> {
    validate_route_key(field, value)?;
    if value.split('/').any(|seg| seg == ".." || seg.is_empty()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{field} must not contain empty or '..' path segments"),
        ));
    }
    Ok(())
}

/// Escapes a literal string for use as a git config `value_regex` (POSIX ERE).
fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        if "\\^$.[]|()*+?{}".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Anchored `value_regex` matching exactly one literal config value.
pub fn exact_value_regex(value: &str) -> String {
    format!("^{}$", regex_escape(value))
}

/// Strips a trailing `:port` from a credential `host` field so a route for
/// `github.com` still matches a remote served on a non-default port.
/// IPv6 literals arrive bracketed (`[::1]:8080`), so only a colon after the
/// closing bracket is a port separator.
pub fn strip_port(host: &str) -> &str {
    if host.starts_with('[') {
        return match host.find(']') {
            Some(end) => &host[..=end],
            None => host,
        };
    }
    match host.rsplit_once(':') {
        Some((h, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => h,
        _ => host,
    }
}

#[cfg(unix)]
fn create_dir_private(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
}

#[cfg(not(unix))]
fn create_dir_private(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)
}

#[cfg(unix)]
fn write_private(path: &Path, contents: &str) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(contents.as_bytes())
}

#[cfg(not(unix))]
fn write_private(path: &Path, contents: &str) -> io::Result<()> {
    fs::write(path, contents)
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
    create_dir_private(parent)?;
    let contents = toml::to_string_pretty(config).map_err(io::Error::other)?;
    // rename(2) is atomic only within a filesystem, so the temp file has to share
    // the destination's directory.
    let tmp_path = path.with_extension("toml.tmp");
    write_private(&tmp_path, &contents)?;
    fs::rename(&tmp_path, &path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub const GENERATED_GITCONFIG_NAME: &str = "git-router.gitconfig";

/// Pre-0.2 name for the generated include. Still removed on regeneration so an
/// upgraded install does not leave a stale include behind.
pub const LEGACY_GITCONFIG_NAME: &str = "identities.gitconfig";

/// Remote URL shapes git may record for `host`/`org`. Each becomes an includeIf
/// pattern; git glob-matches these against the whole remote URL.
fn remote_url_patterns(host: &str, org: &str) -> Vec<String> {
    vec![
        format!("*@{host}:{org}/**"),
        format!("{host}:{org}/**"),
        format!("ssh://*@{host}/{org}/**"),
        format!("ssh://*@{host}:*/{org}/**"),
        format!("ssh://{host}/{org}/**"),
        format!("https://{host}/{org}/**"),
        format!("https://*@{host}/{org}/**"),
        format!("https://{host}:*/{org}/**"),
    ]
}

pub fn identity_fragment_name(host: &str, org: &str) -> String {
    format!("identity-{}-{}.gitconfig", host, org.replace('/', "-"))
}

/// Generates the single gitconfig file that `include.path` points at, plus one
/// identity fragment per route that sets a committer identity.
///
/// The credential block is emitted only for routes that carry a token: `helper = ""`
/// clears inherited helpers for matching URLs, so emitting it for a tokenless route
/// would suppress the user's own credential store with nothing to replace it.
pub fn write_generated_gitconfig_in(dir: &Path, config: &Config) -> io::Result<()> {
    create_dir_private(dir)?;

    let mut out = String::from(
        "# Generated by git-router. Do not edit -- rewritten by `git router add|remove|init`.\n",
    );
    let mut expected_fragments = std::collections::HashSet::new();

    for route in &config.routes {
        // Defense-in-depth: catch hand-edited configs with unsafe values.
        let ctx = |e: io::Error| {
            io::Error::new(e.kind(), format!("route {}/{}: {e}", route.host, route.org))
        };
        validate_host("host", &route.host).map_err(ctx)?;
        validate_org("org", &route.org).map_err(ctx)?;

        if route.user_name.is_some() || route.user_email.is_some() {
            if let Some(name) = &route.user_name {
                validate_gitconfig_value("user_name", name).map_err(ctx)?;
            }
            if let Some(email) = &route.user_email {
                validate_gitconfig_value("user_email", email).map_err(ctx)?;
            }

            let fragment_name = identity_fragment_name(&route.host, &route.org);
            let fragment_path = dir.join(&fragment_name);
            expected_fragments.insert(fragment_name);

            let mut content = String::from("[user]\n");
            if let Some(name) = &route.user_name {
                content.push_str(&format!("    name = {name}\n"));
            }
            if let Some(email) = &route.user_email {
                content.push_str(&format!("    email = {email}\n"));
            }
            write_private(&fragment_path, &content)?;

            out.push('\n');
            for pattern in remote_url_patterns(&route.host, &route.org) {
                out.push_str(&format!(
                    "[includeIf \"hasconfig:remote.*.url:{}\"]\n    path = {}\n",
                    pattern,
                    fragment_path.display()
                ));
            }
        }

        if route.token.is_some() {
            out.push_str(&format!(
                "\n[credential \"https://{}/{}\"]\n    \
                 useHttpPath = true\n    \
                 helper = \"\"\n    \
                 helper = \"!git-router credential-helper\"\n",
                route.host, route.org
            ));
        }
    }

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with("identity-")
                && name.ends_with(".gitconfig")
                && !expected_fragments.contains(name)
            {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    let _ = fs::remove_file(dir.join(LEGACY_GITCONFIG_NAME));

    write_private(&dir.join(GENERATED_GITCONFIG_NAME), &out)
}

pub fn write_generated_gitconfig(config: &Config) -> io::Result<()> {
    write_generated_gitconfig_in(&config_dir(), config)
}

pub fn config_dir() -> PathBuf {
    config_path()
        .parent()
        .expect("config path has parent")
        .to_path_buf()
}

pub fn generated_gitconfig_path() -> PathBuf {
    config_dir().join(GENERATED_GITCONFIG_NAME)
}

pub fn legacy_gitconfig_path() -> PathBuf {
    config_dir().join(LEGACY_GITCONFIG_NAME)
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

/// Matches progressively shorter path prefixes so `acme.dev/platform/repo.git`
/// tries `acme.dev/platform` before `acme.dev`.
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
                    org: "acmecorp".into(),
                    ssh_key: Some("~/.ssh/acme-gh.pub".into()),
                    token: None,
                    user_name: Some("Tucker DeWitt".into()),
                    user_email: Some("dev@acme.dev".into()),
                },
                Route {
                    host: "gitlab.com".into(),
                    org: "acme.dev".into(),
                    ssh_key: Some("~/.ssh/acme-gl.pub".into()),
                    token: None,
                    user_name: None,
                    user_email: None,
                },
                Route {
                    host: "gitlab.com".into(),
                    org: "acme.dev/platform".into(),
                    ssh_key: Some("~/.ssh/acme-gl-platform.pub".into()),
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
        let route = find_route(&cfg, "gitlab.com", "acme.dev/platform/service.git").unwrap();
        assert_eq!(route.org, "acme.dev/platform");
    }

    #[test]
    fn nested_gitlab_path_fallback() {
        let cfg = test_config();
        let route = find_route(&cfg, "gitlab.com", "acme.dev/other-group/repo.git").unwrap();
        assert_eq!(route.org, "acme.dev");
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
        let route = find_route(&cfg, "github.com", "acmecorp/some-repo.git").unwrap();
        assert_eq!(route.org, "acmecorp");
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
        assert!(validate_route_key("org", "acme.dev/platform").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn write_private_sets_mode_600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.toml");
        write_private(&path, "token = \"ghp_secret\"").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {mode:04o}");
    }

    #[cfg(unix)]
    #[test]
    fn create_dir_private_sets_mode_700() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("private-dir");
        create_dir_private(&nested).unwrap();
        let mode = std::fs::metadata(&nested).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "expected 0700, got {mode:04o}");
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

    fn cfg(routes: Vec<Route>) -> Config {
        Config { routes }
    }

    fn route(host: &str, org: &str) -> Route {
        Route {
            host: host.into(),
            org: org.into(),
            ssh_key: None,
            token: None,
            user_name: None,
            user_email: None,
        }
    }

    #[test]
    fn generated_config_writes_identity_fragment_and_includes() {
        let dir = tempfile::tempdir().unwrap();
        let config = cfg(vec![
            Route {
                user_name: Some("Tucker".into()),
                user_email: Some("tucker@personal.dev".into()),
                ..route("github.com", "personal")
            },
            route("github.com", "work"),
        ]);

        write_generated_gitconfig_in(dir.path(), &config).unwrap();

        let fragment =
            std::fs::read_to_string(dir.path().join("identity-github.com-personal.gitconfig"))
                .unwrap();
        assert!(fragment.contains("name = Tucker"));
        assert!(fragment.contains("email = tucker@personal.dev"));

        let generated = std::fs::read_to_string(dir.path().join(GENERATED_GITCONFIG_NAME)).unwrap();
        assert!(generated.contains("hasconfig:remote.*.url:*@github.com:personal/**"));
        assert!(generated.contains("hasconfig:remote.*.url:https://github.com/personal/**"));
        // A route with neither identity nor token contributes nothing.
        assert!(!generated.contains("work"));
    }

    #[test]
    fn generated_config_emits_credential_section_only_for_token_routes() {
        let dir = tempfile::tempdir().unwrap();
        let config = cfg(vec![
            Route {
                token: Some("ghp_x".into()),
                ..route("github.com", "with-token")
            },
            Route {
                ssh_key: Some("~/.ssh/id.pub".into()),
                ..route("github.com", "no-token")
            },
        ]);

        write_generated_gitconfig_in(dir.path(), &config).unwrap();
        let generated = std::fs::read_to_string(dir.path().join(GENERATED_GITCONFIG_NAME)).unwrap();

        assert!(generated.contains("[credential \"https://github.com/with-token\"]"));
        assert!(generated.contains("useHttpPath = true"));
        // The empty helper resets inherited helpers; emitting it for a tokenless
        // route would disable the user's own credential store for those URLs.
        assert!(generated.contains("helper = \"\""));
        assert!(!generated.contains("[credential \"https://github.com/no-token\"]"));
    }

    #[test]
    fn generated_config_prunes_fragments_for_removed_routes() {
        let dir = tempfile::tempdir().unwrap();
        let with_route = cfg(vec![Route {
            user_email: Some("a@b.c".into()),
            ..route("github.com", "personal")
        }]);
        write_generated_gitconfig_in(dir.path(), &with_route).unwrap();
        let fragment = dir.path().join("identity-github.com-personal.gitconfig");
        assert!(fragment.exists());

        write_generated_gitconfig_in(dir.path(), &cfg(vec![])).unwrap();
        assert!(
            !fragment.exists(),
            "stale identity fragment left behind after route removal"
        );
    }

    #[test]
    fn generated_config_removes_legacy_include_file() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join(LEGACY_GITCONFIG_NAME);
        std::fs::create_dir_all(dir.path()).unwrap();
        std::fs::write(&legacy, "stale").unwrap();

        write_generated_gitconfig_in(dir.path(), &cfg(vec![])).unwrap();
        assert!(!legacy.exists());
    }

    #[test]
    fn generated_config_rejects_hand_edited_unsafe_route() {
        let dir = tempfile::tempdir().unwrap();
        let config = cfg(vec![Route {
            user_email: Some("a@b.c".into()),
            ..route("github.com/../evil", "org")
        }]);
        let err = write_generated_gitconfig_in(dir.path(), &config).unwrap_err();
        assert!(err.to_string().contains("github.com/../evil"), "{err}");
    }

    #[test]
    fn validate_host_rejects_path_separator() {
        assert!(validate_host("host", "a/../../b").is_err());
        assert!(validate_host("host", "github.com").is_ok());
    }

    #[test]
    fn validate_org_allows_nesting_but_rejects_traversal() {
        assert!(validate_org("org", "acme/foo").is_ok());
        assert!(validate_org("org", "acme/../foo").is_err());
        assert!(validate_org("org", "acme//foo").is_err());
    }

    #[test]
    fn strip_port_handles_ipv4_ipv6_and_bare_hosts() {
        assert_eq!(strip_port("github.com"), "github.com");
        assert_eq!(strip_port("github.com:8443"), "github.com");
        assert_eq!(strip_port("[::1]:8080"), "[::1]");
        assert_eq!(strip_port("[::1]"), "[::1]");
    }

    #[test]
    fn exact_value_regex_escapes_metacharacters() {
        let re = exact_value_regex("/a b/git-router.gitconfig");
        assert_eq!(re, "^/a b/git-router\\.gitconfig$");
    }
}
