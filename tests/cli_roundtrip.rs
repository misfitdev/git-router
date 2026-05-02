use std::process::Command;

fn git_router() -> Command {
    Command::new(env!("CARGO_BIN_EXE_git-router"))
}

fn with_config_dir<'a>(cmd: &'a mut Command, dir: &std::path::Path) -> &'a mut Command {
    // XDG_CONFIG_HOME overrides dirs::config_dir() on Linux.
    // On macOS dirs uses ~/Library/Application Support, but we can
    // override via HOME to an isolated tmpdir.
    cmd.env("HOME", dir)
}

#[test]
fn add_list_remove_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let tmp_path = tmp.path();

    // Create the config directory structure that dirs::config_dir() will resolve
    // On macOS: HOME/Library/Application Support/git-router/
    // On Linux: HOME/.config/git-router/
    // We set HOME to tmp, so both resolve inside tmp.

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
    assert!(output.status.success(), "add failed: {}", String::from_utf8_lossy(&output.stderr));

    // List should show the route
    let output = with_config_dir(&mut git_router(), tmp_path)
        .args(["list"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("github.com/testorg"), "list output: {stdout}");
    assert!(stdout.contains("ssh_key=~/.ssh/test.pub"), "list output: {stdout}");

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
    assert!(stdout.contains("github.com/testorg"), "list output: {stdout}");
    assert!(stdout.contains("github.com/otherorg"), "list output: {stdout}");
    assert!(stdout.contains("token=***"), "token should be masked: {stdout}");

    // Update existing route
    let output = with_config_dir(&mut git_router(), tmp_path)
        .args([
            "add",
            "github.com",
            "testorg",
            "--token",
            "ghp_updated",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Updated"), "should say updated: {stdout}");

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
    assert!(!stdout.contains("github.com/testorg"), "should be removed: {stdout}");
    assert!(stdout.contains("github.com/otherorg"), "should remain: {stdout}");

    // Remove non-existent route
    let output = with_config_dir(&mut git_router(), tmp_path)
        .args(["remove", "github.com", "nonexistent"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No route found"), "output: {stdout}");
}
