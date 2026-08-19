use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_torudo")
}

fn fresh_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(name);
    fs::remove_dir_all(&dir).ok();
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, args: &[&str]) -> std::process::Output {
    let mut full = vec!["--todotxt-dir", dir.to_str().unwrap()];
    full.extend_from_slice(args);
    Command::new(bin())
        .args(&full)
        .output()
        .expect("failed to run torudo")
}

fn stdout_of(output: &std::process::Output) -> String {
    assert!(
        output.status.success(),
        "non-zero exit: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn show_returns_the_item_with_its_detail_md() {
    let dir = fresh_dir("torudo_it_show_with_md");
    fs::write(dir.join("todo.txt"), "Fix the spec id:aaa\n").unwrap();
    fs::create_dir_all(dir.join("todos")).unwrap();
    fs::write(dir.join("todos/aaa.md"), "- [x] done\n- [ ] todo\n").unwrap();

    let out = stdout_of(&run(&dir, &["show", "aaa"]));
    let json: serde_json::Value = serde_json::from_str(out.trim()).unwrap();

    assert_eq!(json["id"], "aaa");
    assert_eq!(json["title"], "Fix the spec");
    assert_eq!(json["md"], "- [x] done\n- [ ] todo\n");
}

#[test]
fn show_reports_the_mode_it_found_the_item_in() {
    let dir = fresh_dir("torudo_it_show_mode");
    fs::write(dir.join("waiting.txt"), "Reply from bob id:bbb\n").unwrap();

    let out = stdout_of(&run(&dir, &["show", "bbb"]));
    let json: serde_json::Value = serde_json::from_str(out.trim()).unwrap();

    assert_eq!(json["mode"], "waiting");
}

#[test]
fn show_omits_the_md_key_when_there_is_no_detail_file() {
    let dir = fresh_dir("torudo_it_show_no_md");
    fs::write(dir.join("todo.txt"), "Fix the spec id:aaa\n").unwrap();

    let out = stdout_of(&run(&dir, &["show", "aaa"]));
    let json: serde_json::Value = serde_json::from_str(out.trim()).unwrap();

    assert!(
        json.get("md").is_none(),
        "a missing detail file drops the key instead of printing null: {json}"
    );
}

#[test]
fn show_finds_a_completed_item_in_the_archive() {
    let dir = fresh_dir("torudo_it_show_done");
    fs::write(dir.join("done.txt"), "x 2026-08-19 Old thing id:zzz\n").unwrap();

    let out = stdout_of(&run(&dir, &["show", "zzz"]));
    let json: serde_json::Value = serde_json::from_str(out.trim()).unwrap();

    assert_eq!(json["id"], "zzz");
    assert_eq!(json["mode"], "done");
}

#[test]
fn show_prefers_a_live_mode_over_the_archive() {
    let dir = fresh_dir("torudo_it_show_live_wins");
    fs::write(dir.join("todo.txt"), "Fix the spec id:aaa\n").unwrap();
    fs::write(dir.join("done.txt"), "x 2026-08-19 Fix the spec id:aaa\n").unwrap();

    let out = stdout_of(&run(&dir, &["show", "aaa"]));
    let json: serde_json::Value = serde_json::from_str(out.trim()).unwrap();

    assert_eq!(json["mode"], "todo", "the actionable copy wins: {json}");
}

#[test]
fn show_fails_on_an_unknown_id() {
    let dir = fresh_dir("torudo_it_show_unknown");
    fs::write(dir.join("todo.txt"), "Fix the spec id:aaa\n").unwrap();

    let output = run(&dir, &["show", "nope"]);

    assert!(!output.status.success(), "an unknown id is an error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("nope"), "stderr names the id: {stderr}");
}
