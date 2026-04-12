use std::fs;
use std::path::PathBuf;
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

#[test]
fn inbox_add_happy_path() {
    let dir = fresh_dir("torudo_it_inbox_add_happy");

    let output = Command::new(bin())
        .args([
            "--todotxt-dir",
            dir.to_str().unwrap(),
            "inbox",
            "add",
            "(A) Buy milk +grocery @home",
        ])
        .output()
        .expect("failed to run torudo");

    assert!(
        output.status.success(),
        "non-zero exit: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).expect("stdout is JSON");

    assert_eq!(json["title"], "Buy milk");
    assert_eq!(json["priority"], "A");
    assert_eq!(json["projects"], serde_json::json!(["grocery"]));
    assert_eq!(json["contexts"], serde_json::json!(["home"]));
    assert!(json["id"].is_string());

    let inbox = fs::read_to_string(dir.join("inbox.txt")).unwrap();
    assert!(inbox.contains("Buy milk"));
    assert!(inbox.contains("+grocery"));
    assert!(inbox.contains("@home"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn inbox_add_preserves_existing_id() {
    let dir = fresh_dir("torudo_it_inbox_add_keep_id");

    let output = Command::new(bin())
        .args([
            "--todotxt-dir",
            dir.to_str().unwrap(),
            "inbox",
            "add",
            "Keep id:my-fixed-id",
        ])
        .output()
        .expect("failed to run torudo");
    assert!(output.status.success());

    let json: serde_json::Value =
        serde_json::from_str(String::from_utf8(output.stdout).unwrap().trim()).unwrap();
    assert_eq!(json["id"], "my-fixed-id");

    let inbox = fs::read_to_string(dir.join("inbox.txt")).unwrap();
    // id:my-fixed-id がちょうど 1 回だけ現れること（新規 UUID で上書きされない）
    assert_eq!(inbox.matches("id:my-fixed-id").count(), 1);
    assert_eq!(inbox.matches("id:").count(), 1);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn inbox_add_successive_calls_produce_distinct_uuids() {
    let dir = fresh_dir("torudo_it_inbox_add_successive");

    let run = |text: &str| -> serde_json::Value {
        let output = Command::new(bin())
            .args(["--todotxt-dir", dir.to_str().unwrap(), "inbox", "add", text])
            .output()
            .expect("failed to run torudo");
        assert!(
            output.status.success(),
            "non-zero exit: stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_str(String::from_utf8(output.stdout).unwrap().trim()).unwrap()
    };

    let first = run("First task");
    let second = run("Second task");

    let id1 = first["id"].as_str().unwrap().to_string();
    let id2 = second["id"].as_str().unwrap().to_string();
    assert_ne!(id1, id2, "successive adds should generate distinct UUIDs");

    let inbox = fs::read_to_string(dir.join("inbox.txt")).unwrap();
    let lines: Vec<&str> = inbox.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("First task"));
    assert!(lines[1].contains("Second task"));

    fs::remove_dir_all(&dir).ok();
}
