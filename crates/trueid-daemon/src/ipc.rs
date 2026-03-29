use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;
use std::time::Instant;

use trueid_core::{TrueIdApp, UserId};
use trueid_ipc::{IPC_PROTOCOL_VERSION, Request, Response};

pub fn run_unix_socket(path: &str, app: Arc<TrueIdApp>) -> std::io::Result<()> {
    let listener = UnixListener::bind(path)?;
    tracing::info!(
        path,
        ipc_version = IPC_PROTOCOL_VERSION,
        "trueid-daemon listening (unix socket)"
    );

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let app = Arc::clone(&app);
                if let Err(e) = handle_connection(stream, &app) {
                    tracing::warn!(error = %e, "ipc: connection error");
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "ipc: accept error");
            }
        }
    }

    Ok(())
}

fn handle_connection(stream: UnixStream, app: &TrueIdApp) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let request: Request = serde_json::from_str(line.trim()).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid request json: {e}"),
        )
    })?;

    let response = dispatch(app, request);

    let mut stream = reader.into_inner();
    let body = serde_json::to_string(&response).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("response serialization: {e}"),
        )
    })?;
    writeln!(stream, "{body}")?;
    stream.flush()?;
    Ok(())
}

fn dispatch(app: &TrueIdApp, request: Request) -> Response {
    let t0 = Instant::now();
    let op = match &request {
        Request::Ping => "ping",
        Request::Verify { .. } => "verify",
        Request::Enroll { .. } => "enroll",
    };
    tracing::info!(op, ?request, "ipc: request");

    let response = match request {
        Request::Ping => match app.ping() {
            Ok(()) => Response::Pong {
                ipc_version: IPC_PROTOCOL_VERSION,
            },
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },
        Request::Verify { uid } => match app.verify(&UserId(uid)) {
            Ok(accepted) => Response::VerifyResult { accepted },
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },
        Request::Enroll { uid } => match app.enroll(&UserId(uid)) {
            Ok(()) => Response::EnrollOk,
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },
    };

    tracing::info!(
        op,
        elapsed_ms = t0.elapsed().as_millis(),
        ok = !matches!(&response, Response::Error { .. }),
        "ipc: done"
    );
    response
}
