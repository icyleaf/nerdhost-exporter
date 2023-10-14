mod configuration;
mod nerdhost;
mod metrics;
mod server;
mod command;
use command::Command;

// use hyper::{client::connect::HttpConnector, body};
// use hyper_rustls::HttpsConnector;
// use hyper::{Client, Body};
// use tokio::io::{stdout, AsyncWriteExt as _};
// use hyper::body::Buf;
// use serde::Deserialize;
// use std::str::from_utf8;

// use serde::Serialize;
// use byte_unit::Byte;

use std::net::SocketAddr;
use clap::Parser;

type MainResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::main]
async fn main() -> MainResult<()> {
  let command = Command::parse();
  let addr: SocketAddr = command.metrics_server
    .parse()
    .expect("Unable to parse metrics server");

  server::run_web_server(addr, command.clone()).await;

  // let https = hyper_rustls::HttpsConnectorBuilder::new()
  //     .with_native_roots()
  //     .https_only()
  //     .enable_http1()
  //     .build();

  // let client: Client<_, hyper::Body> = Client::builder().build(https);
  // let url = format!("https://nerdvm.racknerd.com/api/client/command.php?key=A3B5V-SW3HV-QWEA0&hash=ade93c72e424a816d439e729a5e294f6e897e137&action=info&bw=true&mem=true&hdd=true&ipaddr=true").parse()?;
  // let res = client.get(url).await?;
  // let body_bytes = hyper::body::to_bytes(res.into_body()).await?;
  // let body = String::from_utf8(body_bytes.to_vec()).unwrap();
  // let data = format!("<root>{}</root>", body);

  // println!("result: {}", data);

  // let server_info: nerdhost::ServerInfo = serde_xml_rs::from_str(data.as_str()).unwrap();
  // println!("deserialized = {:?}", server_info);

  // let hdds: Vec<&str> = server_info.hdd.split(",").collect();
  // let total_hdd = hdds[0];
  // let total_hdd_str = Byte::from_bytes(total_hdd.parse::<u128>().unwrap()).get_appropriate_unit(true).to_string();

  // println!("result: {}", total_hdd_str);

  // let service_info: nerdhost::ServerInfo = serde_xml_rs::from_reader(body.reader()).unwrap();

  // println!("service_info: {:?}", service_info);

  // println!("body: {:?}", body_bytes);

  Ok(())
}
