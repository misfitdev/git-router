use crate::config::{find_route, load_config, resolve_key_path};
use std::os::unix::process::CommandExt;
use std::process::Command;

/// Parse the host from the SSH destination argument (e.g., `git@github.com` → `github.com`).
pub fn parse_host(destination: &str) -> &str {
    destination
        .split('@')
        .last()
        .unwrap_or(destination)
}

/// Extract the org (first path segment) from a git command argument.
/// Input examples:
///   `git-upload-pack 'planeraio/repo.git'` → args like ["git-upload-pack", "'planeraio/repo.git'"]
///   but git actually calls: ssh host git-upload-pack '/planeraio/repo.git'
///
/// We receive the full path as a single arg. Strip leading `/` and quotes,
/// then return everything (org + remaining path segments without .git suffix)
/// so the route matcher can try nested prefixes.
pub fn parse_org_path(git_command_args: &[String]) -> Option<String> {
    // The path is typically the last argument to git-upload-pack / git-receive-pack
    let path_arg = git_command_args.last()?;
    let cleaned = path_arg
        .trim_matches('\'')
        .trim_matches('"')
        .trim_start_matches('/');
    if cleaned.is_empty() {
        return None;
    }
    Some(cleaned.to_string())
}

/// Run the SSH wrapper: parse host/org, find route, exec ssh with the right key.
pub fn run(args: &[String]) -> ! {
    // args[0] = destination (e.g., git@github.com)
    // args[1..] = git command (e.g., git-upload-pack '/org/repo.git')
    if args.is_empty() {
        exec_ssh(args);
    }

    let host = parse_host(&args[0]);
    let git_cmd_args = &args[1..];

    let org_path = match parse_org_path(git_cmd_args) {
        Some(p) => p,
        None => exec_ssh(args),
    };

    let config = match load_config() {
        Ok(c) => c,
        Err(_) => exec_ssh(args),
    };

    match find_route(&config, host, &org_path) {
        Some(route) if route.ssh_key.is_some() => {
            let key_path = resolve_key_path(route.ssh_key.as_ref().unwrap());
            let mut ssh_args = vec![
                "-o".to_string(),
                format!("IdentityFile={}", key_path.display()),
                "-o".to_string(),
                "IdentitiesOnly=yes".to_string(),
            ];
            ssh_args.extend_from_slice(args);
            exec_ssh(&ssh_args);
        }
        _ => exec_ssh(args),
    }
}

/// Exec ssh, replacing the current process. Never returns.
fn exec_ssh(args: &[String]) -> ! {
    let err = Command::new("ssh").args(args).exec();
    eprintln!("git-router: failed to exec ssh: {err}");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_host_with_user() {
        assert_eq!(parse_host("git@github.com"), "github.com");
    }

    #[test]
    fn parse_host_without_user() {
        assert_eq!(parse_host("github.com"), "github.com");
    }

    #[test]
    fn parse_host_custom_user() {
        assert_eq!(parse_host("deploy@gitlab.com"), "gitlab.com");
    }

    #[test]
    fn parse_org_simple() {
        let args = vec![
            "git-upload-pack".to_string(),
            "'/planeraio/repo.git'".to_string(),
        ];
        assert_eq!(parse_org_path(&args).unwrap(), "planeraio/repo.git");
    }

    #[test]
    fn parse_org_nested_gitlab() {
        let args = vec![
            "git-upload-pack".to_string(),
            "'/planera.io/corp-it/auto-it.git'".to_string(),
        ];
        assert_eq!(
            parse_org_path(&args).unwrap(),
            "planera.io/corp-it/auto-it.git"
        );
    }

    #[test]
    fn parse_org_no_leading_slash() {
        let args = vec![
            "git-upload-pack".to_string(),
            "'planeraio/repo.git'".to_string(),
        ];
        assert_eq!(parse_org_path(&args).unwrap(), "planeraio/repo.git");
    }

    #[test]
    fn parse_org_empty() {
        let args: Vec<String> = vec![];
        assert!(parse_org_path(&args).is_none());
    }
}
