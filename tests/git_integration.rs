//! End-to-end tests that drive real `git` against the config git-router generates.
//!
//! Asserting on the strings git-router writes proves only self-consistency; every
//! check here makes git resolve the config and reports what git actually did.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_git-router"))
}

/// `!git-router credential-helper` is resolved by the shell, so the binary must be
/// reachable by name.
fn path_with_bin() -> String {
    let dir = bin().parent().unwrap().to_string_lossy().to_string();
    format!("{dir}:{}", std::env::var("PATH").unwrap_or_default())
}

/// Isolates every path git-router or git might resolve. Overriding `HOME` alone
/// is not enough: `dirs::config_dir()` prefers `XDG_CONFIG_HOME` on Linux, so a
/// test would otherwise read and write the real user's config directory.
fn isolate<'a>(cmd: &'a mut Command, home: &Path) -> &'a mut Command {
    cmd.env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("GIT_CONFIG_GLOBAL", home.join("gitconfig"))
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("PATH", path_with_bin())
}

fn git(home: &Path) -> Command {
    let mut c = Command::new("git");
    isolate(&mut c, home);
    c
}

fn router(home: &Path) -> Command {
    let mut c = Command::new(bin());
    isolate(&mut c, home);
    c
}

fn run(cmd: &mut Command) -> String {
    let out = cmd.output().expect("spawn failed");
    assert!(
        out.status.success(),
        "command failed: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Runs `git credential fill` and returns the resolved password, if any.
fn credential_password(home: &Path, url: &str) -> Option<String> {
    let mut cmd = git(home);
    cmd.args(["credential", "fill"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = cmd.spawn().expect("spawn git credential fill");
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().unwrap();
        write!(stdin, "url={url}\n\n").unwrap();
    }
    let out = child.wait_with_output().expect("wait");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("password=").map(str::to_string))
}

fn write_exec(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn credential_helper_is_invoked_and_returns_the_routed_token() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    run(router(home).args(["add", "github.com", "work-org", "--token", "ghp_worktoken"]));
    run(router(home).arg("init"));

    assert_eq!(
        credential_password(home, "https://github.com/work-org/repo.git").as_deref(),
        Some("ghp_worktoken"),
        "git did not reach git-router or did not get the routed token"
    );
}

#[test]
fn credential_helper_leaves_other_orgs_to_the_existing_helper() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    let fallback = home.join("git-credential-fallback");
    write_exec(
        &fallback,
        "#!/bin/sh\ncat >/dev/null\nprintf 'username=fallback\\npassword=FALLBACK\\n'\n",
    );

    run(router(home).args(["add", "github.com", "work-org", "--token", "ghp_worktoken"]));
    run(router(home).arg("init"));
    run(git(home).args([
        "config",
        "--global",
        "--add",
        "credential.helper",
        &format!("!{}", fallback.display()),
    ]));

    assert_eq!(
        credential_password(home, "https://github.com/work-org/repo.git").as_deref(),
        Some("ghp_worktoken"),
        "routed org must use git-router even though another helper is configured"
    );
    assert_eq!(
        credential_password(home, "https://github.com/other-org/repo.git").as_deref(),
        Some("FALLBACK"),
        "unrouted org must fall through to the user's own helper"
    );
}

#[test]
fn tokenless_route_does_not_suppress_the_existing_helper() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    let fallback = home.join("git-credential-fallback");
    write_exec(
        &fallback,
        "#!/bin/sh\ncat >/dev/null\nprintf 'username=fallback\\npassword=FALLBACK\\n'\n",
    );

    // SSH-only route: no token, so no credential section should be generated.
    run(router(home).args([
        "add",
        "github.com",
        "ssh-only",
        "--ssh-key",
        "~/.ssh/id_ed25519.pub",
    ]));
    run(router(home).arg("init"));
    run(git(home).args([
        "config",
        "--global",
        "--add",
        "credential.helper",
        &format!("!{}", fallback.display()),
    ]));

    assert_eq!(
        credential_password(home, "https://github.com/ssh-only/repo.git").as_deref(),
        Some("FALLBACK"),
        "a route without a token must not clear the user's credential helper"
    );
}

#[test]
fn credential_helper_refuses_cleartext_http() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    run(router(home).args(["add", "github.com", "work-org", "--token", "ghp_worktoken"]));
    run(router(home).arg("init"));

    // Wire the helper unconditionally, bypassing the https-scoped section, to prove
    // the helper itself refuses rather than relying only on config scoping.
    run(git(home).args([
        "config",
        "--global",
        "--add",
        "credential.helper",
        "!git-router credential-helper",
    ]));
    run(git(home).args(["config", "--global", "credential.useHttpPath", "true"]));

    assert_ne!(
        credential_password(home, "http://github.com/work-org/repo.git").as_deref(),
        Some("ghp_worktoken"),
        "token must never be emitted over cleartext http"
    );
}

#[test]
fn identity_applies_across_remote_url_forms() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    // Set the global identity before init: git applies the last value it reads, so
    // this must sit above the include for the routed identity to win.
    run(git(home).args(["config", "--global", "user.email", "global@example.com"]));
    run(router(home).args([
        "add",
        "github.com",
        "personal",
        "--ssh-key",
        "~/.ssh/personal.pub",
        "--user-name",
        "Routed Name",
        "--user-email",
        "routed@personal.dev",
    ]));
    run(router(home).arg("init"));

    let urls = [
        "git@github.com:personal/repo.git",
        "deploy@github.com:personal/repo.git",
        "ssh://git@github.com/personal/repo.git",
        "ssh://git@github.com:2222/personal/repo.git",
        "https://github.com/personal/repo.git",
        "https://tucker@github.com/personal/repo.git",
        "https://github.com:8443/personal/repo.git",
    ];

    for (i, url) in urls.iter().enumerate() {
        let repo = home.join(format!("repo{i}"));
        std::fs::create_dir_all(&repo).unwrap();
        run(git(home).args(["init", "-q"]).arg(&repo));
        run(git(home)
            .args(["-C"])
            .arg(&repo)
            .args(["remote", "add", "origin", url]));

        let email = run(git(home)
            .args(["-C"])
            .arg(&repo)
            .args(["config", "user.email"]));
        assert_eq!(
            email.trim(),
            "routed@personal.dev",
            "identity not applied for remote form: {url}"
        );
    }
}

#[test]
fn identity_does_not_leak_to_other_orgs() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    run(git(home).args(["config", "--global", "user.email", "global@example.com"]));
    run(router(home).args([
        "add",
        "github.com",
        "personal",
        "--ssh-key",
        "~/.ssh/personal.pub",
        "--user-email",
        "routed@personal.dev",
    ]));
    run(router(home).arg("init"));

    let repo = home.join("other");
    std::fs::create_dir_all(&repo).unwrap();
    run(git(home).args(["init", "-q"]).arg(&repo));
    run(git(home).args(["-C"]).arg(&repo).args([
        "remote",
        "add",
        "origin",
        "git@github.com:personal-other/x.git",
    ]));

    let email = run(git(home)
        .args(["-C"])
        .arg(&repo)
        .args(["config", "user.email"]));
    assert_eq!(
        email.trim(),
        "global@example.com",
        "org matching must be path-component exact, not string-prefix"
    );
}

#[test]
fn nested_org_identity_prefers_the_more_specific_route() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    run(router(home).args([
        "add",
        "gitlab.com",
        "acme",
        "--ssh-key",
        "~/.ssh/a.pub",
        "--user-email",
        "broad@acme.dev",
    ]));
    run(router(home).args([
        "add",
        "gitlab.com",
        "acme/foo",
        "--ssh-key",
        "~/.ssh/b.pub",
        "--user-email",
        "narrow@acme.dev",
    ]));
    run(router(home).arg("init"));

    let repo = home.join("nested");
    std::fs::create_dir_all(&repo).unwrap();
    run(git(home).args(["init", "-q"]).arg(&repo));
    run(git(home).args(["-C"]).arg(&repo).args([
        "remote",
        "add",
        "origin",
        "git@gitlab.com:acme/foo/bar.git",
    ]));

    let email = run(git(home)
        .args(["-C"])
        .arg(&repo)
        .args(["config", "user.email"]));
    assert_eq!(
        email.trim(),
        "narrow@acme.dev",
        "nested route must win over its parent"
    );
}

#[test]
fn init_preserves_unrelated_gitconfig_values() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    run(git(home).args([
        "config",
        "--global",
        "--add",
        "credential.helper",
        "osxkeychain",
    ]));
    run(git(home).args([
        "config",
        "--global",
        "--add",
        "credential.helper",
        "manager",
    ]));
    run(git(home).args([
        "config",
        "--global",
        "--add",
        "include.path",
        "/tmp/other-a.gitconfig",
    ]));
    run(git(home).args([
        "config",
        "--global",
        "--add",
        "include.path",
        "/tmp/other-b.gitconfig",
    ]));

    run(router(home).arg("init"));

    let helpers = run(git(home).args(["config", "--global", "--get-all", "credential.helper"]));
    assert!(helpers.contains("osxkeychain"), "helpers: {helpers}");
    assert!(helpers.contains("manager"), "helpers: {helpers}");

    let includes = run(git(home).args(["config", "--global", "--get-all", "include.path"]));
    assert!(
        includes.contains("other-a.gitconfig"),
        "includes: {includes}"
    );
    assert!(
        includes.contains("other-b.gitconfig"),
        "includes: {includes}"
    );
    assert!(
        includes.contains("git-router.gitconfig"),
        "includes: {includes}"
    );

    // Second init must not stack duplicates.
    run(router(home).arg("init"));
    let includes = run(git(home).args(["config", "--global", "--get-all", "include.path"]));
    assert_eq!(
        includes.matches("git-router.gitconfig").count(),
        1,
        "init should be idempotent: {includes}"
    );
}

#[test]
fn init_removes_the_uninvokable_legacy_helper() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    run(git(home).args([
        "config",
        "--global",
        "--add",
        "credential.helper",
        "git-router credential-helper",
    ]));
    run(git(home).args([
        "config",
        "--global",
        "--add",
        "credential.helper",
        "osxkeychain",
    ]));

    run(router(home).arg("init"));

    let helpers = run(git(home).args(["config", "--global", "--get-all", "credential.helper"]));
    assert!(
        !helpers.contains("git-router credential-helper"),
        "legacy uninvokable helper should be removed: {helpers}"
    );
    assert!(helpers.contains("osxkeychain"), "helpers: {helpers}");
}

#[test]
fn doctor_fails_when_the_include_is_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    run(router(home).args(["add", "github.com", "work-org", "--token", "ghp_x"]));
    run(router(home).arg("init"));

    // Simulate a user who removed the include but kept everything else.
    run(git(home).args(["config", "--global", "--unset-all", "include.path"]));

    let out = router(home).arg("doctor").output().unwrap();
    assert!(
        !out.status.success(),
        "doctor must not pass without the include: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// Prepends a directory holding a fake `ssh` that records its argv, so the
/// wrapper's exec target can be inspected. `exec_ssh` resolves `ssh` from PATH.
fn fake_ssh(home: &Path) -> String {
    let dir = home.join("fakebin");
    std::fs::create_dir_all(&dir).unwrap();
    write_exec(
        &dir.join("ssh"),
        &format!(
            "#!/bin/sh\nfor a in \"$@\"; do echo \"$a\"; done > {}/ssh-argv\n",
            home.display()
        ),
    );
    format!("{}:{}", dir.display(), path_with_bin())
}

fn ssh_argv(home: &Path) -> Vec<String> {
    std::fs::read_to_string(home.join("ssh-argv"))
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn ssh_wrap_injects_the_routed_key() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    std::fs::create_dir_all(home.join(".ssh")).unwrap();
    std::fs::write(home.join(".ssh/work.pub"), "ssh-ed25519 AAAA work\n").unwrap();

    run(router(home).args(["add", "github.com", "work", "--ssh-key", "~/.ssh/work.pub"]));

    let status = router(home)
        .env("PATH", fake_ssh(home))
        .args([
            "ssh-wrap",
            "git@github.com",
            "git-upload-pack 'work/repo.git'",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "ssh-wrap failed");

    let argv = ssh_argv(home);
    let expected_key = format!("IdentityFile={}/.ssh/work.pub", home.display());
    assert_eq!(
        argv,
        vec![
            "-o".to_string(),
            expected_key,
            "-o".to_string(),
            "IdentitiesOnly=yes".to_string(),
            "git@github.com".to_string(),
            "git-upload-pack 'work/repo.git'".to_string(),
        ],
        "ssh did not receive the routed key ahead of git's own arguments"
    );
}

#[test]
fn ssh_wrap_passes_through_an_unrouted_org_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    std::fs::create_dir_all(home.join(".ssh")).unwrap();
    std::fs::write(home.join(".ssh/work.pub"), "ssh-ed25519 AAAA work\n").unwrap();

    run(router(home).args(["add", "github.com", "work", "--ssh-key", "~/.ssh/work.pub"]));

    let status = router(home)
        .env("PATH", fake_ssh(home))
        .args([
            "ssh-wrap",
            "git@github.com",
            "git-upload-pack 'someone-else/repo.git'",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    assert_eq!(
        ssh_argv(home),
        vec![
            "git@github.com".to_string(),
            "git-upload-pack 'someone-else/repo.git'".to_string(),
        ],
        "unrouted org must reach ssh with git's arguments untouched"
    );
}

#[test]
fn ssh_wrap_passes_through_when_no_config_exists() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    let status = router(home)
        .env("PATH", fake_ssh(home))
        .args([
            "ssh-wrap",
            "git@github.com",
            "git-upload-pack 'any/repo.git'",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    assert_eq!(
        ssh_argv(home),
        vec![
            "git@github.com".to_string(),
            "git-upload-pack 'any/repo.git'".to_string(),
        ],
        "a missing config must not block a git operation"
    );
}

#[test]
fn add_does_not_record_a_route_it_could_not_wire() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    run(router(home).args(["add", "github.com", "first", "--token", "ghp_a"]));

    // Make the generated gitconfig unwritable by replacing it with a directory.
    let out = router(home).args(["doctor"]).output().unwrap();
    let generated = String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| {
            l.split_once("Generated config: ")
                .map(|(_, p)| p.to_string())
        })
        .expect("doctor should report the generated config path");
    std::fs::remove_file(&generated).unwrap();
    std::fs::create_dir(&generated).unwrap();

    let out = router(home)
        .args(["add", "github.com", "second", "--token", "ghp_b"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "add should fail when it cannot wire");

    let listed = run(router(home).arg("list"));
    assert!(
        !listed.contains("github.com/second"),
        "config.toml recorded a route whose gitconfig was never written: {listed}"
    );
}
