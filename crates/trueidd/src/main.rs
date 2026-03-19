use std::fs;
use std::os::unix::net::UnixListener;
use std::path::Path;

use trueid_ipc::SOCKET_PATH;

fn main() -> std::io::Result<()> {
    if Path::new(SOCKET_PATH).exists() {
        fs::remove_file(SOCKET_PATH)?;
    }

    let listener = UnixListener::bind(SOCKET_PATH)?;
    println!("TrueID daemon is running on {}", SOCKET_PATH);

    for stream in listener.incoming() {
        match stream {
            Ok(_stream) => {
                println!("Client connected");
            }
            Err(err) => {
                eprintln!("Error accepting connection: {}", err);
            }
        }
    }

    Ok(())
}