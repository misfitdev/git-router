use std::process::Command;

fn git_router() -> Command {
    Command::new(env!("CARGO_BIN_EXE_git-router"))
}

fn with_config_dir<'a>(cmd: &'a mut Command, dir: &std::path::Path) -> &'a mut Command {
    cmd.env("HOME", dir)
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
fn add_list_remove_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let tmp_path = tmp.path();

    // Add a route
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

    // List should show the route
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

    // Add a second route with a token
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

    // List should show both routes
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

    // Update existing route
    let output = with_config_dir(&mut git_router(), tmp_path)
        .args(["add", "github.com", "testorg", "--token", "ghp_updated"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Updated"), "should say updated: {stdout}");

    // Verify partial update: adding --token to testorg preserved its ssh_key
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

    // Remove the first route
    let output = with_config_dir(&mut git_router(), tmp_path)
        .args(["remove", "github.com", "testorg"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Removed"), "remove output: {stdout}");

    // List should only show the second route
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

    // Add with neither --ssh-key nor --token should fail
    let output = with_config_dir(&mut git_router(), tmp_path)
        .args(["add", "github.com", "failorg"])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "should reject add with no key or token"
    );

    // Remove non-existent route
    let output = with_config_dir(&mut git_router(), tmp_path)
        .args(["remove", "github.com", "nonexistent"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No route found"), "output: {stdout}");
}
