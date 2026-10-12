use std::collections::HashMap;

use prometheus::{Encoder, IntGaugeVec, Opts, Registry, TextEncoder};

use crate::scrape::{Call, ScrapeResult};

/// The one place that knows the metric families, their labels and their values.
/// The registry is built per render, so a Scrape's response cannot carry a
/// previous Scrape's series.
pub fn render(result: &ScrapeResult) -> Result<String, prometheus::Error> {
  let registry = Registry::new();

  let node_info = IntGaugeVec::new(
    Opts::new(
      "nerdhost_node_info",
      "Node identity and image as the node API reports it. Always 1.",
    ),
    &["hostname", "ipaddress", "template", "os"],
  )?;

  let bandwidth_total = IntGaugeVec::new(
    Opts::new("nerdhost_bandwidth_total", "Bandwidth total in bytes"),
    &["hostname"],
  )?;
  let bandwidth_free = IntGaugeVec::new(
    Opts::new("nerdhost_bandwidth_free", "Bandwidth free in bytes"),
    &["hostname"],
  )?;
  let bandwidth_used = IntGaugeVec::new(
    Opts::new("nerdhost_bandwidth_used", "Bandwidth used in bytes"),
    &["hostname"],
  )?;

  let disk_total = IntGaugeVec::new(
    Opts::new("nerdhost_disk_total", "Disk total in bytes"),
    &["hostname"],
  )?;
  let disk_free = IntGaugeVec::new(
    Opts::new("nerdhost_disk_free", "Disk free in bytes"),
    &["hostname"],
  )?;
  let disk_used = IntGaugeVec::new(
    Opts::new("nerdhost_disk_used", "Disk used in bytes"),
    &["hostname"],
  )?;

  let memory_total = IntGaugeVec::new(
    Opts::new("nerdhost_memory_total", "Memory total in bytes"),
    &["hostname"],
  )?;
  let memory_free = IntGaugeVec::new(
    Opts::new("nerdhost_memory_free", "Memory free in bytes"),
    &["hostname"],
  )?;
  let memory_used = IntGaugeVec::new(
    Opts::new("nerdhost_memory_used", "Memory used in bytes"),
    &["hostname"],
  )?;

  // The node API reports its own percentage for each resource, so the exporter
  // publishes it as it arrived rather than recomputing a different one.
  let bandwidth_used_percent = IntGaugeVec::new(
    Opts::new(
      "nerdhost_bandwidth_used_percent",
      "Bandwidth used, in percent of the total",
    ),
    &["hostname"],
  )?;
  let disk_used_percent = IntGaugeVec::new(
    Opts::new(
      "nerdhost_disk_used_percent",
      "Disk used, in percent of the total",
    ),
    &["hostname"],
  )?;
  let memory_used_percent = IntGaugeVec::new(
    Opts::new(
      "nerdhost_memory_used_percent",
      "Memory used, in percent of the total",
    ),
    &["hostname"],
  )?;

  // A node whose calls fail reports no usage, so without this it would simply
  // vanish from the metrics; the count under `node` is the only other trace of it.
  let node_up = IntGaugeVec::new(
    Opts::new(
      "nerdhost_node_up",
      "1 when every call for the node answered in this Scrape, 0 otherwise",
    ),
    &["node"],
  )?;

  let server_status = IntGaugeVec::new(
    Opts::new(
      "nerdhost_server_status",
      "Server status: 1 when online, 0 otherwise",
    ),
    &["hostname", "vm_status"],
  )?;

  let scrape_results = IntGaugeVec::new(
    Opts::new(
      "nerdhost_node_scrape_results",
      "Node calls that ended this way in this Scrape. reason is empty when the call answered.",
    ),
    &["node", "call", "reason"],
  )?;

  for collector in [
    boxed(&node_info),
    boxed(&bandwidth_total),
    boxed(&bandwidth_free),
    boxed(&bandwidth_used),
    boxed(&disk_total),
    boxed(&disk_free),
    boxed(&disk_used),
    boxed(&memory_total),
    boxed(&memory_free),
    boxed(&memory_used),
    boxed(&bandwidth_used_percent),
    boxed(&disk_used_percent),
    boxed(&memory_used_percent),
    boxed(&server_status),
    boxed(&node_up),
    boxed(&scrape_results),
  ] {
    registry.register(collector)?;
  }

  let mut results: HashMap<(String, Call, &'static str), i64> = HashMap::new();

  for sample in &result.samples {
    let node_label = sample
      .identity
      .name
      .as_deref()
      .or_else(|| sample.usage.as_ref().ok().map(|u| u.hostname.as_str()))
      .filter(|s| !s.is_empty())
      .map(|s| s.to_string())
      .unwrap_or_else(|| sample.identity.display_name());

    for call in Call::ALL {
      let reason = match sample.outcome(call) {
        Ok(()) => "",
        Err(error) => error.reason(),
      };
      *results
        .entry((node_label.clone(), call, reason))
        .or_insert(0) += 1;
    }

    // Both calls have to answer for the node to count as up: a node the exporter
    // can only half read is not one it can report on.
    let answered = Call::ALL.iter().all(|call| sample.outcome(*call).is_ok());
    node_up
      .with_label_values(&[&node_label])
      .set(if answered { 1 } else { 0 });

    if let Ok(usage) = &sample.usage {
      node_info
        .with_label_values(&[
          &usage.hostname,
          &usage.ipaddress,
          &usage.template,
          &usage.os,
        ])
        .set(1);

      bandwidth_total
        .with_label_values(&[&usage.hostname])
        .set(usage.bandwidth.total);
      bandwidth_free
        .with_label_values(&[&usage.hostname])
        .set(usage.bandwidth.free);
      bandwidth_used
        .with_label_values(&[&usage.hostname])
        .set(usage.bandwidth.used);
      bandwidth_used_percent
        .with_label_values(&[&usage.hostname])
        .set(usage.bandwidth.percent);

      disk_total
        .with_label_values(&[&usage.hostname])
        .set(usage.disk.total);
      disk_free
        .with_label_values(&[&usage.hostname])
        .set(usage.disk.free);
      disk_used
        .with_label_values(&[&usage.hostname])
        .set(usage.disk.used);
      disk_used_percent
        .with_label_values(&[&usage.hostname])
        .set(usage.disk.percent);

      memory_total
        .with_label_values(&[&usage.hostname])
        .set(usage.memory.total);
      memory_free
        .with_label_values(&[&usage.hostname])
        .set(usage.memory.free);
      memory_used
        .with_label_values(&[&usage.hostname])
        .set(usage.memory.used);
      memory_used_percent
        .with_label_values(&[&usage.hostname])
        .set(usage.memory.percent);
    }

    // The status reply carries no hostname, so a status reading is only
    // attributable while the same Node's usage call has named it.
    if let (Ok(usage), Ok(status)) = (&sample.usage, &sample.status) {
      let online = if status.vm_status == "online" { 1 } else { 0 };
      server_status
        .with_label_values(&[&usage.hostname, &status.vm_status])
        .set(online);
    }
  }

  for ((node_label, call, reason), count) in results {
    scrape_results
      // One element type for the whole slice: prometheus 0.14 takes `&[V]` with a
      // single `V: AsRef<str>`, so mixing `&String` with `&str` does not infer.
      .with_label_values(&[node_label.as_str(), call.as_str(), reason])
      .set(count);
  }

  let encoder = TextEncoder::new();
  let mut buffer = Vec::new();
  encoder.encode(&registry.gather(), &mut buffer)?;

  String::from_utf8(buffer)
    .map_err(|e| prometheus::Error::Msg(format!("metrics are not utf-8: {}", e)))
}

fn boxed(gauge: &IntGaugeVec) -> Box<dyn prometheus::core::Collector> {
  Box::new(gauge.clone())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::nerdhost::{NodeError, StatusSample, Usage, UsageSample};
  use crate::scrape::NodeSample;

  const HOSTNAME: &str = "vm12345";
  const IPADDRESS: &str = "1.2.3.4";
  const TEMPLATE: &str = "linux-debian-10-x86_64-gen2-v1";
  const OS: &str = "Debian 10 64 Bit";

  fn usage_sample(hostname: &str) -> UsageSample {
    UsageSample {
      hostname: hostname.to_string(),
      ipaddress: IPADDRESS.to_string(),
      template: TEMPLATE.to_string(),
      os: OS.to_string(),
      bandwidth: Usage {
        total: 50000,
        used: 1000,
        free: 40000,
        percent: 10,
      },
      disk: Usage {
        total: 10000,
        used: 3000,
        free: 7000,
        percent: 30,
      },
      memory: Usage {
        total: 20000,
        used: 5000,
        free: 15000,
        percent: 25,
      },
    }
  }

  fn status_sample(vm_status: &str) -> StatusSample {
    StatusSample {
      vm_status: vm_status.to_string(),
    }
  }

  fn identity(index: usize, name: Option<&str>) -> crate::scrape::NodeIdentity {
    crate::scrape::NodeIdentity {
      name: name.map(|s| s.to_string()),
      index,
    }
  }

  fn healthy(hostname: &str) -> NodeSample {
    healthy_at(hostname, 0, None)
  }

  fn healthy_at(hostname: &str, index: usize, name: Option<&str>) -> NodeSample {
    NodeSample {
      identity: identity(index, name),
      usage: Ok(usage_sample(hostname)),
      status: Ok(status_sample("online")),
    }
  }

  #[test]
  fn a_healthy_node_is_reported_under_the_hostname_the_api_gave() {
    let result = ScrapeResult {
      samples: vec![healthy(HOSTNAME)],
    };

    let text = render(&result).unwrap();

    // Labels come out in the order the text encoder sorts them, not the order
    // they were declared in.
    assert!(text.contains(
      "nerdhost_node_info{hostname=\"vm12345\",ipaddress=\"1.2.3.4\",os=\"Debian 10 64 Bit\",template=\"linux-debian-10-x86_64-gen2-v1\"} 1"
    ));
    assert!(text.contains("nerdhost_bandwidth_used{hostname=\"vm12345\"} 1000"));
    assert!(
      text.contains("nerdhost_bandwidth_used_percent{hostname=\"vm12345\"} 10")
    );
    assert!(text.contains("nerdhost_disk_used_percent{hostname=\"vm12345\"} 30"));
    assert!(text.contains("nerdhost_memory_used_percent{hostname=\"vm12345\"} 25"));
    assert!(text.contains("nerdhost_node_up{node=\"vm12345\"} 1"));
    assert!(text.contains("nerdhost_disk_free{hostname=\"vm12345\"} 7000"));
    assert!(text.contains("nerdhost_memory_total{hostname=\"vm12345\"} 20000"));
    assert!(text.contains("nerdhost_memory_used{hostname=\"vm12345\"} 5000"));
    assert!(text.contains("nerdhost_memory_free{hostname=\"vm12345\"} 15000"));
    assert!(text.contains(
      "nerdhost_server_status{hostname=\"vm12345\",vm_status=\"online\"} 1"
    ));
    assert!(text.contains(
      "nerdhost_node_scrape_results{call=\"usage\",node=\"vm12345\",reason=\"\"} 1"
    ));
    assert!(text.contains("nerdhost_node_scrape_results{call=\"status\",node=\"vm12345\",reason=\"\"} 1"));
  }

  #[test]
  fn a_failed_call_is_counted_under_its_reason_and_carries_no_hostname() {
    let result = ScrapeResult {
      samples: vec![NodeSample {
        identity: identity(0, None),
        usage: Err(NodeError::Api("Invalid key or hash".to_string())),
        status: Ok(status_sample("offline")),
      }],
    };

    let text = render(&result).unwrap();

    assert!(text
      .contains("nerdhost_node_scrape_results{call=\"usage\",node=\"node-0\",reason=\"api\"} 1"));
    assert!(
      text.contains("nerdhost_node_scrape_results{call=\"status\",node=\"node-0\",reason=\"\"} 1")
    );

    // No hostname ever came back, so there is nothing to report usage or status under.
    assert!(!text.contains("nerdhost_bandwidth_total"), "{}", text);
    assert!(!text.contains("nerdhost_memory_total"), "{}", text);
    assert!(!text.contains("nerdhost_server_status"), "{}", text);
    assert!(!text.contains("nerdhost_node_info"), "{}", text);

    // The node itself is still reported, so a failure never leaves a gap where a
    // node should be.
    assert!(
      text.contains("nerdhost_node_up{node=\"node-0\"} 0"),
      "{}",
      text
    );
  }

  #[test]
  fn a_status_reading_without_a_hostname_is_not_attributed_to_anyone() {
    let result = ScrapeResult {
      samples: vec![NodeSample {
        identity: identity(0, None),
        usage: Err(NodeError::Transport("connection refused".to_string())),
        status: Ok(status_sample("online")),
      }],
    };

    let text = render(&result).unwrap();

    // The status call answered, and it is still counted.
    assert!(
      text.contains("nerdhost_node_scrape_results{call=\"status\",node=\"node-0\",reason=\"\"} 1")
    );
    assert!(!text.contains("nerdhost_server_status"), "{}", text);

    // Half a node is not an up node.
    assert!(
      text.contains("nerdhost_node_up{node=\"node-0\"} 0"),
      "{}",
      text
    );
  }

  #[test]
  fn outcomes_are_counted_across_nodes_rather_than_overwriting_each_other() {
    let result = ScrapeResult {
      samples: vec![
        healthy_at("vm-a", 0, None),
        NodeSample {
          identity: identity(1, None),
          usage: Err(NodeError::Api("Invalid key or hash".to_string())),
          status: Err(NodeError::Api("Invalid key or hash".to_string())),
        },
        healthy_at("vm-c", 2, None),
      ],
    };

    let text = render(&result).unwrap();

    assert!(
      text.contains("nerdhost_node_scrape_results{call=\"usage\",node=\"vm-a\",reason=\"\"} 1")
    );
    assert!(
      text.contains("nerdhost_node_scrape_results{call=\"usage\",node=\"vm-c\",reason=\"\"} 1")
    );
    assert!(text
      .contains("nerdhost_node_scrape_results{call=\"usage\",node=\"node-1\",reason=\"api\"} 1"));
    assert!(
      text.contains("nerdhost_node_scrape_results{call=\"status\",node=\"vm-a\",reason=\"\"} 1")
    );
    assert!(
      text.contains("nerdhost_node_scrape_results{call=\"status\",node=\"vm-c\",reason=\"\"} 1")
    );
    assert!(text
      .contains("nerdhost_node_scrape_results{call=\"status\",node=\"node-1\",reason=\"api\"} 1"));

    // Three nodes, so six calls in total.
    let counted: i64 = [2, 1, 2, 1].iter().sum();
    assert_eq!(counted, 6);
  }

  #[test]
  fn every_render_starts_from_an_empty_registry() {
    let first = ScrapeResult {
      samples: vec![healthy("vm-a")],
    };
    let second = ScrapeResult {
      samples: vec![NodeSample {
        identity: identity(0, None),
        usage: Err(NodeError::Transport("connection refused".to_string())),
        status: Err(NodeError::Transport("connection refused".to_string())),
      }],
    };

    render(&first).unwrap();
    let text = render(&second).unwrap();

    // The previous Scrape's series are gone, rather than accumulating.
    assert!(!text.contains("vm-a"), "{}", text);
    assert!(text.contains(
      "nerdhost_node_scrape_results{call=\"usage\",node=\"node-0\",reason=\"transport\"} 1"
    ));
  }

  #[test]
  fn the_reason_label_separates_the_three_ways_a_call_dies() {
    for (error, reason) in [
      (
        NodeError::Transport("connection refused".to_string()),
        "transport",
      ),
      (NodeError::Api("Invalid key or hash".to_string()), "api"),
      (NodeError::Parse("response has no hdd".to_string()), "parse"),
    ] {
      let result = ScrapeResult {
        samples: vec![NodeSample {
          identity: identity(0, None),
          usage: Err(error),
          status: Err(NodeError::Api("Invalid key or hash".to_string())),
        }],
      };

      let text = render(&result).unwrap();
      let expected = format!(
        "nerdhost_node_scrape_results{{call=\"usage\",node=\"node-0\",reason=\"{}\"}} 1",
        reason
      );
      assert!(text.contains(&expected), "missing {}", expected);
    }
  }
}
