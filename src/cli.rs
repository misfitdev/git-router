use crate::config::{self, Route};
use std::io;
use std::path::PathBuf;
use std::process::Command;

/// Sets a global gitconfig value, replacing only git-router's own entry.
///
/// A bare `--replace-all` with no value_regex deletes every value of a
/// multi-valued key, which for `include.path` means silently dropping unrelated
/// includes the user configured themselves.
fn set_global_own(key: &str, value: &str) -> io::Result<()> {
    let status = Command::new("git")
        .args([
            "config",
            "--global",
            "--replace-all",
            key,
            value,
            &config::exact_value_regex(value),
        ])
        .status()?;
    if !status.success() {
        eprintln!("Failed to set {key}");
        return Err(io::Error::other("git config failed"));
    }
    Ok(())
}

/// Removes a global gitconfig value if present. Failure is ignored: git exits 5
/// when the value is absent, which is the usual case.
fn unset_global_own(key: &str, value: &str) {
    let _ = Command::new("git")
        .args([
            "config",
            "--global",
            "--unset-all",
            key,
            &config::exact_value_regex(value),
        ])
        .status();
}

pub fn init() -> io::Result<()> {
    set_global_own("core.sshCommand", "git-router ssh-wrap")?;

    // Credentials are wired per-route inside the generated include, not globally:
    // a global helper cannot be scoped to an org, and git consults helpers in
    // declaration order, so an include added after an existing helper never wins.
    unset_global_own("credential.helper", "git-router credential-helper");

    let generated_path = config::generated_gitconfig_path();
    unset_global_own(
        "include.path",
        &config::legacy_gitconfig_path().to_string_lossy(),
    );
    set_global_own("include.path", &generated_path.to_string_lossy())?;

    let cfg = config::load_config()?;
    config::write_generated_gitconfig(&cfg)?;

    println!("Configured global gitconfig:");
    println!("  core.sshCommand = git-router ssh-wrap");
    println!("  include.path = {}", generated_path.display());
    println!("\nPer-route credential and identity config is generated into that file.");
    Ok(())
}

pub fn add(
    host: &str,
    org: &str,
    ssh_key: Option<&str>,
    token: Option<&str>,
    user_name: Option<&str>,
    user_email: Option<&str>,
) -> io::Result<()> {
    if ssh_key.is_none() && token.is_none() && user_name.is_none() && user_email.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "at least one of --ssh-key, --token, --user-name, or --user-email is required",
        ));
    }

    config::validate_host("host", host)?;
    config::validate_org("org", org)?;
    if let Some(name) = user_name {
        config::validate_gitconfig_value("user_name", name)?;
    }
    if let Some(email) = user_email {
        config::validate_gitconfig_value("user_email", email)?;
    }

    let mut cfg = config::load_config()?;

    if let Some(existing) = cfg
        .routes
        .iter_mut()
        .find(|r| r.host == host && r.org == org)
    {
        if let Some(key) = ssh_key {
            existing.ssh_key = Some(key.to_string());
        }
        if let Some(tok) = token {
            existing.token = Some(tok.to_string());
        }
        if let Some(name) = user_name {
            existing.user_name = Some(name.to_string());
        }
        if let Some(email) = user_email {
            existing.user_email = Some(email.to_string());
        }
        config::write_generated_gitconfig(&cfg)?;
        config::save_config(&cfg)?;
        println!("Updated route: {host}/{org}");
        return Ok(());
    }

    cfg.routes.push(Route {
        host: host.to_string(),
        org: org.to_string(),
        ssh_key: ssh_key.map(String::from),
        token: token.map(String::from),
        user_name: user_name.map(String::from),
        user_email: user_email.map(String::from),
    });
    // Generated config first: if it fails, config.toml is left untouched rather
    // than recording a route whose gitconfig was never written.
    config::write_generated_gitconfig(&cfg)?;
    config::save_config(&cfg)?;
    println!("Added route: {host}/{org}");
    Ok(())
}

pub fn list() -> io::Result<()> {
    let cfg = config::load_config()?;
    if cfg.routes.is_empty() {
        println!("No routes configured.");
        return Ok(());
    }
    for route in &cfg.routes {
        println!("{route}");
    }
    Ok(())
}

pub fn show() -> io::Result<()> {
    let cfg = config::load_config()?;
    if cfg.routes.is_empty() {
        println!("No routes configured.");
        return Ok(());
    }
    for (i, route) in cfg.routes.iter().enumerate() {
        if i > 0 {
            println!();
        }
        println!("[{}/{}]", route.host, route.org);
        if let Some(key) = &route.ssh_key {
            println!("  ssh_key    = {key}");
        }
        if route.token.is_some() {
            println!("  token      = ***");
        }
        if let Some(name) = &route.user_name {
            println!("  user_name  = {name}");
        }
        if let Some(email) = &route.user_email {
            println!("  user_email = {email}");
        }
    }
    Ok(())
}

pub fn remove(host: &str, org: &str) -> io::Result<()> {
    let mut cfg = config::load_config()?;
    let before = cfg.routes.len();
    cfg.routes.retain(|r| !(r.host == host && r.org == org));
    if cfg.routes.len() == before {
        println!("No route found for {host}/{org}");
        return Ok(());
    }
    config::write_generated_gitconfig(&cfg)?;
    config::save_config(&cfg)?;
    println!("Removed route: {host}/{org}");
    Ok(())
}

pub fn doctor() -> io::Result<()> {
    let mut all_ok = true;

    let cfg_path = config::config_path();
    if cfg_path.exists() {
        println!("[pass] Config file exists: {}", cfg_path.display());
    } else {
        println!("[FAIL] Config file not found: {}", cfg_path.display());
        all_ok = false;
    }

    let cfg = match config::load_config() {
        Ok(c) => {
            println!(
                "[pass] Config file parses successfully ({} routes)",
                c.routes.len()
            );
            Some(c)
        }
        Err(e) => {
            println!("[FAIL] Config file parse error: {e}");
            all_ok = false;
            None
        }
    };

    if let Some(ref cfg) = cfg {
        for route in &cfg.routes {
            if let Some(ref key_path) = route.ssh_key {
                let expanded = config::expand_tilde(key_path);
                let private = config::resolve_key_path(key_path);
                if expanded.exists() || private.exists() {
                    println!(
                        "[pass] SSH key for {}/{}: {}",
                        route.host, route.org, key_path
                    );
                } else {
                    println!(
                        "[FAIL] SSH key not found for {}/{}: {}",
                        route.host, route.org, key_path
                    );
                    all_ok = false;
                }
            }
        }
    }

    check_git_config("core.sshCommand", "git-router ssh-wrap", &mut all_ok);

    // The pre-0.2 global helper is uninvokable: git expands a non-`!`, non-absolute
    // helper to `git credential-<value>`, so this entry never runs.
    if git_config_values("credential.helper")
        .iter()
        .any(|v| v == "git-router credential-helper")
    {
        println!(
            "[FAIL] Stale global credential.helper = git-router credential-helper \
             (git cannot invoke it; run `git router init` to remove)"
        );
        all_ok = false;
    }

    let generated_path = config::generated_gitconfig_path();
    let generated = std::fs::read_to_string(&generated_path).ok();

    if generated.is_some() {
        println!("[pass] Generated config: {}", generated_path.display());
    } else {
        println!(
            "[FAIL] Generated config missing (run `git router init`): {}",
            generated_path.display()
        );
        all_ok = false;
    }

    // Existence is not wiring: without the include, git never reads the file.
    let generated_str = generated_path.to_string_lossy().to_string();
    if git_config_values("include.path").contains(&generated_str) {
        println!("[pass] include.path points at the generated config");
    } else {
        println!("[FAIL] include.path missing (run `git router init`): {generated_str}");
        all_ok = false;
    }

    if let (Some(cfg), Some(generated)) = (&cfg, &generated) {
        for route in cfg.routes.iter().filter(|r| r.token.is_some()) {
            let section = format!("[credential \"https://{}/{}\"]", route.host, route.org);
            if generated.contains(&section) {
                println!("[pass] Credential wiring for {}/{}", route.host, route.org);
            } else {
                println!(
                    "[FAIL] Credential wiring missing for {}/{} (run `git router init`)",
                    route.host, route.org
                );
                all_ok = false;
            }
        }
        if !cfg
            .routes
            .iter()
            .any(|r| r.user_name.is_some() || r.user_email.is_some())
        {
            println!("[info] No per-route identities configured");
        }
    }

    check_ssh_agent(&mut all_ok);
    check_include_ordering();

    if all_ok {
        println!("\nAll checks passed.");
        Ok(())
    } else {
        println!("\nSome checks failed. Run `git router init` to fix gitconfig.");
        Err(io::Error::other("doctor found problems"))
    }
}

/// Git applies the last value it reads, so a `user.name`/`user.email` written
/// below our include silently overrides every routed identity.
fn check_include_ordering() {
    let generated = config::generated_gitconfig_path();
    let generated = generated.to_string_lossy();

    let global = std::env::var_os("GIT_CONFIG_GLOBAL")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".gitconfig")));
    let Some(global) = global else { return };
    let Ok(text) = std::fs::read_to_string(&global) else {
        return;
    };

    let Some(include_line) = text
        .lines()
        .position(|l| l.contains(generated.as_ref()) && l.trim_start().starts_with("path"))
    else {
        return;
    };
    let overrides_after = text.lines().skip(include_line + 1).any(|l| {
        let t = l.trim_start();
        t.starts_with("name") || t.starts_with("email")
    });
    if overrides_after {
        println!(
            "[warn] user.name/user.email appears after include.path in {} \
             -- it will override routed identities; move it above the include",
            global.display()
        );
    }
}

fn git_config_values(key: &str) -> Vec<String> {
    match Command::new("git")
        .args(["config", "--global", "--get-all", key])
        .output()
    {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .collect(),
        _ => Vec::new(),
    }
}

fn check_git_config(key: &str, expected_contains: &str, all_ok: &mut bool) {
    match Command::new("git")
        .args(["config", "--global", "--get-all", key])
        .output()
    {
        Ok(output) if output.status.success() => {
            let value = String::from_utf8_lossy(&output.stdout);
            if value.lines().any(|line| line.trim() == expected_contains) {
                println!("[pass] {key} = {expected_contains}");
            } else {
                println!("[FAIL] {key} is set but does not include '{expected_contains}'");
                *all_ok = false;
            }
        }
        _ => {
            println!("[FAIL] {key} is not set");
            *all_ok = false;
        }
    }
}

fn check_ssh_agent(_all_ok: &mut bool) {
    // SSH agent is not required: git-router uses IdentityFile + IdentitiesOnly=yes
    // for key-file auth, which works without an agent. Missing agent is informational only.
    match std::env::var("SSH_AUTH_SOCK") {
        Ok(sock) => {
            let path = std::path::Path::new(&sock);
            if path.exists() {
                println!("[pass] SSH agent socket: {sock}");
            } else {
                println!("[warn] SSH_AUTH_SOCK set but socket does not exist: {sock}");
            }
        }
        Err(_) => {
            println!("[info] SSH_AUTH_SOCK is not set (not required for key-file auth)");
        }
    }
}
