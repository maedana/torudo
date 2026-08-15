use log::debug;
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;

use crate::app_state::AppState;
use crate::todo;

pub const METHOD_GET_CURRENT: &str = "get_current";
pub const METHOD_FOCUS: &str = "focus";

const MAX_REQUEST_SIZE: usize = 4096;

/// Return the torudo RPC socket path: `/tmp/torudo-{uid}.sock`
pub fn socket_path() -> PathBuf {
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/tmp/torudo-{uid}.sock"))
}

/// Encode a msgpack-rpc request with string params: `[0, msgid, method, params]`
pub fn encode_request_with_params(msgid: u32, method: &str, params: &[&str]) -> Vec<u8> {
    let args = params
        .iter()
        .map(|p| rmpv::Value::String((*p).into()))
        .collect();
    let request = rmpv::Value::Array(vec![
        rmpv::Value::Integer(0.into()),
        rmpv::Value::Integer(msgid.into()),
        rmpv::Value::String(method.into()),
        rmpv::Value::Array(args),
    ]);
    let mut buf = Vec::new();
    rmpv::encode::write_value(&mut buf, &request).expect("encode should not fail");
    buf
}

/// Decode a msgpack-rpc request: `[0, msgid, method, params]`
pub fn decode_request(data: &[u8]) -> Result<(u32, String, rmpv::Value), String> {
    let value =
        rmpv::decode::read_value(&mut &data[..]).map_err(|e| format!("decode error: {e}"))?;
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

/// Decode a msgpack-rpc response: `[1, msgid, error, result]`
pub fn decode_response(data: &[u8]) -> Result<(Option<String>, Option<String>), String> {
    let value =
        rmpv::decode::read_value(&mut &data[..]).map_err(|e| format!("decode error: {e}"))?;
    let arr = value.as_array().ok_or("expected array")?;
    if arr.len() != 4 {
        return Err(format!("expected 4 elements, got {}", arr.len()));
    }
    let error = if arr[2].is_nil() {
        None
    } else {
        Some(arr[2].as_str().unwrap_or("unknown error").to_string())
    };
    let result = if arr[3].is_nil() {
        None
    } else {
        Some(arr[3].as_str().unwrap_or("").to_string())
    };
    Ok((error, result))
}

pub struct RpcServer {
    listener: UnixListener,
    path: PathBuf,
}

impl RpcServer {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let path = socket_path();
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        listener.set_nonblocking(true)?;
        debug!("RPC server listening on {}", path.display());
        Ok(Self { listener, path })
    }

    /// Poll for incoming RPC requests (non-blocking). Takes the whole state
    /// because `focus` moves the cursor rather than only reading it.
    pub fn poll(&self, state: &mut AppState) {
        let stream = match self.listener.accept() {
            Ok((stream, _)) => stream,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => return,
            Err(e) => {
                debug!("RPC accept error: {e}");
                return;
            }
        };
        Self::handle_connection(stream, state);
    }

    fn handle_connection(mut stream: std::os::unix::net::UnixStream, state: &mut AppState) {
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(500)));

        let mut buf = [0u8; MAX_REQUEST_SIZE];
        let Ok(n) = stream.read(&mut buf) else {
            return;
        };

        let (msgid, method, params) = match decode_request(&buf[..n]) {
            Ok(v) => v,
            Err(e) => {
                debug!("RPC decode error: {e}");
                return;
            }
        };

        debug!("RPC request: method={method}, msgid={msgid}");

        // Every handler takes the same pair, so this stays a plain name-to-handler table.
        let result = match method.as_str() {
            METHOD_GET_CURRENT => handle_get_current(state, &params),
            METHOD_FOCUS => handle_focus(state, &params),
            _ => Err(format!("unknown method: {method}")),
        };
        let response = match result {
            Ok(json) => encode_response(msgid, None, Some(&json)),
            Err(e) => encode_response(msgid, Some(&e), None),
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

/// The first param of an RPC request. Callers name the argument in their own error.
fn first_string_param(params: &rmpv::Value) -> Option<&str> {
    params
        .as_array()
        .and_then(|args| args.first())
        .and_then(rmpv::Value::as_str)
}

fn handle_focus(state: &mut AppState, params: &rmpv::Value) -> Result<String, String> {
    let id = first_string_param(params)
        .ok_or_else(|| format!("{METHOD_FOCUS} requires an id argument"))?;
    let focused = state
        .focus_todo(id)
        .ok_or_else(|| format!("no item with id:{id}"))?;
    let response = serde_json::json!({
        "id": id,
        "mode": focused.mode.cli_name(),
        "column": focused.column,
        "filter_cleared": focused.filter_cleared,
    });
    serde_json::to_string_pretty(&response).map_err(|e| e.to_string())
}

fn handle_get_current(state: &AppState, _params: &rmpv::Value) -> Result<String, String> {
    let item = state.get_current_todo().ok_or("no todo selected")?;
    todo::item_to_json(item, &state.todotxt_dir).map_err(|e| e.to_string())
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
        let (error, result) = decode_response(&encoded).unwrap();
        assert!(error.is_none());
        assert_eq!(result.unwrap(), "hello world");
    }

    #[test]
    fn test_encode_decode_response_error() {
        let encoded = encode_response(1, Some("something went wrong"), None);
        let (error, result) = decode_response(&encoded).unwrap();
        assert_eq!(error.unwrap(), "something went wrong");
        assert!(result.is_none());
    }

    #[test]
    fn test_encode_decode_request_roundtrip() {
        let encoded = encode_request_with_params(1, METHOD_GET_CURRENT, &[]);
        let (msgid, method, params) = decode_request(&encoded).unwrap();
        assert_eq!(msgid, 1);
        assert_eq!(method, METHOD_GET_CURRENT);
        assert!(params.as_array().unwrap().is_empty());
    }

    #[test]
    fn test_encode_request_with_params_roundtrip() {
        let encoded = encode_request_with_params(7, METHOD_FOCUS, &["abc-123"]);
        let (msgid, method, params) = decode_request(&encoded).unwrap();
        assert_eq!(msgid, 7);
        assert_eq!(method, METHOD_FOCUS);
        let args = params.as_array().unwrap();
        assert_eq!(args.len(), 1);
        assert_eq!(args[0].as_str().unwrap(), "abc-123");
    }

    #[test]
    fn test_decode_request_invalid_bytes() {
        let result = decode_request(&[0xff, 0xff]);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_request_wrong_type() {
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
    fn test_handle_get_current_with_md() {
        let dir = tempfile::tempdir().unwrap();
        let todos_dir = dir.path().join("todos");
        std::fs::create_dir(&todos_dir).unwrap();
        std::fs::write(todos_dir.join("abc-123.md"), "# Details").unwrap();

        let state = test_state(dir.path(), "(A) My task +project @home id:abc-123\n");
        let result = handle_get_current(&state, &no_params()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(json["title"], "My task");
        assert_eq!(json["priority"], "A");
        assert_eq!(json["id"], "abc-123");
        assert_eq!(json["md"], "# Details");
        assert_eq!(json["projects"], serde_json::json!(["project"]));
        assert_eq!(json["contexts"], serde_json::json!(["home"]));
        assert_eq!(json["completed"], false);
    }

    #[test]
    fn test_handle_get_current_without_md() {
        let dir = tempfile::tempdir().unwrap();
        let todos_dir = dir.path().join("todos");
        std::fs::create_dir(&todos_dir).unwrap();

        let state = test_state(dir.path(), "Simple task id:xyz-789\n");
        let result = handle_get_current(&state, &no_params()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(json["title"], "Simple task");
        assert_eq!(json["id"], "xyz-789");
        assert!(json.get("md").is_none());
    }

    #[test]
    fn test_handle_get_current_no_selection() {
        let dir = tempfile::tempdir().unwrap();
        let state = test_state(dir.path(), "");
        let result = handle_get_current(&state, &no_params());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no todo selected"));
    }

    /// The empty param list every no-argument RPC method is called with.
    fn no_params() -> rmpv::Value {
        rmpv::Value::Array(vec![])
    }

    /// An `AppState` in Todo mode reading `content` from a throwaway todotxt dir.
    fn test_state(dir: &std::path::Path, content: &str) -> AppState {
        let todo_file = dir.join("todo.txt");
        std::fs::write(&todo_file, content).unwrap();
        let todos = todo::load_todos(todo_file.to_str().unwrap()).unwrap();
        AppState::new(todos, String::new(), dir.to_str().unwrap().to_string())
    }

    fn string_params(args: &[&str]) -> rmpv::Value {
        rmpv::Value::Array(
            args.iter()
                .map(|a| rmpv::Value::String((*a).into()))
                .collect(),
        )
    }

    #[test]
    fn test_handle_focus_success() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = test_state(dir.path(), "T1 +work id:t1\n");

        let result = handle_focus(&mut state, &string_params(&["t1"])).unwrap();
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(json["id"], "t1");
        assert_eq!(json["mode"], "todo");
        assert_eq!(json["column"], "work");
        assert_eq!(json["filter_cleared"], false);
    }

    #[test]
    fn test_handle_focus_unknown_id() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = test_state(dir.path(), "T1 +work id:t1\n");

        let err = handle_focus(&mut state, &string_params(&["nope"])).unwrap_err();

        assert_eq!(err, "no item with id:nope");
    }

    #[test]
    fn test_handle_focus_missing_id_param() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = test_state(dir.path(), "T1 +work id:t1\n");

        let err = handle_focus(&mut state, &string_params(&[])).unwrap_err();

        assert_eq!(err, "focus requires an id argument");
    }

    #[test]
    fn test_rpc_roundtrip() {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;

        let dir = tempfile::tempdir().unwrap();
        let todos_dir = dir.path().join("todos");
        std::fs::create_dir(&todos_dir).unwrap();
        std::fs::write(todos_dir.join("test-id.md"), "# Test Content").unwrap();

        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = RpcServer {
            listener,
            path: sock_path.clone(),
        };

        let mut state = test_state(dir.path(), "Test todo id:test-id\n");

        let mut client = UnixStream::connect(&sock_path).unwrap();
        let req_buf = encode_request_with_params(42, METHOD_GET_CURRENT, &[]);
        client.write_all(&req_buf).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();

        server.poll(&mut state);

        let mut resp_buf = Vec::new();
        client.read_to_end(&mut resp_buf).unwrap();
        let (error, result) = decode_response(&resp_buf).unwrap();
        assert!(error.is_none());
        let json: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(json["title"], "Test todo");
        assert_eq!(json["md"], "# Test Content");
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

        let mut state = test_state(dir.path(), "x id:x\n");

        let mut client = UnixStream::connect(&sock_path).unwrap();
        let req_buf = encode_request_with_params(1, "nonexistent", &[]);
        client.write_all(&req_buf).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();

        server.poll(&mut state);

        let mut resp_buf = Vec::new();
        client.read_to_end(&mut resp_buf).unwrap();
        let (error, _result) = decode_response(&resp_buf).unwrap();
        assert!(error.unwrap().contains("unknown method"));
    }

    #[test]
    fn test_rpc_focus_roundtrip() {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;

        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test3.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = RpcServer {
            listener,
            path: sock_path.clone(),
        };

        let mut state = test_state(
            dir.path(),
            "First +alpha id:first\nSecond +beta id:second\n",
        );
        assert_eq!(state.get_current_todo_id(), Some("first"));

        let mut client = UnixStream::connect(&sock_path).unwrap();
        let req_buf = encode_request_with_params(3, METHOD_FOCUS, &["second"]);
        client.write_all(&req_buf).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();

        server.poll(&mut state);

        let mut resp_buf = Vec::new();
        client.read_to_end(&mut resp_buf).unwrap();
        let (error, result) = decode_response(&resp_buf).unwrap();
        assert!(error.is_none());
        let json: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(json["id"], "second");
        assert_eq!(json["mode"], "todo");
        assert_eq!(json["column"], "beta");
        assert_eq!(state.get_current_todo_id(), Some("second"));
    }
}
