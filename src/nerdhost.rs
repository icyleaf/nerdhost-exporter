use hyper::{Client, Body};
use hyper::client::connect::HttpConnector;
use hyper_rustls::HttpsConnector;
use serde::Deserialize;

#[derive(Clone)]
pub struct SolusVM {
  pub endpoint: String,
  client: Client<HttpsConnector<HttpConnector>, Body>,
}

#[derive(Deserialize, Debug)]
pub struct ServerInfo {
  pub hostname: String,                               // Hostname of the VPS
  pub ipaddress: String,                              // IP address of the physical node: 1.1.1.1,2.2.2.2
  pub mem: String,                                    // Memory: total,used,free,percentused (bytes)
  pub hdd: String,                                    // HDD Disk: total,used,free,percentused (bytes)
  #[serde(rename(deserialize = "bw"))]
  pub bandwidth: String,                              // Bandwidth: total,used,free,percentused (bytes)
  pub vmstat: String,                                 // VM status
  pub status: String,                                 // API Status
  pub statusmsg: String                               // API Status message
}

#[derive(Deserialize, Debug)]
pub struct ServerStatus {
  pub hostname: String,                               // Hostname of the VPS
  pub ipaddress: String,                              // IP address of the physical node: 1.1.1.1,2.2.2.2
  #[serde(rename(deserialize = "vmstat"))]
  pub vm_status: String,                              // VM status
  pub status: String,                                 // API Status
  pub statusmsg: String                               // API Status message
}

impl ServerInfo {
  pub fn bandwidth_total(&self) -> i64 {
    let value = self.parse_bandwidth()[0];
    return value.parse::<i64>().unwrap();
  }

  pub fn bandwidth_used(&self) -> i64 {
    let value = self.parse_bandwidth()[1];
    return value.parse::<i64>().unwrap();
  }

  pub fn bandwidth_free(&self) -> i64 {
    let value = self.parse_bandwidth()[2];
    return value.parse::<i64>().unwrap();
  }

  pub fn parse_bandwidth(&self) -> Vec<&str> {
    return self.bandwidth.split(",").collect();
  }

  pub fn disk_total(&self) -> i64 {
    let value = self.parse_disk()[0];
    return value.parse::<i64>().unwrap();
  }

  pub fn disk_used(&self) -> i64 {
    let value = self.parse_disk()[1];
    return value.parse::<i64>().unwrap();
  }

  pub fn disk_free(&self) -> i64 {
    let value = self.parse_disk()[2];
    return value.parse::<i64>().unwrap();
  }

  pub fn parse_disk(&self) -> Vec<&str> {
    return self.hdd.split(",").collect();
  }
}

impl SolusVM {
  pub fn new(endpoint: String) -> SolusVM {
    let https = hyper_rustls::HttpsConnectorBuilder::new()
      .with_native_roots()
      .https_only()
      .enable_http1()
      .build();

    SolusVM {
      endpoint,
      client: Client::builder().build(https)
    }
  }

  #[allow(dead_code)]
  pub async fn get_service_info(&self, api_key: &String, api_secret: &String) -> Result<ServerInfo, hyper::Error> {
    let url = format!("{}/api/client/command.php?key={}&hash={}&action=info&bw=true&mem=true&hdd=true&ipaddr=true", self.endpoint, *api_key, *api_secret).parse().unwrap();
    let res = self.client.get(url).await?;
    let body_bytes = hyper::body::to_bytes(res.into_body()).await?;
    let body = String::from_utf8(body_bytes.to_vec()).unwrap();
    let xml_data = format!("<root>{}</root>", body);
    let service_info: ServerInfo = serde_xml_rs::from_reader(xml_data.as_bytes()).unwrap();

    Ok(service_info)
  }

  #[allow(dead_code)]
  pub async fn get_status(&self, api_key: &String, api_secret: &String) -> Result<ServerStatus, hyper::Error> {
    let url = format!("{}/api/client/command.php?key={}&hash={}&action=status", self.endpoint, *api_key, *api_secret).parse().unwrap();
    let res = self.client.get(url).await?;
    let body_bytes = hyper::body::to_bytes(res.into_body()).await?;
    let body = String::from_utf8(body_bytes.to_vec()).unwrap();
    let xml_data = format!("<root>{}</root>", body);
    let server_status: ServerStatus = serde_xml_rs::from_reader(xml_data.as_bytes()).unwrap();

    Ok(server_status)
  }
}
