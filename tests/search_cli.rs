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
fn search_spans_every_mode_file() {
    let dir = fresh_dir("torudo_it_search_all_modes");
    fs::write(dir.join("inbox.txt"), "Buy milk id:aaa\n").unwrap();
    fs::write(dir.join("someday.txt"), "Buy a boat id:zzz\n").unwrap();

    let out = stdout_of(&run(&dir, &["search", "buy", "--json"]));
    let hits: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(hits.as_array().unwrap().len(), 2);
    assert_eq!(hits[0]["id"], "aaa");
    assert_eq!(hits[0]["mode"], "inbox");
    assert_eq!(hits[1]["id"], "zzz");
    assert_eq!(hits[1]["mode"], "someday");
}

#[test]
fn search_ignores_case() {
    let dir = fresh_dir("torudo_it_search_case");
    fs::write(dir.join("todo.txt"), "Fix FLAKY test id:aaa\n").unwrap();

    let out = stdout_of(&run(&dir, &["search", "flaky", "--json"]));
    let hits: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["matched"], "title");
}

#[test]
fn search_reaches_into_the_detail_md_file() {
    let dir = fresh_dir("torudo_it_search_md");
    fs::write(dir.join("ref.txt"), "Onboarding notes id:aaa\n").unwrap();
    fs::create_dir_all(dir.join("todos")).unwrap();
    fs::write(
        dir.join("todos/aaa.md"),
        "# Notes\n\nask about VPN access\n",
    )
    .unwrap();

    let out = stdout_of(&run(&dir, &["search", "vpn", "--json"]));
    let hits: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["id"], "aaa");
    assert_eq!(hits[0]["matched"], "md");
}

#[test]
fn search_reports_both_when_the_title_and_the_md_match() {
    let dir = fresh_dir("torudo_it_search_both");
    fs::write(dir.join("todo.txt"), "Fix flaky test id:aaa\n").unwrap();
    fs::create_dir_all(dir.join("todos")).unwrap();
    fs::write(dir.join("todos/aaa.md"), "the flaky one is spec/foo\n").unwrap();

    let out = stdout_of(&run(&dir, &["search", "flaky", "--json"]));
    let hits: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(hits[0]["matched"], "both");
}

#[test]
fn search_matches_a_project_tag() {
    let dir = fresh_dir("torudo_it_search_project_tag");
    fs::write(
        dir.join("todo.txt"),
        "Fix the flaky spec +keihi @office time:short id:aaa\n",
    )
    .unwrap();

    let out = stdout_of(&run(&dir, &["search", "keihi", "--json"]));
    let hits: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["id"], "aaa");
    assert_eq!(hits[0]["matched"], "title");
}

#[test]
fn search_matches_a_context_tag() {
    let dir = fresh_dir("torudo_it_search_context_tag");
    fs::write(
        dir.join("todo.txt"),
        "Fix the flaky spec +keihi @office time:short id:aaa\n",
    )
    .unwrap();

    let out = stdout_of(&run(&dir, &["search", "office", "--json"]));
    let hits: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["id"], "aaa");
}

#[test]
fn search_never_matches_a_key_value_tag() {
    let dir = fresh_dir("torudo_it_search_key_value");
    fs::write(
        dir.join("todo.txt"),
        "Fix the flaky spec +keihi @office time:short id:aaa\n",
    )
    .unwrap();

    // `time:short` and `id:aaa` are metadata, not something the user searches for.
    let out = stdout_of(&run(&dir, &["search", "short", "--json"]));
    let hits: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(hits, serde_json::json!([]));
}

#[test]
fn search_never_looks_at_done_txt() {
    let dir = fresh_dir("torudo_it_search_done");
    fs::write(dir.join("done.txt"), "x 2026-08-15 Buy milk id:done-1\n").unwrap();
    fs::write(dir.join("inbox.txt"), "Buy milk id:aaa\n").unwrap();

    let out = stdout_of(&run(&dir, &["search", "milk", "--json"]));
    let hits: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["id"], "aaa");
}

#[test]
fn search_with_a_mode_narrows_the_sweep() {
    let dir = fresh_dir("torudo_it_search_mode");
    fs::write(dir.join("inbox.txt"), "Buy milk id:aaa\n").unwrap();
    fs::write(dir.join("ref.txt"), "Buy milk elsewhere id:bbb\n").unwrap();

    let out = stdout_of(&run(&dir, &["search", "milk", "--json", "--mode", "ref"]));
    let hits: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["id"], "bbb");
    assert_eq!(hits[0]["mode"], "ref");
}

#[test]
fn search_without_json_puts_the_id_first_on_every_line() {
    let dir = fresh_dir("torudo_it_search_plain");
    fs::write(dir.join("inbox.txt"), "Buy milk id:aaa-111\n").unwrap();
    fs::write(dir.join("todo.txt"), "Buy a boat\n").unwrap();

    let out = stdout_of(&run(&dir, &["search", "buy"]));

    assert_eq!(
        out,
        "aaa-111 [inbox] Buy milk (title)\n- [todo] Buy a boat (title)\n"
    );
}

#[test]
fn search_with_no_hits_prints_an_empty_json_array_and_exits_zero() {
    let dir = fresh_dir("torudo_it_search_empty");
    fs::write(dir.join("inbox.txt"), "Buy milk id:aaa\n").unwrap();

    let out = stdout_of(&run(&dir, &["search", "nothing-here", "--json"]));
    let hits: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(hits, serde_json::json!([]));
}

#[test]
fn search_succeeds_when_no_mode_file_exists_yet() {
    let dir = fresh_dir("torudo_it_search_no_files");

    let out = stdout_of(&run(&dir, &["search", "milk", "--json"]));
    let hits: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(hits, serde_json::json!([]));
}

#[test]
fn search_with_an_empty_query_fails() {
    let dir = fresh_dir("torudo_it_search_blank");
    fs::write(dir.join("inbox.txt"), "Buy milk id:aaa\n").unwrap();

    let output = run(&dir, &["search", ""]);

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("query must not be empty"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn search_with_md_returns_the_body_of_an_md_hit() {
    let dir = fresh_dir("torudo_it_search_with_md_body");
    fs::write(dir.join("todo.txt"), "Fix the spec id:aaa\n").unwrap();
    fs::create_dir_all(dir.join("todos")).unwrap();
    fs::write(dir.join("todos/aaa.md"), "the flaky one is spec/foo\n").unwrap();

    let out = stdout_of(&run(&dir, &["search", "flaky", "--json", "--with-md"]));
    let hits: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["matched"], "md");
    assert_eq!(hits[0]["md"], "the flaky one is spec/foo\n");
}

#[test]
fn search_with_md_omits_the_key_for_a_title_hit_without_an_md_file() {
    let dir = fresh_dir("torudo_it_search_with_md_no_file");
    fs::write(dir.join("todo.txt"), "Fix the flaky spec id:aaa\n").unwrap();

    let out = stdout_of(&run(&dir, &["search", "flaky", "--json", "--with-md"]));
    let hits: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["matched"], "title");
    assert!(
        hits[0].get("md").is_none(),
        "a missing detail file drops the key instead of printing null: {}",
        hits[0]
    );
}

#[test]
fn search_json_without_with_md_has_no_md_key() {
    let dir = fresh_dir("torudo_it_search_json_no_md");
    fs::write(dir.join("todo.txt"), "Fix the spec id:aaa\n").unwrap();
    fs::create_dir_all(dir.join("todos")).unwrap();
    fs::write(dir.join("todos/aaa.md"), "the flaky one is spec/foo\n").unwrap();

    let out = stdout_of(&run(&dir, &["search", "flaky", "--json"]));
    let hits: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert!(
        hits[0].get("md").is_none(),
        "the md body is opt-in even for an md hit: {}",
        hits[0]
    );
}

#[test]
fn search_with_md_requires_json() {
    let dir = fresh_dir("torudo_it_search_with_md_requires_json");
    fs::write(dir.join("todo.txt"), "Fix the flaky spec id:aaa\n").unwrap();

    let output = run(&dir, &["search", "flaky", "--with-md"]);

    assert!(!output.status.success(), "the flag pair is rejected");
    assert!(output.stdout.is_empty(), "nothing is printed on rejection");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--json"), "stderr names --json: {stderr}");
}
