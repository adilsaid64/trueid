use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use trueid_ipc::{Request, Response, SOCKET_PATH};

fn main() -> std::io::Result<()> {
    let mut stream = UnixStream::connect(SOCKET_PATH)?;
    println!("Connected to daemon");

    let request = Request::Ping;

    let request_json = serde_json::to_string(&request).unwrap();

    writeln!(stream, "{}", request_json)?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    reader.read_line(&mut line)?;

    let response: Response = serde_json::from_str(&line).unwrap();

    println!("Response: {:?}", response);

    Ok(())
}