use crate::config::{find_route, load_config};
use std::collections::HashMap;
use std::io::{self, BufRead};

/// Parse credential helper input from stdin.
/// Format: key=value lines, terminated by a blank line or EOF.
pub fn parse_stdin(input: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.to_string(), value.to_string());
        }
    }
    map
}

/// Extract the org from the path field (first segment before `/`).
fn org_from_path(path: &str) -> &str {
    path.split('/').next().unwrap_or(path)
}

/// Handle the `get` operation: look up credentials and print them.
pub fn handle_get() {
    let mut input = String::new();
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        match line {
            Ok(l) if l.trim().is_empty() => break,
            Ok(l) => {
                input.push_str(&l);
                input.push('\n');
            }
            Err(_) => break,
        }
    }

    let fields = parse_stdin(&input);

    let host = match fields.get("host") {
        Some(h) => h.as_str(),
        None => return,
    };

    let path = match fields.get("path") {
        Some(p) => p.as_str(),
        None => return,
    };

    let org = org_from_path(path);

    let config = match load_config() {
        Ok(c) => c,
        Err(_) => return,
    };

    if let Some(route) = find_route(&config, host, org) {
        if let Some(token) = &route.token {
            println!("username=x-access-token");
            println!("password={token}");
        }
    }
}

/// Entry point for credential helper mode.
/// `store` and `erase` are no-ops.
pub fn run(operation: &str) {
    match operation {
        "get" => handle_get(),
        "store" | "erase" => {}
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_stdin_basic() {
        let input = "protocol=https\nhost=github.com\npath=planeraio/repo.git\n\n";
        let fields = parse_stdin(input);
        assert_eq!(fields.get("protocol").unwrap(), "https");
        assert_eq!(fields.get("host").unwrap(), "github.com");
        assert_eq!(fields.get("path").unwrap(), "planeraio/repo.git");
    }

    #[test]
    fn parse_stdin_no_trailing_newline() {
        let input = "protocol=https\nhost=github.com";
        let fields = parse_stdin(input);
        assert_eq!(fields.get("protocol").unwrap(), "https");
        assert_eq!(fields.get("host").unwrap(), "github.com");
    }

    #[test]
    fn parse_stdin_empty() {
        let fields = parse_stdin("");
        assert!(fields.is_empty());
    }

    #[test]
    fn org_from_path_simple() {
        assert_eq!(org_from_path("planeraio/repo.git"), "planeraio");
    }

    #[test]
    fn org_from_path_nested() {
        assert_eq!(org_from_path("planera.io/corp-it/repo.git"), "planera.io");
    }

    #[test]
    fn org_from_path_bare() {
        assert_eq!(org_from_path("solo"), "solo");
    }
}
