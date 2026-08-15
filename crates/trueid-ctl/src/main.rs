use clap::{Parser, Subcommand};
use trueid_ipc::{Request, Response, send_request};
mod models;

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
    GetModels,
}

fn uid_or_current(uid: Option<u32>) -> u32 {
    uid.unwrap_or_else(current_uid)
}

fn rpc(request: Request) -> Response {
    match send_request(request) {
        Ok(Response::Error { message }) => {
            eprintln!("daemon error: {message}");
            std::process::exit(1);
        }
        Ok(response) => response,
        Err(e) => {
            eprintln!("failed to reach trueid-daemon: {e}");
            std::process::exit(1);
        }
    }
}

fn unexpected(op: &str) -> ! {
    eprintln!("unexpected response for {op}");
    std::process::exit(1);
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Ping) => match rpc(Request::Ping) {
            Response::Pong { ipc_version } => {
                println!("daemon ok (ipc protocol v{ipc_version})");
            }
            _ => unexpected("ping"),
        },
        Some(Commands::Verify { uid }) => {
            let uid = uid_or_current(uid);
            match rpc(Request::Verify { uid }) {
                Response::VerifyResult { accepted: true } => {
                    println!("verify accepted (uid {uid})");
                }
                Response::VerifyResult { accepted: false } => {
                    println!("verify rejected (uid {uid})");
                    std::process::exit(1);
                }
                _ => unexpected("verify"),
            }
        }
        Some(Commands::Enroll { uid }) => {
            let uid = uid_or_current(uid);
            match rpc(Request::Enroll { uid }) {
                Response::EnrollOk => println!("enrolled (uid {uid})"),
                _ => unexpected("enroll"),
            }
        }
        Some(Commands::AddTemplate { uid }) => {
            let uid = uid_or_current(uid);
            match rpc(Request::AddTemplate { uid }) {
                Response::AddTemplateOk => println!("added template (uid {uid})"),
                _ => unexpected("add-template"),
            }
        }
        Some(Commands::GetModels) => {
            if let Err(e) = models::get_models() {
                eprintln!("failed to install models: {e}");
                std::process::exit(1);
            }
        }
        None => {
            eprintln!("No subcommand. Try `trueid-ctl ping` or `--help`.");
            std::process::exit(2);
        }
    }
}
