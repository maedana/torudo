use serde::Deserialize;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Parse a version string like "crmux 0.10.0\n" into (major, minor, patch).
fn parse_crmux_version(output: &str) -> Option<(u32, u32, u32)> {
    let version_str = output.trim().strip_prefix("crmux ")?;
    let mut parts = version_str.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// Check if version meets the minimum required version.
const fn version_meets_minimum(version: (u32, u32, u32), minimum: (u32, u32, u32)) -> bool {
    if version.0 != minimum.0 {
        return version.0 > minimum.0;
    }
    if version.1 != minimum.1 {
        return version.1 > minimum.1;
    }
    version.2 >= minimum.2
}

/// Minimum crmux version required for `send_text` with project targeting.
const MIN_CRMUX_VERSION: (u32, u32, u32) = (0, 10, 0);

/// Minimum crmux version required for `get-plans` RPC.
pub const MIN_CRMUX_GET_PLANS_VERSION: (u32, u32, u32) = (0, 11, 0);

/// Return the crmux socket path: `/tmp/crmux-{uid}.sock`
fn socket_path() -> PathBuf {
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/tmp/crmux-{uid}.sock"))
}

/// Detect a compatible crmux instance and return its version.
///
/// Returns `Some((major, minor, patch))` if:
/// 1. `crmux --version` reports version >= 0.10.0
/// 2. The crmux Unix domain socket exists
///
/// Returns `None` if crmux is not available.
pub fn detect() -> Option<(u32, u32, u32)> {
    let output = Command::new("crmux")
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version_str = String::from_utf8_lossy(&output.stdout);
    let version = parse_crmux_version(&version_str)?;
    if !version_meets_minimum(version, MIN_CRMUX_VERSION) {
        return None;
    }
    if !socket_path().exists() {
        return None;
    }
    Some(version)
}

/// Check if version meets the minimum required for get-plans RPC.
pub const fn version_supports_get_plans(version: (u32, u32, u32)) -> bool {
    version_meets_minimum(version, MIN_CRMUX_GET_PLANS_VERSION)
}

/// Encode a msgpack-rpc notification: `[2, method, params]`
///
/// Uses the same encoding as crmux: `rmp` for the header, `rmp_serde` for params.
fn encode_notification(method: &str, params: &serde_json::Value) -> Vec<u8> {
    let mut buf = Vec::new();
    rmp::encode::write_array_len(&mut buf, 3).expect("encode array len");
    rmp::encode::write_uint(&mut buf, 2).expect("encode type");
    rmp::encode::write_str(&mut buf, method).expect("encode method");
    let params_bytes = rmp_serde::to_vec(params).expect("encode params");
    buf.extend_from_slice(&params_bytes);
    buf
}

/// Send text to a crmux project via the Unix domain socket.
/// If `mode` is `Some`, crmux will switch the target session's permission mode before sending.
/// Valid modes: `"plan-mode"`, `"accept-edits"`.
pub fn send_text(
    project: &str,
    text: &str,
    mode: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = serde_json::json!({
        "text": text,
        "project": project,
    });
    if let Some(m) = mode {
        params["mode"] = serde_json::Value::String(m.to_string());
    }
    let payload = encode_notification("send_text", &params);
    let mut stream = UnixStream::connect(socket_path())?;
    stream.write_all(&payload)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct Plan {
    pub title: String,
    pub project_name: String,
    pub slug: String,
    pub path: String,
}

#[derive(Deserialize)]
struct PlansResponse {
    plans: Vec<Plan>,
}

pub fn parse_plans(json: &str) -> Result<Vec<Plan>, Box<dyn std::error::Error>> {
    let response: PlansResponse = serde_json::from_str(json)?;
    Ok(response.plans)
}

pub fn get_plans() -> Result<Vec<Plan>, Box<dyn std::error::Error>> {
    let output = Command::new("crmux")
        .args(["rpc", "get-plans"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err("crmux rpc get-plans failed".into());
    }
    let json = String::from_utf8(output.stdout)?;
    parse_plans(&json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plans_valid_json() {
        let json = r#"{
            "plans": [
                {
                    "path": "/home/user/.claude/plans/abc.md",
                    "project_name": "myproject",
                    "session_id": "sess-1",
                    "slug": "abc",
                    "title": "My Plan"
                },
                {
                    "path": "/home/user/.claude/plans/def.md",
                    "project_name": "other",
                    "session_id": "sess-2",
                    "slug": "def",
                    "title": "Other Plan"
                }
            ]
        }"#;
        let plans = parse_plans(json).unwrap();
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].title, "My Plan");
        assert_eq!(plans[0].project_name, "myproject");
        assert_eq!(plans[0].slug, "abc");
        assert_eq!(plans[0].path, "/home/user/.claude/plans/abc.md");
        assert_eq!(plans[1].slug, "def");
    }

    #[test]
    fn test_parse_plans_empty() {
        let json = r#"{"plans": []}"#;
        let plans = parse_plans(json).unwrap();
        assert!(plans.is_empty());
    }

    #[test]
    fn test_parse_plans_invalid_json() {
        let result = parse_plans("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_crmux_version_valid() {
        assert_eq!(parse_crmux_version("crmux 0.10.0\n"), Some((0, 10, 0)));
        assert_eq!(parse_crmux_version("crmux 1.0.0\n"), Some((1, 0, 0)));
        assert_eq!(parse_crmux_version("crmux 0.10.1\n"), Some((0, 10, 1)));
    }

    #[test]
    fn test_parse_crmux_version_invalid() {
        assert_eq!(parse_crmux_version("invalid"), None);
        assert_eq!(parse_crmux_version(""), None);
        assert_eq!(parse_crmux_version("crmux abc\n"), None);
        assert_eq!(parse_crmux_version("other 0.10.0\n"), None);
    }

    #[test]
    fn test_version_meets_minimum() {
        // Exact match
        assert!(version_meets_minimum((0, 10, 0), (0, 10, 0)));
        // Higher patch
        assert!(version_meets_minimum((0, 10, 1), (0, 10, 0)));
        // Higher minor
        assert!(version_meets_minimum((0, 11, 0), (0, 10, 0)));
        // Higher major
        assert!(version_meets_minimum((1, 0, 0), (0, 10, 0)));
        // Lower minor
        assert!(!version_meets_minimum((0, 9, 0), (0, 10, 0)));
        // Lower minor, higher patch
        assert!(!version_meets_minimum((0, 9, 9), (0, 10, 0)));
    }

    #[test]
    fn test_version_supports_get_plans() {
        // Below 0.11.0
        assert!(!version_supports_get_plans((0, 10, 0)));
        assert!(!version_supports_get_plans((0, 10, 9)));
        // Exact 0.11.0
        assert!(version_supports_get_plans((0, 11, 0)));
        // Above 0.11.0
        assert!(version_supports_get_plans((0, 11, 1)));
        assert!(version_supports_get_plans((0, 12, 0)));
        assert!(version_supports_get_plans((1, 0, 0)));
    }

    #[test]
    fn test_socket_path_contains_uid() {
        let path = socket_path();
        let uid = unsafe { libc::getuid() };
        assert_eq!(path, PathBuf::from(format!("/tmp/crmux-{uid}.sock")));
    }

    #[test]
    fn test_encode_notification_roundtrip() {
        let params = serde_json::json!({"text": "hello", "project": "work"});
        let payload = encode_notification("send_text", &params);

        // Decode header manually (same as crmux's decode_notification)
        let mut cursor = std::io::Cursor::new(&payload);
        let array_len = rmp::decode::read_array_len(&mut cursor).unwrap();
        assert_eq!(array_len, 3);

        let msg_type = rmp::decode::read_int::<u64, _>(&mut cursor).unwrap();
        assert_eq!(msg_type, 2); // Notification

        let mut method_buf = vec![0u8; 256];
        let method = rmp::decode::read_str(&mut cursor, &mut method_buf).unwrap();
        assert_eq!(method, "send_text");

        #[allow(clippy::cast_possible_truncation)]
        let remaining = &payload[cursor.position() as usize..];
        let decoded_params: serde_json::Value = rmp_serde::from_slice(remaining).unwrap();
        assert_eq!(decoded_params["text"], "hello");
        assert_eq!(decoded_params["project"], "work");
    }

    #[test]
    fn test_encode_notification_empty_params() {
        let params = serde_json::json!({});
        let payload = encode_notification("ping", &params);

        let mut cursor = std::io::Cursor::new(&payload);
        let array_len = rmp::decode::read_array_len(&mut cursor).unwrap();
        assert_eq!(array_len, 3);

        let msg_type = rmp::decode::read_int::<u64, _>(&mut cursor).unwrap();
        assert_eq!(msg_type, 2);

        let mut method_buf = vec![0u8; 256];
        let method = rmp::decode::read_str(&mut cursor, &mut method_buf).unwrap();
        assert_eq!(method, "ping");

        #[allow(clippy::cast_possible_truncation)]
        let remaining = &payload[cursor.position() as usize..];
        let decoded_params: serde_json::Value = rmp_serde::from_slice(remaining).unwrap();
        assert_eq!(decoded_params, serde_json::json!({}));
    }
}
