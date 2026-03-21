use std::fs;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;

use trueid_ipc::SOCKET_PATH;
use trueid_ipc::Response;

fn main() -> std::io::Result<()> {
    if Path::new(SOCKET_PATH).exists() {
        fs::remove_file(SOCKET_PATH)?;
    }

    // create and bind the unix socket listener
    // the dameon proceses will listen for connections
    let listener = UnixListener::bind(SOCKET_PATH)?;
    println!("TrueID daemon is running on {}", SOCKET_PATH);

    // main loop to accepting connections and handling them
    for stream in listener.incoming() {
        match stream {
            // on successful connection, we pass the stream to a handler function
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
    
    // creates an empty buffer, an array of 1024 bytes all initialised to 0
    let mut buffer = [0; 1024];
    
    // writes bytes to buffer
    let result = stream.read(&mut buffer);
    let bytes_read = result.unwrap();

    // covert bytes to string
    let message = String::from_utf8_lossy(&buffer[..bytes_read]);
    println!("Received message: {}", message);


    let request = serde_json::from_str::<String>(&message);

    let response = match request {
        Ok(req) => {
            println!("Parsed request: {}", req);
            Response::Pong
        }
        Err(err) => {
            eprintln!("Failed to parse request: {}", err);
            Response::Error("Invalid request format".to_string())
        }
    };

    // 
    let response_json = serde_json::to_string(&response).unwrap();
    let _ = writeln!(stream, "{}", response_json);
}