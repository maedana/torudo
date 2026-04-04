use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process;

use crate::rpc_server;

/// Encode a msgpack-rpc request: `[0, msgid, method, params]`
fn encode_request(msgid: u32, method: &str) -> Vec<u8> {
    let request = rmpv::Value::Array(vec![
        rmpv::Value::Integer(0.into()),
        rmpv::Value::Integer(msgid.into()),
        rmpv::Value::String(method.into()),
        rmpv::Value::Array(vec![]),
    ]);
    let mut buf = Vec::new();
    rmpv::encode::write_value(&mut buf, &request).expect("encode should not fail");
    buf
}

/// Run the `current-md` subcommand: connect to running torudo, fetch current md, print to stdout.
pub fn run_current_md() -> Result<(), Box<dyn std::error::Error>> {
    let path = rpc_server::socket_path();
    let Ok(mut stream) = UnixStream::connect(&path) else {
        eprintln!("torudo is not running");
        process::exit(1);
    };

    let payload = encode_request(1, "get_current_md");
    stream.write_all(&payload)?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut response_buf = Vec::new();
    stream.read_to_end(&mut response_buf)?;

    let value = rmpv::decode::read_value(&mut &response_buf[..])
        .map_err(|e| format!("failed to decode response: {e}"))?;
    let arr = value
        .as_array()
        .ok_or("invalid response format")?;

    if arr.len() != 4 {
        return Err("invalid response format".into());
    }

    // arr[2] is error, arr[3] is result
    if !arr[2].is_nil() {
        let err_msg = arr[2].as_str().unwrap_or("unknown error");
        eprintln!("{err_msg}");
        process::exit(1);
    }

    if let Some(content) = arr[3].as_str() {
        print!("{content}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_request() {
        let encoded = encode_request(1, "get_current_md");
        let (msgid, method, params) = rpc_server::decode_request(&encoded).unwrap();
        assert_eq!(msgid, 1);
        assert_eq!(method, "get_current_md");
        assert!(params.as_array().unwrap().is_empty());
    }
}
