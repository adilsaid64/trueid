use clap::{Parser, Subcommand};
use trueid_ipc::Response;
use trueid_ipc::Request;


#[derive(Parser)]
#[command(version, about, long_about = None)]
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
        Some(Commands::Ping) => {
            println!("Sending ping request to TrueID daemon...");
            let request = Request::Ping;
            // let response = trueid_ipc::send_request(request);
            // println!("Response: {:?}", response);
        }
        None => {
            println!("No command provided. Use --help for more information.");
        }
    }

}