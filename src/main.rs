mod command;
mod configuration;
mod metrics;
mod racknerd;
mod scrape;
mod server;

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;

use command::Command;
use configuration::Configuration;
use racknerd::{HttpTransport, NodeClient};
use server::AppState;

#[tokio::main]
async fn main() -> ExitCode {
  let command = Command::parse();

  let addr: SocketAddr = match command.metrics_server.parse() {
    Ok(addr) => addr,
    Err(error) => {
      eprintln!(
        "metrics server {:?} is not a HOST:PORT address: {}",
        command.metrics_server, error
      );
      return ExitCode::FAILURE;
    }
  };

  let configuration = match Configuration::load(&command.config_path) {
    Ok(configuration) => configuration,
    Err(error) => {
      eprintln!("configuration error: {}", error);
      return ExitCode::FAILURE;
    }
  };

  let client = Arc::new(NodeClient::new(
    HttpTransport::new(),
    configuration.endpoint.clone(),
    configuration.node_timeout(),
  ));

  let served = server::run_web_server(
    addr,
    AppState {
      configuration,
      metrics_path: command.metrics_path.clone(),
      client,
    },
  )
  .await;

  match served {
    Ok(()) => ExitCode::SUCCESS,
    Err(error) => {
      eprintln!("server error: {}", error);
      ExitCode::FAILURE
    }
  }
}
