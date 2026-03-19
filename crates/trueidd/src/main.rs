use std::fs;
use std::os::unix::net::UnixListener;
use std::path::Path;

use trueid_ipc::{SOCKET_PATH, Request, Response};
use serde_json;
use std::io::{BufRead, BufReader, Write};

fn main() -> std::io::Result<()> {
    println!("Hello, world!");
    Ok(())
}