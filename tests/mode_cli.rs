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
fn list_json_returns_items_of_the_mode() {
    let dir = fresh_dir("torudo_it_list_json");
    fs::write(
        dir.join("inbox.txt"),
        "Buy milk +grocery @home id:aaa\n(A) Call bob id:bbb\n",
    )
    .unwrap();

    let output = run(&dir, &["inbox", "list", "--json"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    let items = json.as_array().expect("stdout is a JSON array");
    assert_eq!(items.len(), 2);
    let titles: Vec<&str> = items.iter().map(|i| i["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"Buy milk"));
    assert!(titles.contains(&"Call bob"));
    let ids: Vec<&str> = items.iter().map(|i| i["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"aaa"));
    assert!(ids.contains(&"bbb"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn list_reads_the_file_matching_the_mode() {
    let dir = fresh_dir("torudo_it_list_per_mode");
    fs::write(dir.join("inbox.txt"), "In the inbox id:i1\n").unwrap();
    fs::write(dir.join("todo.txt"), "In the todo id:t1\n").unwrap();
    fs::write(dir.join("waiting.txt"), "In the waiting id:w1\n").unwrap();

    for (mode, expected) in [
        ("inbox", "In the inbox"),
        ("todo", "In the todo"),
        ("waiting", "In the waiting"),
    ] {
        let output = run(&dir, &[mode, "list", "--json"]);
        let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();
        let items = json.as_array().unwrap();
        assert_eq!(items.len(), 1, "mode {mode} should hold one item");
        assert_eq!(items[0]["title"], expected);
    }

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn list_on_missing_file_returns_empty_array() {
    let dir = fresh_dir("torudo_it_list_missing");

    let output = run(&dir, &["someday", "list", "--json"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json, serde_json::json!([]));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_writes_time_and_energy_tags() {
    let dir = fresh_dir("torudo_it_set_tags");
    fs::write(dir.join("todo.txt"), "Write docs +torudo id:t1\n").unwrap();

    let output = run(
        &dir,
        &["todo", "set", "t1", "--time", "short", "--energy", "low"],
    );
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json["id"], "t1");
    assert_eq!(json["key_values"]["time"], "short");
    assert_eq!(json["key_values"]["energy"], "low");

    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(todo.contains("time:short"), "todo.txt: {todo}");
    assert!(todo.contains("energy:low"), "todo.txt: {todo}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_replaces_an_existing_value() {
    let dir = fresh_dir("torudo_it_set_replace");
    fs::write(dir.join("todo.txt"), "Write docs id:t1 time:long\n").unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--time", "short"]);
    stdout_of(&output);

    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(todo.contains("time:short"));
    assert!(!todo.contains("time:long"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_accepts_a_single_option() {
    let dir = fresh_dir("torudo_it_set_single");
    fs::write(dir.join("inbox.txt"), "Capture me id:i1\n").unwrap();

    let output = run(&dir, &["inbox", "set", "i1", "--energy", "high"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json["key_values"]["energy"], "high");
    assert!(json["key_values"].get("time").is_none());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_on_unknown_id_fails() {
    let dir = fresh_dir("torudo_it_set_unknown");
    fs::write(dir.join("todo.txt"), "Write docs id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "nope", "--time", "short"]);

    assert!(!output.status.success(), "unknown id should exit non-zero");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("nope"),
        "stderr should name the id: {stderr}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_without_any_option_fails() {
    let dir = fresh_dir("torudo_it_set_no_option");
    fs::write(dir.join("todo.txt"), "Write docs id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "t1"]);

    assert!(!output.status.success(), "no-op set should exit non-zero");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_rejects_an_invalid_value() {
    let dir = fresh_dir("torudo_it_set_invalid");
    fs::write(dir.join("todo.txt"), "Write docs id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--time", "huge"]);

    assert!(
        !output.status.success(),
        "invalid value should exit non-zero"
    );

    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(!todo.contains("time:"), "nothing should be written: {todo}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn promote_moves_an_item_between_modes() {
    let dir = fresh_dir("torudo_it_promote");
    fs::write(
        dir.join("inbox.txt"),
        "Capture me +torudo id:i1\nStay here id:i2\n",
    )
    .unwrap();

    let output = run(&dir, &["inbox", "promote", "i1", "--to", "todo"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json["id"], "i1");
    assert_eq!(json["title"], "Capture me");

    let inbox = fs::read_to_string(dir.join("inbox.txt")).unwrap();
    assert!(!inbox.contains("id:i1"), "moved out of inbox: {inbox}");
    assert!(inbox.contains("id:i2"), "others stay: {inbox}");

    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(todo.contains("Capture me"), "landed in todo: {todo}");
    assert!(todo.contains("+torudo"), "tags survive the move: {todo}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn promote_creates_the_target_file_when_missing() {
    let dir = fresh_dir("torudo_it_promote_lazy_file");
    fs::write(dir.join("todo.txt"), "Someday maybe id:t1\n").unwrap();

    let output = run(&dir, &["todo", "promote", "t1", "--to", "someday"]);
    stdout_of(&output);

    let someday = fs::read_to_string(dir.join("someday.txt")).unwrap();
    assert!(someday.contains("Someday maybe"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn promote_on_unknown_id_fails() {
    let dir = fresh_dir("torudo_it_promote_unknown");
    fs::write(dir.join("inbox.txt"), "Capture me id:i1\n").unwrap();

    let output = run(&dir, &["inbox", "promote", "nope", "--to", "todo"]);

    assert!(!output.status.success(), "unknown id should exit non-zero");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("nope"),
        "stderr should name the id: {stderr}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn promote_to_the_same_mode_fails() {
    let dir = fresh_dir("torudo_it_promote_same");
    fs::write(dir.join("inbox.txt"), "Capture me id:i1\n").unwrap();

    let output = run(&dir, &["inbox", "promote", "i1", "--to", "inbox"]);

    assert!(
        !output.status.success(),
        "same-mode move should exit non-zero"
    );
    let inbox = fs::read_to_string(dir.join("inbox.txt")).unwrap();
    assert!(inbox.contains("id:i1"), "item is untouched: {inbox}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn complete_moves_the_item_to_done() {
    let dir = fresh_dir("torudo_it_complete");
    fs::write(
        dir.join("todo.txt"),
        "2026-08-01 Write docs +torudo id:t1\nStay here id:t2\n",
    )
    .unwrap();

    let output = run(&dir, &["todo", "complete", "t1"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json["id"], "t1");
    assert_eq!(json["completed"], true);
    assert!(json["completion_date"].is_string());

    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(!todo.contains("id:t1"), "gone from todo.txt: {todo}");
    assert!(todo.contains("id:t2"), "others stay: {todo}");

    let done = fs::read_to_string(dir.join("done.txt")).unwrap();
    assert!(done.contains("Write docs"), "landed in done.txt: {done}");
    assert!(done.starts_with("x "), "marked complete: {done}");
    assert!(
        done.contains("2026-08-01"),
        "creation date survives: {done}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn complete_works_from_waiting() {
    let dir = fresh_dir("torudo_it_complete_waiting");
    fs::write(dir.join("waiting.txt"), "Waiting on bob id:w1\n").unwrap();

    let output = run(&dir, &["waiting", "complete", "w1"]);
    stdout_of(&output);

    let done = fs::read_to_string(dir.join("done.txt")).unwrap();
    assert!(done.contains("Waiting on bob"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn complete_from_inbox_is_refused() {
    let dir = fresh_dir("torudo_it_complete_inbox");
    fs::write(dir.join("inbox.txt"), "Capture me id:i1\n").unwrap();

    let output = run(&dir, &["inbox", "complete", "i1"]);

    assert!(
        !output.status.success(),
        "inbox complete should exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Todo"),
        "stderr should point at promoting to Todo: {stderr}"
    );

    let inbox = fs::read_to_string(dir.join("inbox.txt")).unwrap();
    assert!(inbox.contains("id:i1"), "item is untouched: {inbox}");
    assert!(
        !dir.join("done.txt").exists(),
        "nothing written to done.txt"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn complete_on_unknown_id_fails() {
    let dir = fresh_dir("torudo_it_complete_unknown");
    fs::write(dir.join("todo.txt"), "Write docs id:t1\n").unwrap();

    let output = run(&dir, &["todo", "complete", "nope"]);

    assert!(!output.status.success(), "unknown id should exit non-zero");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("nope"),
        "stderr should name the id: {stderr}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn list_without_json_prints_one_line_per_item() {
    let dir = fresh_dir("torudo_it_list_plain");
    fs::write(
        dir.join("todo.txt"),
        "Buy milk +grocery id:aaa\nCall bob id:bbb\n",
    )
    .unwrap();

    let output = run(&dir, &["todo", "list"]);
    let stdout = stdout_of(&output);

    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 2);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("Buy milk") && l.contains("aaa"))
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("Call bob") && l.contains("bbb"))
    );

    fs::remove_dir_all(&dir).ok();
}
