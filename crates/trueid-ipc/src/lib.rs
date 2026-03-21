use serde::{Deserialize, Serialize};

pub const SOCKET_PATH: &str = "/tmp/trueid.sock";

#[derive(Serialize, Deserialize, Debug)]
pub enum Request {
    Ping,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Response {
    Pong,
    Error(String),
}

pub fn send_request(request: Request) -> std::io::Result<Response> {
    use std::os::unix::net::UnixStream;
    use std::io::{Write, BufRead, BufReader};

    // connect to daemon
    let mut stream = UnixStream::connect(SOCKET_PATH)?;

    // serialise request to json
    let request_json = serde_json::to_string(&request).unwrap();

    // write request
    writeln!(stream, "{}", request_json)?;

    // read response
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    // parse response
    let response: Response = serde_json::from_str(&line).unwrap();

    Ok(response)
}