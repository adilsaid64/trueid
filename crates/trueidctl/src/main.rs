use clap::{Parser, Subcommand};
use trueid_ipc::{Request, Response, send_request};

#[derive(Parser)]
#[command(version, about = "TrueID control tool", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Ping,
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
            Err(e) => {
                eprintln!("failed to reach trueidd: {e}");
                std::process::exit(1);
            }
        },
        None => {
            eprintln!("No subcommand. Try `trueidctl ping` or `--help`.");
            std::process::exit(2);
        }
    }
}
