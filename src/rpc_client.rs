use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process;

use crate::rpc_server;

pub fn run_current_md() -> Result<(), Box<dyn std::error::Error>> {
    let path = rpc_server::socket_path();
    let Ok(mut stream) = UnixStream::connect(&path) else {
        eprintln!("torudo is not running");
        process::exit(1);
    };

    let payload = rpc_server::encode_request(1, rpc_server::METHOD_GET_CURRENT_MD);
    stream.write_all(&payload)?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut response_buf = Vec::new();
    stream.read_to_end(&mut response_buf)?;

    let (error, result) = rpc_server::decode_response(&response_buf)?;

    if let Some(err_msg) = error {
        eprintln!("{err_msg}");
        process::exit(1);
    }

    if let Some(content) = result {
        print!("{content}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::rpc_server;

    #[test]
    fn test_encode_request_roundtrip() {
        let encoded = rpc_server::encode_request(1, rpc_server::METHOD_GET_CURRENT_MD);
        let (msgid, method, params) = rpc_server::decode_request(&encoded).unwrap();
        assert_eq!(msgid, 1);
        assert_eq!(method, rpc_server::METHOD_GET_CURRENT_MD);
        assert!(params.as_array().unwrap().is_empty());
    }
}
