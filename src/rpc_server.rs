use log::debug;
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;

/// Return the torudo RPC socket path: `/tmp/torudo-{uid}.sock`
pub fn socket_path() -> PathBuf {
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/tmp/torudo-{uid}.sock"))
}

/// Decode a msgpack-rpc request: `[0, msgid, method, params]`
pub fn decode_request(data: &[u8]) -> Result<(u32, String, rmpv::Value), String> {
    let value = rmpv::decode::read_value(&mut &data[..]).map_err(|e| format!("decode error: {e}"))?;
    let arr = value.as_array().ok_or("expected array")?;
    if arr.len() != 4 {
        return Err(format!("expected 4 elements, got {}", arr.len()));
    }
    let msg_type = arr[0].as_u64().ok_or("invalid type")?;
    if msg_type != 0 {
        return Err(format!("expected type 0 (request), got {msg_type}"));
    }
    #[allow(clippy::cast_possible_truncation)]
    let msgid = arr[1].as_u64().ok_or("invalid msgid")? as u32;
    let method = arr[2].as_str().ok_or("invalid method")?.to_string();
    let params = arr[3].clone();
    Ok((msgid, method, params))
}

/// Encode a msgpack-rpc response: `[1, msgid, error, result]`
pub fn encode_response(msgid: u32, error: Option<&str>, result: Option<&str>) -> Vec<u8> {
    let error_val = error.map_or(rmpv::Value::Nil, |e| rmpv::Value::String(e.into()));
    let result_val = result.map_or(rmpv::Value::Nil, |r| rmpv::Value::String(r.into()));
    let response = rmpv::Value::Array(vec![
        rmpv::Value::Integer(1.into()),
        rmpv::Value::Integer(msgid.into()),
        error_val,
        result_val,
    ]);
    let mut buf = Vec::new();
    rmpv::encode::write_value(&mut buf, &response).expect("encode should not fail");
    buf
}

pub struct RpcServer {
    listener: UnixListener,
    path: PathBuf,
}

impl RpcServer {
    /// Create a new RPC server listening on the torudo socket.
    /// Removes any stale socket file before binding.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let path = socket_path();
        // Remove stale socket if it exists
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        listener.set_nonblocking(true)?;
        debug!("RPC server listening on {}", path.display());
        Ok(Self { listener, path })
    }

    /// Poll for incoming RPC requests (non-blocking).
    /// Call this once per main loop iteration.
    pub fn poll(&self, current_todo_id: Option<&str>, todotxt_dir: &str) {
        let stream = match self.listener.accept() {
            Ok((stream, _)) => stream,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => return,
            Err(e) => {
                debug!("RPC accept error: {e}");
                return;
            }
        };
        Self::handle_connection(stream, current_todo_id, todotxt_dir);
    }

    fn handle_connection(
        mut stream: std::os::unix::net::UnixStream,
        current_todo_id: Option<&str>,
        todotxt_dir: &str,
    ) {
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(500)));

        let mut buf = Vec::new();
        if stream.read_to_end(&mut buf).is_err() {
            return;
        }

        let (msgid, method, _params) = match decode_request(&buf) {
            Ok(v) => v,
            Err(e) => {
                debug!("RPC decode error: {e}");
                return;
            }
        };

        debug!("RPC request: method={method}, msgid={msgid}");

        let response = match method.as_str() {
            "get_current_md" => match handle_get_current_md(current_todo_id, todotxt_dir) {
                Ok(content) => encode_response(msgid, None, Some(&content)),
                Err(e) => encode_response(msgid, Some(&e), None),
            },
            _ => encode_response(msgid, Some(&format!("unknown method: {method}")), None),
        };

        let _ = stream.write_all(&response);
    }
}

impl Drop for RpcServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        debug!("RPC server socket removed: {}", self.path.display());
    }
}

/// Handle `get_current_md` RPC method.
/// Returns the content of the currently selected todo's .md file.
pub fn handle_get_current_md(
    current_todo_id: Option<&str>,
    todotxt_dir: &str,
) -> Result<String, String> {
    let todo_id = current_todo_id.ok_or("no todo selected")?;
    let file_path = format!("{todotxt_dir}/todos/{todo_id}.md");
    std::fs::read_to_string(&file_path)
        .map_err(|e| format!("failed to read {file_path}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_path() {
        let path = socket_path();
        let uid = unsafe { libc::getuid() };
        assert_eq!(path, PathBuf::from(format!("/tmp/torudo-{uid}.sock")));
    }

    #[test]
    fn test_encode_decode_response_success() {
        let encoded = encode_response(42, None, Some("hello world"));
        let value = rmpv::decode::read_value(&mut &encoded[..]).unwrap();
        let arr = value.as_array().unwrap();
        assert_eq!(arr[0].as_u64().unwrap(), 1); // type = response
        assert_eq!(arr[1].as_u64().unwrap(), 42); // msgid
        assert!(arr[2].is_nil()); // no error
        assert_eq!(arr[3].as_str().unwrap(), "hello world");
    }

    #[test]
    fn test_encode_decode_response_error() {
        let encoded = encode_response(1, Some("something went wrong"), None);
        let value = rmpv::decode::read_value(&mut &encoded[..]).unwrap();
        let arr = value.as_array().unwrap();
        assert_eq!(arr[0].as_u64().unwrap(), 1);
        assert_eq!(arr[1].as_u64().unwrap(), 1);
        assert_eq!(arr[2].as_str().unwrap(), "something went wrong");
        assert!(arr[3].is_nil());
    }

    #[test]
    fn test_decode_request_valid() {
        // Build a valid request: [0, 1, "get_current_md", []]
        let request = rmpv::Value::Array(vec![
            rmpv::Value::Integer(0.into()),
            rmpv::Value::Integer(1.into()),
            rmpv::Value::String("get_current_md".into()),
            rmpv::Value::Array(vec![]),
        ]);
        let mut buf = Vec::new();
        rmpv::encode::write_value(&mut buf, &request).unwrap();

        let (msgid, method, params) = decode_request(&buf).unwrap();
        assert_eq!(msgid, 1);
        assert_eq!(method, "get_current_md");
        assert!(params.as_array().unwrap().is_empty());
    }

    #[test]
    fn test_decode_request_invalid_bytes() {
        let result = decode_request(&[0xff, 0xff]);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_request_wrong_type() {
        // type=2 (notification) instead of 0 (request)
        let request = rmpv::Value::Array(vec![
            rmpv::Value::Integer(2.into()),
            rmpv::Value::String("method".into()),
            rmpv::Value::Array(vec![]),
            rmpv::Value::Nil,
        ]);
        let mut buf = Vec::new();
        rmpv::encode::write_value(&mut buf, &request).unwrap();

        let result = decode_request(&buf);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected type 0"));
    }

    #[test]
    fn test_handle_get_current_md_success() {
        let dir = tempfile::tempdir().unwrap();
        let todos_dir = dir.path().join("todos");
        std::fs::create_dir(&todos_dir).unwrap();
        let md_path = todos_dir.join("abc-123.md");
        std::fs::write(&md_path, "# My Todo\nDetails here").unwrap();

        let result = handle_get_current_md(Some("abc-123"), dir.path().to_str().unwrap());
        assert_eq!(result.unwrap(), "# My Todo\nDetails here");
    }

    #[test]
    fn test_handle_get_current_md_no_selection() {
        let result = handle_get_current_md(None, "/tmp");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no todo selected"));
    }

    #[test]
    fn test_rpc_roundtrip() {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;

        let dir = tempfile::tempdir().unwrap();
        let todos_dir = dir.path().join("todos");
        std::fs::create_dir(&todos_dir).unwrap();
        std::fs::write(todos_dir.join("test-id.md"), "# Test Content").unwrap();

        // Create server on a temp socket path
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = RpcServer {
            listener,
            path: sock_path.clone(),
        };

        // Client sends request
        let mut client = UnixStream::connect(&sock_path).unwrap();
        let request = rmpv::Value::Array(vec![
            rmpv::Value::Integer(0.into()),
            rmpv::Value::Integer(42.into()),
            rmpv::Value::String("get_current_md".into()),
            rmpv::Value::Array(vec![]),
        ]);
        let mut req_buf = Vec::new();
        rmpv::encode::write_value(&mut req_buf, &request).unwrap();
        client.write_all(&req_buf).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();

        // Server polls and handles the request
        server.poll(Some("test-id"), dir.path().to_str().unwrap());

        // Client reads response
        let mut resp_buf = Vec::new();
        client.read_to_end(&mut resp_buf).unwrap();
        let value = rmpv::decode::read_value(&mut &resp_buf[..]).unwrap();
        let arr = value.as_array().unwrap();
        assert_eq!(arr[0].as_u64().unwrap(), 1); // response type
        assert_eq!(arr[1].as_u64().unwrap(), 42); // msgid
        assert!(arr[2].is_nil()); // no error
        assert_eq!(arr[3].as_str().unwrap(), "# Test Content");
    }

    #[test]
    fn test_rpc_unknown_method() {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;

        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test2.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = RpcServer {
            listener,
            path: sock_path.clone(),
        };

        let mut client = UnixStream::connect(&sock_path).unwrap();
        let request = rmpv::Value::Array(vec![
            rmpv::Value::Integer(0.into()),
            rmpv::Value::Integer(1.into()),
            rmpv::Value::String("nonexistent".into()),
            rmpv::Value::Array(vec![]),
        ]);
        let mut req_buf = Vec::new();
        rmpv::encode::write_value(&mut req_buf, &request).unwrap();
        client.write_all(&req_buf).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();

        server.poll(Some("x"), dir.path().to_str().unwrap());

        let mut resp_buf = Vec::new();
        client.read_to_end(&mut resp_buf).unwrap();
        let value = rmpv::decode::read_value(&mut &resp_buf[..]).unwrap();
        let arr = value.as_array().unwrap();
        assert!(arr[2].as_str().unwrap().contains("unknown method"));
    }

    #[test]
    fn test_handle_get_current_md_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let todos_dir = dir.path().join("todos");
        std::fs::create_dir(&todos_dir).unwrap();

        let result = handle_get_current_md(Some("nonexistent"), dir.path().to_str().unwrap());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to read"));
    }
}
