use clap::{Parser, Subcommand};
use trueid_ipc::{Request, Response, send_request};

#[cfg(unix)]
fn current_uid() -> u32 {
    unsafe { libc::getuid() }
}

#[cfg(not(unix))]
fn current_uid() -> u32 {
    0
}

#[derive(Parser)]
#[command(name = "trueid-ctl", version, about = "TrueID control tool", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Ping,
    Verify {
        /// Linux uid whose template to check (default: your uid, same as `id -u`)
        #[arg(long)]
        uid: Option<u32>,
    },
    Enroll {
        /// Linux uid to enroll (default: your uid, same as `id -u`)
        #[arg(long)]
        uid: Option<u32>,
    },
    /// Append a new face template from a capture (does not remove existing templates).
    AddTemplate {
        /// Linux uid (default: your uid, same as `id -u`)
        #[arg(long)]
        uid: Option<u32>,
    },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Ping) => match send_request(Request::Ping) {
            Ok(Response::Pong { ipc_version }) => {
                println!("daemon ok (ipc protocol v{ipc_version})");
            }
            Ok(Response::Error { message }) => {
                eprintln!("daemon error: {message}");
                std::process::exit(1);
            }
            Ok(_) => {
                eprintln!("unexpected response for ping");
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("failed to reach trueid-daemon: {e}");
                std::process::exit(1);
            }
        },
        Some(Commands::Verify { uid }) => {
            let uid = uid.unwrap_or_else(current_uid);
            match send_request(Request::Verify { uid }) {
                Ok(Response::VerifyResult { accepted }) => {
                    if accepted {
                        println!("verify accepted (uid {uid})");
                    } else {
                        println!("verify rejected (uid {uid})");
                        std::process::exit(1);
                    }
                }
                Ok(Response::Error { message }) => {
                    eprintln!("daemon error: {message}");
                    std::process::exit(1);
                }
                Ok(_) => {
                    eprintln!("unexpected response for verify");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("failed to reach trueid-daemon: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Enroll { uid }) => {
            let uid = uid.unwrap_or_else(current_uid);
            match send_request(Request::Enroll { uid }) {
                Ok(Response::EnrollOk) => {
                    println!("enrolled (uid {uid})");
                }
                Ok(Response::Error { message }) => {
                    eprintln!("daemon error: {message}");
                    std::process::exit(1);
                }
                Ok(_) => {
                    eprintln!("unexpected response for enroll");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("failed to reach trueid-daemon: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::AddTemplate { uid }) => {
            let uid = uid.unwrap_or_else(current_uid);
            match send_request(Request::AddTemplate { uid }) {
                Ok(Response::AddTemplateOk) => {
                    println!("added template (uid {uid})");
                }
                Ok(Response::Error { message }) => {
                    eprintln!("daemon error: {message}");
                    std::process::exit(1);
                }
                Ok(_) => {
                    eprintln!("unexpected response for add-template");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("failed to reach trueid-daemon: {e}");
                    std::process::exit(1);
                }
            }
        }
        None => {
            eprintln!("No subcommand. Try `trueid-ctl ping` or `--help`.");
            std::process::exit(2);
        }
    }
}
