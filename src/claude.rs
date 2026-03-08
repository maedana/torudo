use log::debug;
use std::error::Error;
use std::process::Command;

pub fn detect() -> bool {
    Command::new("claude")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn git_root() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

pub fn launch(prompt: &str, permission_mode: &str, worktree_name: &str) -> Result<(), Box<dyn Error>> {
    let mut cmd = Command::new("tmux");
    cmd.args([
        "new-window",
        "-d",
        "-n",
        worktree_name,
        "--",
        "claude",
        "--worktree",
        worktree_name,
        "--permission-mode",
        permission_mode,
        prompt,
    ]);

    if let Some(root) = git_root() {
        debug!("Setting current_dir to git root: {root}");
        cmd.current_dir(&root);
    }

    let mut child = cmd.spawn()?;
    // Avoid zombie processes
    std::thread::spawn(move || {
        let _ = child.wait();
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_root_returns_some() {
        // This test runs inside a git repository, so git_root should return Some
        let root = git_root();
        assert!(root.is_some(), "git_root() should return Some in a git repo");
        let root_path = root.unwrap();
        assert!(!root_path.is_empty());
    }
}
