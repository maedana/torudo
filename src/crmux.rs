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

/// Return the crmux socket path: `/tmp/crmux-{uid}.sock`
fn socket_path() -> PathBuf {
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/tmp/crmux-{uid}.sock"))
}

/// Check if a compatible crmux instance is running.
///
/// Returns `true` if:
/// 1. `crmux --version` reports version >= 0.10.0
/// 2. The crmux Unix domain socket exists
pub fn is_available() -> bool {
    // Check version
    let Ok(output) = Command::new("crmux")
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let version_str = String::from_utf8_lossy(&output.stdout);
    let Some(version) = parse_crmux_version(&version_str) else {
        return false;
    };
    if !version_meets_minimum(version, MIN_CRMUX_VERSION) {
        return false;
    }

    // Check socket exists
    socket_path().exists()
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
pub fn send_text(project: &str, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let params = serde_json::json!({
        "text": text,
        "project": project,
    });
    let payload = encode_notification("send_text", &params);
    let mut stream = UnixStream::connect(socket_path())?;
    stream.write_all(&payload)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
