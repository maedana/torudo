use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process;

use crate::rpc_server;

/// Send one request to the running TUI and print its result, exiting non-zero
/// when torudo is not running or the server answers with an error.
fn call(method: &str, params: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let path = rpc_server::socket_path();
    let Ok(mut stream) = UnixStream::connect(&path) else {
        eprintln!("torudo is not running");
        process::exit(1);
    };

    let payload = rpc_server::encode_request_with_params(1, method, params);
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
        println!("{content}");
    }

    Ok(())
}

pub fn run_current() -> Result<(), Box<dyn std::error::Error>> {
    call(rpc_server::METHOD_GET_CURRENT, &[])
}

pub fn run_focus(id: &str) -> Result<(), Box<dyn std::error::Error>> {
    call(rpc_server::METHOD_FOCUS, &[id])
}
