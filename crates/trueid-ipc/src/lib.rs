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
