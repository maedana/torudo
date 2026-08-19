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

/// A date `offset` days away from today. Recurrence is resolved against the day
/// the command runs, so fixtures that exercise it are seeded from today rather
/// than from a fixed calendar date that would rot.
fn days_from_today(offset: i64) -> chrono::NaiveDate {
    chrono::Local::now().date_naive() + chrono::Duration::days(offset)
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
fn set_writes_a_recurrence_tag() {
    let dir = fresh_dir("torudo_it_set_rec");
    fs::write(dir.join("todo.txt"), "Pay rent id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--rec", "+1m"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json["key_values"]["rec"], "+1m");
    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(todo.contains("rec:+1m"), "tag is written: {todo}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_rec_none_clears_the_recurrence_tag() {
    let dir = fresh_dir("torudo_it_set_rec_none");
    fs::write(
        dir.join("todo.txt"),
        "2026-08-01 Pay rent +home due:2026-09-01 rec:+1m id:t1\n",
    )
    .unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--rec", "none"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert!(
        json["key_values"].get("rec").is_none(),
        "rec is gone from the JSON: {json}"
    );
    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(!todo.contains("rec:"), "rec: token is dropped: {todo}");
    assert!(todo.contains("+home"), "the project survives: {todo}");
    assert!(todo.contains("due:2026-09-01"), "due: survives: {todo}");
    assert!(todo.contains("id:t1"), "id: survives: {todo}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_rec_none_is_case_insensitive() {
    let dir = fresh_dir("torudo_it_set_rec_none_case");
    fs::write(
        dir.join("todo.txt"),
        "2026-08-01 Pay rent +home due:2026-09-01 rec:+1m id:t1\n",
    )
    .unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--rec", "None"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert!(
        json["key_values"].get("rec").is_none(),
        "rec is gone from the JSON: {json}"
    );
    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    // Not just "rec:+1m": a parser that accepted the keyword without lowercasing
    // it would write the keyword back out as rec:None.
    assert!(!todo.contains("rec:"), "rec: token is dropped: {todo}");
    assert!(todo.contains("+home"), "the project survives: {todo}");
    assert!(todo.contains("due:2026-09-01"), "due: survives: {todo}");
    assert!(todo.contains("id:t1"), "id: survives: {todo}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_rec_none_succeeds_on_an_item_without_a_recurrence_tag() {
    let dir = fresh_dir("torudo_it_set_rec_none_absent");
    fs::write(dir.join("todo.txt"), "Pay rent +home id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--rec", "none"]);

    assert!(
        output.status.success(),
        "clearing an absent rec: is not an error: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert_eq!(todo, "Pay rent +home id:t1\n");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_rejects_an_invalid_recurrence_pattern() {
    let dir = fresh_dir("torudo_it_set_rec_invalid");
    fs::write(dir.join("todo.txt"), "Pay rent id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--rec", "banana"]);

    assert!(
        !output.status.success(),
        "an invalid pattern should exit non-zero"
    );
    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(!todo.contains("rec:"), "nothing should be written: {todo}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_writes_a_threshold_tag() {
    let dir = fresh_dir("torudo_it_set_threshold");
    fs::write(dir.join("todo.txt"), "Write docs id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--threshold", "2026-08-24"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json["key_values"]["t"], "2026-08-24");
    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(todo.contains("t:2026-08-24"), "tag is written: {todo}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_writes_a_due_tag() {
    let dir = fresh_dir("torudo_it_set_due");
    fs::write(dir.join("todo.txt"), "Write docs id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--due", "2026-08-24"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json["key_values"]["due"], "2026-08-24");
    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(todo.contains("due:2026-08-24"), "tag is written: {todo}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_replaces_an_existing_threshold() {
    let dir = fresh_dir("torudo_it_set_threshold_replace");
    fs::write(
        dir.join("todo.txt"),
        "2026-08-01 Write docs t:2026-08-10 id:t1\n",
    )
    .unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--threshold", "2026-08-24"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json["key_values"]["t"], "2026-08-24");
    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(todo.contains("t:2026-08-24"), "the new value lands: {todo}");
    assert!(
        !todo.contains("t:2026-08-10"),
        "the old value is gone: {todo}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_threshold_none_clears_the_threshold_tag() {
    let dir = fresh_dir("torudo_it_set_threshold_none");
    fs::write(
        dir.join("todo.txt"),
        "2026-08-01 Pay rent +home t:2026-08-20 due:2026-09-01 rec:+1m id:t1\n",
    )
    .unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--threshold", "none"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert!(
        json["key_values"].get("t").is_none(),
        "t is gone from the JSON: {json}"
    );
    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    // A bare "t:" would also match a word in the description, so the absence is
    // checked on the leading space that separates the tag from what precedes it.
    assert!(!todo.contains(" t:"), "t: token is dropped: {todo}");
    assert!(todo.contains("+home"), "the project survives: {todo}");
    assert!(todo.contains("due:2026-09-01"), "due: survives: {todo}");
    assert!(todo.contains("rec:+1m"), "rec: survives: {todo}");
    assert!(todo.contains("id:t1"), "id: survives: {todo}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_threshold_none_is_case_insensitive() {
    let dir = fresh_dir("torudo_it_set_threshold_none_case");
    fs::write(
        dir.join("todo.txt"),
        "2026-08-01 Pay rent +home t:2026-08-20 rec:+1m id:t1\n",
    )
    .unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--threshold", "NONE"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert!(
        json["key_values"].get("t").is_none(),
        "t is gone from the JSON: {json}"
    );
    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    // A bare "t:" would also match a word in the description, so the absence is
    // checked on the leading space that separates the tag from what precedes it.
    // This also catches a t:NONE written back out by a parser that took the
    // keyword without lowercasing it.
    assert!(!todo.contains(" t:"), "t: token is dropped: {todo}");
    assert!(todo.contains("+home"), "the project survives: {todo}");
    assert!(todo.contains("rec:+1m"), "rec: survives: {todo}");
    assert!(todo.contains("id:t1"), "id: survives: {todo}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_due_none_clears_the_due_tag() {
    let dir = fresh_dir("torudo_it_set_due_none");
    fs::write(
        dir.join("todo.txt"),
        "2026-08-01 Pay rent +home t:2026-08-20 due:2026-09-01 rec:+1m id:t1\n",
    )
    .unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--due", "none"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert!(
        json["key_values"].get("due").is_none(),
        "due is gone from the JSON: {json}"
    );
    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(!todo.contains("due:"), "due: token is dropped: {todo}");
    assert!(todo.contains("+home"), "the project survives: {todo}");
    assert!(todo.contains("t:2026-08-20"), "t: survives: {todo}");
    assert!(todo.contains("rec:+1m"), "rec: survives: {todo}");
    assert!(todo.contains("id:t1"), "id: survives: {todo}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_threshold_none_succeeds_on_an_item_without_a_threshold_tag() {
    let dir = fresh_dir("torudo_it_set_threshold_none_absent");
    fs::write(dir.join("todo.txt"), "Pay rent +home id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--threshold", "none"]);

    assert!(
        output.status.success(),
        "clearing an absent t: is not an error: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert_eq!(todo, "Pay rent +home id:t1\n");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_rejects_a_relative_threshold() {
    let dir = fresh_dir("torudo_it_set_threshold_relative");
    fs::write(dir.join("todo.txt"), "Pay rent id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--threshold", "next monday"]);

    assert!(
        !output.status.success(),
        "a relative date should exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("YYYY-MM-DD"),
        "the error names the format it wants: {stderr}"
    );
    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    // A bare "t:" would also match a word in the description, so the absence is
    // checked on the leading space that separates the tag from what precedes it.
    assert!(!todo.contains(" t:"), "nothing should be written: {todo}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_rejects_an_impossible_due_date() {
    let dir = fresh_dir("torudo_it_set_due_impossible");
    fs::write(dir.join("todo.txt"), "Pay rent id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--due", "2026-02-30"]);

    assert!(
        !output.status.success(),
        "a date that never happens should exit non-zero"
    );
    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(!todo.contains("due:"), "nothing should be written: {todo}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_writes_a_threshold_and_a_due_together() {
    let dir = fresh_dir("torudo_it_set_threshold_and_due");
    fs::write(dir.join("todo.txt"), "Pay rent id:t1\n").unwrap();
    let threshold = days_from_today(7).to_string();
    let due = days_from_today(14).to_string();

    let output = run(
        &dir,
        &[
            "todo",
            "set",
            "t1",
            "--threshold",
            &threshold,
            "--due",
            &due,
            "--time",
            "short",
        ],
    );
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json["key_values"]["t"], threshold);
    assert_eq!(json["key_values"]["due"], due);
    assert_eq!(json["key_values"]["time"], "short");
    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(todo.contains(&format!("t:{threshold}")), "t: lands: {todo}");
    assert!(todo.contains(&format!("due:{due}")), "due: lands: {todo}");
    assert!(todo.contains("time:short"), "time: lands: {todo}");

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
fn set_writes_a_priority() {
    let dir = fresh_dir("torudo_it_set_priority");
    fs::write(dir.join("todo.txt"), "Write docs id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--priority", "a"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json["priority"], "A");

    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(todo.starts_with("(A) "), "todo.txt: {todo}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_replaces_an_existing_priority() {
    let dir = fresh_dir("torudo_it_set_priority_replace");
    fs::write(dir.join("todo.txt"), "(A) Write docs id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--priority", "c"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json["priority"], "C");

    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(todo.starts_with("(C) "), "todo.txt: {todo}");
    assert!(!todo.contains("(A)"), "old priority is gone: {todo}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_priority_none_clears_the_priority() {
    let dir = fresh_dir("torudo_it_set_priority_none");
    fs::write(dir.join("todo.txt"), "(X) Write docs id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--priority", "none"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert!(
        json["priority"].is_null(),
        "priority is cleared in JSON: {json}"
    );

    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(!todo.contains("(X)"), "prefix is gone: {todo}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_priority_accepts_an_uppercase_value() {
    let dir = fresh_dir("torudo_it_set_priority_upper");
    fs::write(dir.join("todo.txt"), "Write docs id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--priority", "B"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json["priority"], "B");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_priority_accepts_the_lowest_level() {
    let dir = fresh_dir("torudo_it_set_priority_lowest");
    fs::write(dir.join("todo.txt"), "Write docs id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--priority", "e"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json["priority"], "E");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_rejects_an_invalid_priority() {
    let dir = fresh_dir("torudo_it_set_priority_invalid");
    fs::write(dir.join("todo.txt"), "Write docs id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "t1", "--priority", "z"]);

    assert!(
        !output.status.success(),
        "invalid priority should exit non-zero"
    );
    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert_eq!(todo, "Write docs id:t1\n", "nothing should be written");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_priority_on_unknown_id_fails() {
    let dir = fresh_dir("torudo_it_set_priority_unknown");
    fs::write(dir.join("todo.txt"), "Write docs id:t1\n").unwrap();

    let output = run(&dir, &["todo", "set", "nope", "--priority", "a"]);

    assert!(!output.status.success(), "unknown id should exit non-zero");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("nope"),
        "stderr should name the id: {stderr}"
    );

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
fn complete_spawns_the_next_occurrence() {
    let dir = fresh_dir("torudo_it_complete_rec");
    // The binary reads the clock, so the item is seeded relative to today. Strict
    // `+1w` counts from the item's own due date, three days back, putting the next
    // occurrence four days out; counting from the completion date would give seven.
    let seeded_due = days_from_today(-3);
    let expected_due = days_from_today(4).to_string();
    fs::write(
        dir.join("todo.txt"),
        format!("2026-08-01 Take out the trash +home due:{seeded_due} rec:+1w id:r1\n"),
    )
    .unwrap();

    let output = run(&dir, &["todo", "complete", "r1"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json["id"], "r1");
    let new_id = json["recurrence"]["id"]
        .as_str()
        .expect("the new id is a string");
    assert_ne!(new_id, "r1", "the next occurrence gets its own id");
    assert_eq!(json["recurrence"]["due"], expected_due);

    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(
        todo.contains(&format!("id:{new_id}")),
        "next occurrence lands in todo.txt: {todo}"
    );
    assert!(todo.contains("rec:+1w"), "rec: is carried over: {todo}");
    assert!(
        todo.contains(&format!("due:{expected_due}")),
        "due: moves on: {todo}"
    );
    assert!(!todo.contains("id:r1"), "the old line is gone: {todo}");

    let done = fs::read_to_string(dir.join("done.txt")).unwrap();
    assert!(done.contains("id:r1"), "the old line is archived: {done}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn complete_with_an_invalid_rec_warns_but_still_succeeds() {
    let dir = fresh_dir("torudo_it_complete_rec_invalid");
    fs::write(dir.join("todo.txt"), "Pay rent +home rec:1b id:r2\n").unwrap();

    let output = run(&dir, &["todo", "complete", "r2"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("rec:1b"),
        "stderr names the bad pattern: {stderr}"
    );

    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();
    assert_eq!(json["completed"], true);
    assert_eq!(json["recurrence"]["value"], "1b");
    assert!(json["recurrence"]["error"].is_string());

    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(
        todo.trim().is_empty(),
        "no next occurrence was created: {todo}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn complete_without_a_rec_tag_leaves_the_recurrence_field_out() {
    let dir = fresh_dir("torudo_it_complete_no_rec");
    fs::write(dir.join("todo.txt"), "Write docs id:r3\n").unwrap();

    let output = run(&dir, &["todo", "complete", "r3"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert!(
        json.get("recurrence").is_none(),
        "a plain item has no recurrence field: {json}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn complete_after_clearing_due_falls_back_to_the_completion_date() {
    let dir = fresh_dir("torudo_it_due_none_then_complete");
    // A strict pattern normally counts from the item's own due date. With the due
    // date cleared there is nothing to count from, so `rec:+1w` falls back to the
    // completion date — today — and the next occurrence lands a week out. This is
    // the pre-existing fallback; the test is here so `--due none` cannot break it.
    let seeded_due = days_from_today(-3);
    let expected_due = days_from_today(7).to_string();
    fs::write(
        dir.join("todo.txt"),
        format!("2026-08-01 Pay rent +home due:{seeded_due} rec:+1w id:r4\n"),
    )
    .unwrap();

    let cleared = run(&dir, &["todo", "set", "r4", "--due", "none"]);
    let cleared_json: serde_json::Value = serde_json::from_str(stdout_of(&cleared).trim()).unwrap();
    assert!(
        cleared_json["key_values"].get("due").is_none(),
        "due is gone before the completion: {cleared_json}"
    );

    let output = run(&dir, &["todo", "complete", "r4"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("rec:"),
        "a due-less strict pattern is not warned about: {stderr}"
    );
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json["completed"], true);
    let new_id = json["recurrence"]["id"]
        .as_str()
        .expect("the next occurrence still gets an id");
    assert_eq!(json["recurrence"]["due"], expected_due);

    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(
        todo.contains(&format!("id:{new_id}")),
        "next occurrence lands in todo.txt: {todo}"
    );
    assert!(todo.contains("rec:+1w"), "rec: is carried over: {todo}");
    assert!(
        todo.contains(&format!("due:{expected_due}")),
        "due: is rebuilt from the completion date: {todo}"
    );

    let done = fs::read_to_string(dir.join("done.txt")).unwrap();
    assert!(done.contains("id:r4"), "the old line is archived: {done}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn complete_after_clearing_a_threshold_only_recurrence_gains_a_due_date() {
    let dir = fresh_dir("torudo_it_threshold_none_then_complete");
    // The mirror of the case above. An item carrying only a threshold moves that
    // threshold on completion; once `--threshold none` takes the threshold away
    // there is no date left to move, so the next occurrence is given a `due:`
    // counted from the completion date instead. The recurrence keeps running —
    // it just changes shape — and this pins that down.
    let seeded_threshold = days_from_today(-3);
    let expected_due = days_from_today(7).to_string();
    fs::write(
        dir.join("todo.txt"),
        format!("2026-08-01 Water the plants +home t:{seeded_threshold} rec:+1w id:w5\n"),
    )
    .unwrap();

    let cleared = run(&dir, &["todo", "set", "w5", "--threshold", "none"]);
    let cleared_json: serde_json::Value = serde_json::from_str(stdout_of(&cleared).trim()).unwrap();
    assert!(
        cleared_json["key_values"].get("t").is_none(),
        "t is gone before the completion: {cleared_json}"
    );

    let output = run(&dir, &["todo", "complete", "w5"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json["completed"], true);
    let new_id = json["recurrence"]["id"]
        .as_str()
        .expect("the next occurrence still gets an id");
    assert_eq!(json["recurrence"]["due"], expected_due);
    assert!(
        json["recurrence"]["t"].is_null(),
        "no threshold comes back: {json}"
    );

    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(
        todo.contains(&format!("id:{new_id}")),
        "next occurrence lands in todo.txt: {todo}"
    );
    assert!(todo.contains("rec:+1w"), "rec: is carried over: {todo}");
    assert!(
        todo.contains(&format!("due:{expected_due}")),
        "the next occurrence is driven by a due date now: {todo}"
    );
    // A bare "t:" would also match a word in the description, so the absence is
    // checked on the leading space that separates the tag from what precedes it.
    assert!(!todo.contains(" t:"), "no threshold is reinstated: {todo}");

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
fn reopen_brings_an_item_back_from_done() {
    let dir = fresh_dir("torudo_it_reopen");
    fs::write(
        dir.join("done.txt"),
        "x 2026-08-15 2026-08-01 Write docs +torudo id:d1\nx 2026-08-14 Other id:d2\n",
    )
    .unwrap();
    fs::write(dir.join("todo.txt"), "Existing id:t9\n").unwrap();

    let output = run(&dir, &["todo", "reopen", "d1"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json["id"], "d1");
    assert_eq!(json["completed"], false);
    assert_eq!(json["completion_date"], serde_json::Value::Null);
    assert_eq!(json["creation_date"], "2026-08-01");

    let todo = fs::read_to_string(dir.join("todo.txt")).unwrap();
    assert!(todo.contains("Write docs"), "back in todo.txt: {todo}");
    assert!(todo.contains("+torudo"), "tags survive: {todo}");
    assert!(todo.contains("Existing id:t9"), "others stay: {todo}");

    let done = fs::read_to_string(dir.join("done.txt")).unwrap();
    assert!(!done.contains("id:d1"), "gone from done.txt: {done}");
    assert!(done.contains("id:d2"), "other completions stay: {done}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn reopen_can_target_any_mode() {
    let dir = fresh_dir("torudo_it_reopen_mode");
    fs::write(dir.join("done.txt"), "x 2026-08-15 Rethink this id:d1\n").unwrap();

    let output = run(&dir, &["someday", "reopen", "d1"]);
    stdout_of(&output);

    let someday = fs::read_to_string(dir.join("someday.txt")).unwrap();
    assert!(someday.contains("Rethink this"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn reopen_on_unknown_id_fails() {
    let dir = fresh_dir("torudo_it_reopen_unknown");
    fs::write(dir.join("done.txt"), "x 2026-08-15 Write docs id:d1\n").unwrap();

    let output = run(&dir, &["todo", "reopen", "nope"]);

    assert!(!output.status.success(), "unknown id should exit non-zero");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("nope"),
        "stderr should name the id: {stderr}"
    );

    let done = fs::read_to_string(dir.join("done.txt")).unwrap();
    assert!(done.contains("id:d1"), "done.txt is untouched: {done}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn done_list_shows_completed_items() {
    let dir = fresh_dir("torudo_it_done_list");
    fs::write(
        dir.join("done.txt"),
        "x 2026-08-15 2026-08-01 Write docs id:d1\nx 2026-08-14 Other id:d2\n",
    )
    .unwrap();

    let output = run(&dir, &["done", "list", "--json"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    let items = json.as_array().expect("stdout is a JSON array");
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|i| i["completed"] == true));
    let ids: Vec<&str> = items.iter().map(|i| i["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"d1"));
    assert!(ids.contains(&"d2"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn done_list_on_missing_file_returns_empty_array() {
    let dir = fresh_dir("torudo_it_done_list_missing");

    let output = run(&dir, &["done", "list", "--json"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    assert_eq!(json, serde_json::json!([]));

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

#[test]
fn list_with_md_includes_the_detail_md_body() {
    let dir = fresh_dir("torudo_it_list_with_md");
    fs::write(dir.join("inbox.txt"), "Buy milk id:aaa\n").unwrap();
    fs::create_dir_all(dir.join("todos")).unwrap();
    fs::write(dir.join("todos/aaa.md"), "- [ ] compare prices\n").unwrap();

    let output = run(&dir, &["inbox", "list", "--json", "--with-md"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    let items = json.as_array().expect("stdout is a JSON array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], "aaa");
    assert_eq!(items[0]["md"], "- [ ] compare prices\n");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn list_with_md_omits_the_md_key_when_there_is_no_detail_file() {
    let dir = fresh_dir("torudo_it_list_with_md_no_file");
    fs::write(dir.join("inbox.txt"), "Buy milk id:aaa\n").unwrap();

    let output = run(&dir, &["inbox", "list", "--json", "--with-md"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    let items = json.as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert!(
        items[0].get("md").is_none(),
        "a missing detail file drops the key instead of printing null: {}",
        items[0]
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn list_with_md_leaves_an_item_without_an_id_alone() {
    let dir = fresh_dir("torudo_it_list_with_md_no_id");
    fs::write(dir.join("inbox.txt"), "No id here\n").unwrap();

    let output = run(&dir, &["inbox", "list", "--json", "--with-md"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    let items = json.as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0]["id"].is_null());
    assert!(items[0].get("md").is_none(), "no id means no md to look up");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn list_json_without_with_md_has_no_md_key() {
    let dir = fresh_dir("torudo_it_list_json_no_md");
    fs::write(dir.join("inbox.txt"), "Buy milk id:aaa\n").unwrap();
    fs::create_dir_all(dir.join("todos")).unwrap();
    fs::write(dir.join("todos/aaa.md"), "- [ ] compare prices\n").unwrap();

    let output = run(&dir, &["inbox", "list", "--json"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    let items = json.as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert!(
        items[0].get("md").is_none(),
        "the md body is opt-in even when the file is right there: {}",
        items[0]
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn list_with_md_requires_json() {
    let dir = fresh_dir("torudo_it_list_with_md_requires_json");
    fs::write(dir.join("inbox.txt"), "Buy milk id:aaa\n").unwrap();

    let output = run(&dir, &["inbox", "list", "--with-md"]);

    assert!(!output.status.success(), "the flag pair is rejected");
    assert!(output.stdout.is_empty(), "nothing is printed on rejection");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--json"), "stderr names --json: {stderr}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn done_list_with_md_includes_the_detail_md_body() {
    let dir = fresh_dir("torudo_it_done_list_with_md");
    fs::write(dir.join("done.txt"), "x 2026-08-19 Old thing id:zzz\n").unwrap();
    fs::create_dir_all(dir.join("todos")).unwrap();
    // Completing an item leaves `todos/{id}.md` where it is, so the archive can
    // still hand back what was written while the item was live.
    fs::write(dir.join("todos/zzz.md"), "what came of it\n").unwrap();

    let output = run(&dir, &["done", "list", "--json", "--with-md"]);
    let json: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).unwrap();

    let items = json.as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], "zzz");
    assert_eq!(items[0]["md"], "what came of it\n");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn done_list_with_md_requires_json() {
    let dir = fresh_dir("torudo_it_done_list_with_md_requires_json");
    fs::write(dir.join("done.txt"), "x 2026-08-19 Old thing id:zzz\n").unwrap();

    let output = run(&dir, &["done", "list", "--with-md"]);

    assert!(!output.status.success(), "the flag pair is rejected");
    assert!(output.stdout.is_empty(), "nothing is printed on rejection");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--json"), "stderr names --json: {stderr}");

    fs::remove_dir_all(&dir).ok();
}
