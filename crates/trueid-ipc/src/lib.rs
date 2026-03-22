use serde::{Deserialize, Serialize};

pub const IPC_PROTOCOL_VERSION: u32 = 1;

pub const SOCKET_PATH: &str = "/tmp/trueid.sock";

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Ping,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Pong {
        ipc_version: u32,
    },
    Error {
        message: String,
    },
}

pub fn send_request(request: Request) -> std::io::Result<Response> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(SOCKET_PATH)?;

    let request_json = serde_json::to_string(&request)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    writeln!(stream, "{request_json}")?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    serde_json::from_str(line.trim())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_ping_roundtrip() {
        let json = serde_json::to_string(&Request::Ping).unwrap();
        assert_eq!(json, r#"{"op":"ping"}"#);
        let back: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Request::Ping);
    }

    #[test]
    fn response_roundtrip() {
        let r = Response::Pong {
            ipc_version: IPC_PROTOCOL_VERSION,
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }
}
