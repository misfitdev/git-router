use crate::config::{find_route, load_config};
use std::collections::HashMap;
use std::io::{self, BufRead};

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

    let config = match load_config() {
        Ok(c) => c,
        Err(_) => return,
    };

    if let Some(route) = find_route(&config, host, path) {
        if let Some(token) = &route.token {
            println!("username=x-access-token");
            println!("password={token}");
        }
    }
}

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
    fn parse_stdin_skips_lines_without_equals() {
        let input = "protocol=https\ngarbage\nhost=github.com\n";
        let fields = parse_stdin(input);
        assert_eq!(fields.get("protocol").unwrap(), "https");
        assert_eq!(fields.get("host").unwrap(), "github.com");
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn parse_stdin_empty_value() {
        let input = "protocol=https\nhost=\n";
        let fields = parse_stdin(input);
        assert_eq!(fields.get("host").unwrap(), "");
    }

    #[test]
    fn parse_stdin_duplicate_key_last_wins() {
        let input = "host=first.com\nhost=second.com\n";
        let fields = parse_stdin(input);
        assert_eq!(fields.get("host").unwrap(), "second.com");
    }

    #[test]
    fn parse_stdin_stops_at_blank_line() {
        let input = "host=github.com\n\npath=org/repo.git\n";
        let fields = parse_stdin(input);
        assert_eq!(fields.len(), 1);
        assert!(!fields.contains_key("path"));
    }

    #[test]
    fn nested_org_path_passed_to_find_route() {
        use crate::config::{Config, Route};
        let cfg = Config {
            routes: vec![
                Route {
                    host: "gitlab.com".into(),
                    org: "planera.io".into(),
                    ssh_key: None,
                    token: Some("tok-org".into()),
                    user_name: None,
                    user_email: None,
                },
                Route {
                    host: "gitlab.com".into(),
                    org: "planera.io/corp-it".into(),
                    ssh_key: None,
                    token: Some("tok-corp-it".into()),
                    user_name: None,
                    user_email: None,
                },
            ],
        };
        // Nested path should match the more specific route.
        let route = crate::config::find_route(&cfg, "gitlab.com", "planera.io/corp-it/repo.git");
        assert_eq!(route.and_then(|r| r.token.as_deref()), Some("tok-corp-it"));
        // Shallow path should fall back to org-level route.
        let route = crate::config::find_route(&cfg, "gitlab.com", "planera.io/other/repo.git");
        assert_eq!(route.and_then(|r| r.token.as_deref()), Some("tok-org"));
    }
}
