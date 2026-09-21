use clap::Parser;
use compact_mcp::client::Client;
use serde_json::{Value, json};
use std::{io::Read, path::PathBuf, time::Duration};

#[derive(Parser)]
#[command(version, about = "Blender bridge: JSON in, JSON out")]
struct Args {
    #[arg(value_parser = ["inspect", "discover", "execute", "capture"])]
    method: String,
    #[arg(long, default_value = "{}")]
    params: String,
    #[arg(long)]
    connection: Option<PathBuf>,
    #[arg(long, default_value_t = 180, value_parser = clap::value_parser!(u64).range(1..=86400))]
    timeout: u64,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args = Args::parse();
    let run = async {
        let mut input = args.params;
        if input == "-" {
            input.clear();
            std::io::stdin()
                .take(compact_mcp::client::MAX_REQUEST as u64 + 1)
                .read_to_string(&mut input)?;
        }
        anyhow::ensure!(
            input.len() <= compact_mcp::client::MAX_REQUEST,
            "Request exceeds 256 KiB"
        );
        let params: Value = serde_json::from_str(&input)?;
        Client::discover(args.connection.as_deref())?
            .call(&args.method, params, Duration::from_secs(args.timeout))
            .await
    }
    .await;
    match run {
        Ok(value) => {
            println!("{value}");
            if value["ok"] == false {
                std::process::ExitCode::FAILURE
            } else {
                std::process::ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("{}", json!({"error":format!("{error:#}")}));
            std::process::ExitCode::FAILURE
        }
    }
}
