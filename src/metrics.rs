use prometheus::{self, IntGaugeVec, TextEncoder, Encoder};
use prometheus::{register_int_gauge_vec, opts};
use lazy_static::lazy_static;

use crate::nerdhost::{ServerInfo, ServerStatus};

lazy_static! {
  pub static ref BANDWIDTH_TOTAL: IntGaugeVec = register_int_gauge_vec!(
    opts!("nerdhost_bandwidth_total", "bandwidth total"),
    &["hostname", "ipaddress"]
  ).expect("Can't create a metric");

  pub static ref BANDWIDTH_FREE: IntGaugeVec = register_int_gauge_vec!(
    opts!("nerdhost_bandwidth_free", "bandwidth free"),
    &["hostname", "ipaddress"]
  ).expect("Can't create a metric");

  pub static ref BANDWIDTH_USED: IntGaugeVec = register_int_gauge_vec!(
    opts!("nerdhost_bandwidth_used", "bandwidth used"),
    &["hostname", "ipaddress"]
  ).expect("Can't create a metric");

  pub static ref DISK_TOTAL: IntGaugeVec = register_int_gauge_vec!(
    opts!("nerdhost_disk_total", "Disk total"),
    &["hostname", "ipaddress"]
  ).expect("Can't create a metric");

  pub static ref DISK_FREE: IntGaugeVec = register_int_gauge_vec!(
    opts!("nerdhost_disk_free", "Disk free"),
    &["hostname", "ipaddress"]
  ).expect("Can't create a metric");

  pub static ref DISK_USED: IntGaugeVec = register_int_gauge_vec!(
    opts!("nerdhost_disk_used", "Disk used"),
    &["hostname", "ipaddress"]
  ).expect("Can't create a metric");

  pub static ref SERVER_STATUS: IntGaugeVec = register_int_gauge_vec!(
    opts!("nerdhost_server_status", "Server status information"),
    &["hostname", "ipaddress", "vm_status"]
  ).expect("Can't create a metric");
}

pub fn set_bandwith_total(server_info: &ServerInfo) {
  BANDWIDTH_TOTAL
    .with_label_values(&[
      &server_info.hostname,
      &server_info.ipaddress,
    ])
    .set(server_info.bandwidth_total());
}

pub fn set_bandwith_free(server_info: &ServerInfo) {
  BANDWIDTH_FREE
    .with_label_values(&[
      &server_info.hostname,
      &server_info.ipaddress,
    ])
    .set(server_info.bandwidth_free());
}

pub fn set_bandwith_used(server_info: &ServerInfo) {
  BANDWIDTH_USED
    .with_label_values(&[
      &server_info.hostname,
      &server_info.ipaddress,
    ])
    .set(server_info.bandwidth_used());
}

pub fn set_disk_total(server_info: &ServerInfo) {
  DISK_TOTAL
    .with_label_values(&[
      &server_info.hostname,
      &server_info.ipaddress
    ])
    .set(server_info.disk_total());
}

pub fn set_disk_free(server_info: &ServerInfo) {
  DISK_FREE
    .with_label_values(&[
      &server_info.hostname,
      &server_info.ipaddress
    ])
    .set(server_info.disk_free());
}

pub fn set_disk_used(server_info: &ServerInfo) {
  DISK_USED
    .with_label_values(&[
      &server_info.hostname,
      &server_info.ipaddress
    ])
    .set(server_info.disk_used());
}

pub fn set_server_status(server_status: &ServerStatus) {
  let value = if server_status.vm_status == "online" { 1 } else { 0 };

  SERVER_STATUS
    .with_label_values(&[
      &server_status.hostname,
      &server_status.ipaddress,
      &server_status.vm_status
    ])
    .set(value);
}

pub fn render_prometheus_text_data() -> String {
  let encoder = TextEncoder::new();
  let mut buffer = vec![];
  encoder.encode(&prometheus::gather(), &mut buffer)
    .expect("Failed to encode metrics");

  let response = String::from_utf8(buffer.clone()).expect("Failed to convert bytes to string");
  buffer.clear();

  response
}
