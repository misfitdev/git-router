use crate::config::{self, Route};
use std::io;
use std::process::Command;

pub fn init() -> io::Result<()> {
    // --replace-all ensures re-running init is idempotent and doesn't stack duplicate entries.
    let status = Command::new("git")
        .args([
            "config",
            "--global",
            "--replace-all",
            "core.sshCommand",
            "git-router ssh-wrap",
        ])
        .status()?;
    if !status.success() {
        eprintln!("Failed to set core.sshCommand");
        return Err(io::Error::other("git config failed"));
    }

    let status = Command::new("git")
        .args([
            "config",
            "--global",
            "--replace-all",
            "credential.helper",
            "git-router credential-helper",
        ])
        .status()?;
    if !status.success() {
        eprintln!("Failed to set credential.helper");
        return Err(io::Error::other("git config failed"));
    }

    // Point global gitconfig at the identity includes file.
    let identities_path = config::identities_gitconfig_path();
    let status = Command::new("git")
        .args([
            "config",
            "--global",
            "--replace-all",
            "include.path",
            &identities_path.to_string_lossy(),
        ])
        .status()?;
    if !status.success() {
        eprintln!("Failed to set include.path for identities");
        return Err(io::Error::other("git config failed"));
    }

    // Generate identity gitconfig files from current routes.
    let cfg = config::load_config()?;
    config::write_identity_configs(&cfg)?;

    println!("Configured global gitconfig:");
    println!("  core.sshCommand = git-router ssh-wrap");
    println!("  credential.helper = git-router credential-helper");
    println!("  include.path = {}", identities_path.display());
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
        config::save_config(&cfg)?;
        config::write_identity_configs(&cfg)?;
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
    config::save_config(&cfg)?;
    config::write_identity_configs(&cfg)?;
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
    config::save_config(&cfg)?;
    config::write_identity_configs(&cfg)?;
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
    check_git_config(
        "credential.helper",
        "git-router credential-helper",
        &mut all_ok,
    );

    let identities_path = config::identities_gitconfig_path();
    if identities_path.exists() {
        println!("[pass] Identities config: {}", identities_path.display());
    } else {
        let has_identity = cfg
            .as_ref()
            .map(|c| {
                c.routes
                    .iter()
                    .any(|r| r.user_name.is_some() || r.user_email.is_some())
            })
            .unwrap_or(false);
        if has_identity {
            println!(
                "[FAIL] Identities config missing (run `git router init`): {}",
                identities_path.display()
            );
            all_ok = false;
        } else {
            println!("[info] No per-route identities configured");
        }
    }

    check_ssh_agent(&mut all_ok);

    if all_ok {
        println!("\nAll checks passed.");
    } else {
        println!("\nSome checks failed. Run `git router init` to fix gitconfig.");
    }

    Ok(())
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
