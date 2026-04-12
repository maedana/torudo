use log::debug;
use std::error::Error;
use std::process::Command;

pub fn detect() -> bool {
    Command::new("claude")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

pub fn launch(
    prompt: &str,
    permission_mode: &str,
    worktree_name: &str,
    cwd: &str,
) -> Result<(), Box<dyn Error>> {
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

    debug!("Setting current_dir to: {cwd}");
    cmd.current_dir(cwd);

    let mut child = cmd.spawn()?;
    // Avoid zombie processes
    std::thread::spawn(move || {
        let _ = child.wait();
    });

    Ok(())
}
