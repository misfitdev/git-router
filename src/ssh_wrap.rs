use crate::config::{find_route, load_config, resolve_key_path};
use std::os::unix::process::CommandExt;
use std::process::Command;

pub fn parse_host(destination: &str) -> &str {
    destination.split('@').next_back().unwrap_or(destination)
}

pub fn parse_org_path(git_command_args: &[String]) -> Option<String> {
    let path_arg = git_command_args.last()?;

    // Git may pass the command and path as a single arg:
    //   "git-upload-pack 'org/repo.git'"
    // or as separate args:
    //   "git-upload-pack" "'org/repo.git'"
    // Extract the last whitespace-separated token to get the path.
    let path_part = path_arg.split_whitespace().next_back().unwrap_or(path_arg);

    let cleaned = path_part
        .trim_matches('\'')
        .trim_matches('"')
        .trim_start_matches('/');
    if cleaned.is_empty() {
        return None;
    }
    Some(cleaned.to_string())
}

/// SSH options that consume the next argument as a value.
const SSH_OPTS_WITH_VALUE: &[&str] = &[
    "-b", "-c", "-D", "-E", "-e", "-F", "-I", "-i", "-J", "-L", "-l", "-m", "-O", "-o", "-p", "-Q",
    "-R", "-S", "-W", "-w",
];

/// Find the destination argument by skipping SSH flags.
/// Returns (destination_index, git_command_start_index).
pub fn find_destination(args: &[String]) -> Option<(usize, usize)> {
    let mut i = 0;
    while i < args.len() {
        if SSH_OPTS_WITH_VALUE.contains(&args[i].as_str()) {
            i += 2; // skip flag + value
        } else if args[i].starts_with('-') {
            i += 1; // standalone flag
        } else {
            // first non-flag arg is the destination
            return Some((i, i + 1));
        }
    }
    None
}

pub fn run(args: &[String]) -> ! {
    if args.is_empty() {
        exec_ssh(args);
    }

    let (dest_idx, git_cmd_start) = match find_destination(args) {
        Some(v) => v,
        None => exec_ssh(args),
    };

    let host = parse_host(&args[dest_idx]);
    let git_cmd_args = &args[git_cmd_start..];

    let org_path = match parse_org_path(git_cmd_args) {
        Some(p) => p,
        None => exec_ssh(args),
    };

    let config = match load_config() {
        Ok(c) => c,
        Err(_) => exec_ssh(args),
    };

    let debug = std::env::var("GIT_ROUTER_DEBUG").is_ok();

    match find_route(&config, host, &org_path) {
        Some(route) if route.ssh_key.is_some() => {
            let key_path = resolve_key_path(route.ssh_key.as_ref().unwrap());
            if debug {
                eprintln!(
                    "git-router: matched {host}/{org_path} -> {}",
                    key_path.display()
                );
            }
            let mut ssh_args = vec![
                "-o".to_string(),
                format!("IdentityFile={}", key_path.display()),
                "-o".to_string(),
                "IdentitiesOnly=yes".to_string(),
            ];
            ssh_args.extend_from_slice(args);
            exec_ssh(&ssh_args);
        }
        _ => {
            if debug {
                eprintln!("git-router: no match for {host}/{org_path}, passthrough");
            }
            exec_ssh(args);
        }
    }
}

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

    #[test]
    fn parse_host_multiple_at_signs() {
        assert_eq!(parse_host("user@name@github.com"), "github.com");
    }

    #[test]
    fn parse_host_empty() {
        assert_eq!(parse_host(""), "");
    }

    #[test]
    fn parse_org_bare_slash() {
        let args = vec!["git-upload-pack".to_string(), "'/'".to_string()];
        assert!(parse_org_path(&args).is_none());
    }

    #[test]
    fn find_dest_simple() {
        let args: Vec<String> = vec!["git@github.com", "git-upload-pack", "'org/repo.git'"]
            .into_iter()
            .map(String::from)
            .collect();
        let (dest, git_start) = find_destination(&args).unwrap();
        assert_eq!(dest, 0);
        assert_eq!(git_start, 1);
    }

    #[test]
    fn find_dest_with_ssh_flags() {
        let args: Vec<String> = vec![
            "-o",
            "SendEnv=GIT_PROTOCOL",
            "git@github.com",
            "git-upload-pack",
            "'misfitdev/repo.git'",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let (dest, git_start) = find_destination(&args).unwrap();
        assert_eq!(args[dest], "git@github.com");
        assert_eq!(git_start, 3);
    }

    #[test]
    fn find_dest_with_multiple_flags() {
        let args: Vec<String> = vec![
            "-v",
            "-o",
            "SendEnv=GIT_PROTOCOL",
            "-p",
            "2222",
            "git@github.com",
            "git-upload-pack",
            "'org/repo.git'",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let (dest, _) = find_destination(&args).unwrap();
        assert_eq!(args[dest], "git@github.com");
    }

    #[test]
    fn find_dest_no_destination() {
        let args: Vec<String> = vec!["-v", "-o", "Foo=bar"]
            .into_iter()
            .map(String::from)
            .collect();
        assert!(find_destination(&args).is_none());
    }

    #[test]
    fn parse_org_single_arg_with_command() {
        let args = vec!["git-upload-pack 'misfitdev/git-router.git'".to_string()];
        assert_eq!(parse_org_path(&args).unwrap(), "misfitdev/git-router.git");
    }

    #[test]
    fn parse_org_double_quoted() {
        let args = vec![
            "git-upload-pack".to_string(),
            "\"/org/repo.git\"".to_string(),
        ];
        assert_eq!(parse_org_path(&args).unwrap(), "org/repo.git");
    }
}
