use std::process::Command;

fn git_router() -> Command {
    Command::new(env!("CARGO_BIN_EXE_git-router"))
}

fn with_config_dir<'a>(cmd: &'a mut Command, dir: &std::path::Path) -> &'a mut Command {
    cmd.env("HOME", dir)
}

#[test]
fn add_rejects_injection_in_user_name() {
    let tmp = tempfile::tempdir().unwrap();
    let output = with_config_dir(&mut git_router(), tmp.path())
        .args([
            "add",
            "github.com",
            "testorg",
            "--ssh-key",
            "~/.ssh/test.pub",
            "--user-name",
            "Alice\n[core]\n    sshCommand = evil",
        ])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "should reject injection in user_name"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsafe"), "stderr: {stderr}");
}

#[test]
fn add_rejects_injection_in_host() {
    let tmp = tempfile::tempdir().unwrap();
    let output = with_config_dir(&mut git_router(), tmp.path())
        .args([
            "add",
            "github.com\n[evil]",
            "testorg",
            "--ssh-key",
            "~/.ssh/test.pub",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success(), "should reject injection in host");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsafe"), "stderr: {stderr}");
}

#[test]
fn completions_generate() {
    let output = git_router().args(["completions", "zsh"]).output().unwrap();
    assert!(
        output.status.success(),
        "completions failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("#compdef git-router"),
        "should produce valid zsh completion: {stdout}"
    );
}

#[test]
fn init_writes_gitconfig() {
    let tmp = tempfile::tempdir().unwrap();
    let tmp_path = tmp.path();

    let output = with_config_dir(&mut git_router(), tmp_path)
        .args(["init"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let gitconfig = std::fs::read_to_string(tmp_path.join(".gitconfig")).unwrap();
    assert!(
        gitconfig.contains("sshCommand = git-router ssh-wrap"),
        "missing core.sshCommand: {gitconfig}"
    );
    assert!(
        !gitconfig.contains("helper = git-router credential-helper"),
        "init must not write a global credential.helper; git cannot invoke it: {gitconfig}"
    );
    assert!(
        gitconfig.contains("git-router.gitconfig"),
        "missing include.path for generated config: {gitconfig}"
    );

    let output = with_config_dir(&mut git_router(), tmp_path)
        .args(["init"])
        .output()
        .unwrap();
    assert!(output.status.success(), "second init failed");

    let gitconfig = std::fs::read_to_string(tmp_path.join(".gitconfig")).unwrap();
    assert_eq!(
        gitconfig
            .matches("sshCommand = git-router ssh-wrap")
            .count(),
        1,
        "init should be idempotent: {gitconfig}"
    );
}

#[test]
fn add_list_remove_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let tmp_path = tmp.path();

    let output = with_config_dir(&mut git_router(), tmp_path)
        .args([
            "add",
            "github.com",
            "testorg",
            "--ssh-key",
            "~/.ssh/test.pub",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output = with_config_dir(&mut git_router(), tmp_path)
        .args(["list"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("github.com/testorg"),
        "list output: {stdout}"
    );
    assert!(
        stdout.contains("ssh_key=~/.ssh/test.pub"),
        "list output: {stdout}"
    );

    let output = with_config_dir(&mut git_router(), tmp_path)
        .args([
            "add",
            "github.com",
            "otherorg",
            "--ssh-key",
            "~/.ssh/other.pub",
            "--token",
            "ghp_test123",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = with_config_dir(&mut git_router(), tmp_path)
        .args(["list"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("github.com/testorg"),
        "list output: {stdout}"
    );
    assert!(
        stdout.contains("github.com/otherorg"),
        "list output: {stdout}"
    );
    assert!(
        stdout.contains("token=***"),
        "token should be masked: {stdout}"
    );

    let output = with_config_dir(&mut git_router(), tmp_path)
        .args(["add", "github.com", "testorg", "--token", "ghp_updated"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Updated"), "should say updated: {stdout}");

    let output = with_config_dir(&mut git_router(), tmp_path)
        .args(["list"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let testorg_line = stdout
        .lines()
        .find(|l| l.contains("github.com/testorg"))
        .expect("testorg should still exist");
    assert!(
        testorg_line.contains("ssh_key=~/.ssh/test.pub"),
        "ssh_key should be preserved after token update: {testorg_line}"
    );
    assert!(
        testorg_line.contains("token=***"),
        "token should be present after update: {testorg_line}"
    );

    let output = with_config_dir(&mut git_router(), tmp_path)
        .args(["remove", "github.com", "testorg"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Removed"), "remove output: {stdout}");

    let output = with_config_dir(&mut git_router(), tmp_path)
        .args(["list"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("github.com/testorg"),
        "should be removed: {stdout}"
    );
    assert!(
        stdout.contains("github.com/otherorg"),
        "should remain: {stdout}"
    );

    let output = with_config_dir(&mut git_router(), tmp_path)
        .args(["add", "github.com", "failorg"])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "should reject add with no key or token"
    );

    let output = with_config_dir(&mut git_router(), tmp_path)
        .args(["remove", "github.com", "nonexistent"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No route found"), "output: {stdout}");
}
