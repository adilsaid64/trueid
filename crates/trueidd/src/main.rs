use std::fs;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;

use trueid_ipc::SOCKET_PATH;
use trueid_ipc::Response;

fn main() -> std::io::Result<()> {
    if Path::new(SOCKET_PATH).exists() {
        fs::remove_file(SOCKET_PATH)?;
    }

    let listener = UnixListener::bind(SOCKET_PATH)?;
    println!("TrueID daemon is running on {}", SOCKET_PATH);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                handle_connection(stream);
            }
            Err(err) => {
                eprintln!("Error accepting connection: {}", err);
            }
        }
    }

    Ok(())
}


fn handle_connection(mut stream: UnixStream) {
    use std::io::{Read, Write};
    let mut buffer = [0; 1024];
    let _ = stream.read(&mut buffer);
    let response_json = serde_json::to_string(&Response::Pong).unwrap();
    let _ = writeln!(stream, "{}", response_json);
}