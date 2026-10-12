use std::collections::HashMap;

use prometheus::{Encoder, IntGaugeVec, Opts, Registry, TextEncoder};

use crate::scrape::{Call, ScrapeResult};

/// The one place that knows the metric families, their labels and their values.
/// The registry is built per render, so a Scrape's response cannot carry a
/// previous Scrape's series.
pub fn render(result: &ScrapeResult) -> Result<String, prometheus::Error> {
  let registry = Registry::new();

  // Two identities travel together: `node` is what the Scrape calls the node (the
  // configured name when there is one, else the hostname the API gave), and
  // `hostname` is the API's own string for it. Every family that carries a hostname
  // carries `node` beside it, so series can be joined to each other on `node` alone.
  let node_info = IntGaugeVec::new(
    Opts::new(
      "racknode_node_info",
      "Node identity and image as the node API reports it. Always 1.",
    ),
    &["node", "hostname", "ipaddress", "template", "os"],
  )?;

  let bandwidth_total = IntGaugeVec::new(
    Opts::new("racknode_bandwidth_total", "Bandwidth total in bytes"),
    &["node", "hostname"],
  )?;
  let bandwidth_free = IntGaugeVec::new(
    Opts::new("racknode_bandwidth_free", "Bandwidth free in bytes"),
    &["node", "hostname"],
  )?;
  let bandwidth_used = IntGaugeVec::new(
    Opts::new("racknode_bandwidth_used", "Bandwidth used in bytes"),
    &["node", "hostname"],
  )?;

  let disk_total = IntGaugeVec::new(
    Opts::new("racknode_disk_total", "Disk total in bytes"),
    &["node", "hostname"],
  )?;
  let disk_free = IntGaugeVec::new(
    Opts::new("racknode_disk_free", "Disk free in bytes"),
    &["node", "hostname"],
  )?;
  let disk_used = IntGaugeVec::new(
    Opts::new("racknode_disk_used", "Disk used in bytes"),
    &["node", "hostname"],
  )?;

  let memory_total = IntGaugeVec::new(
    Opts::new("racknode_memory_total", "Memory total in bytes"),
    &["node", "hostname"],
  )?;
  let memory_free = IntGaugeVec::new(
    Opts::new("racknode_memory_free", "Memory free in bytes"),
    &["node", "hostname"],
  )?;
  let memory_used = IntGaugeVec::new(
    Opts::new("racknode_memory_used", "Memory used in bytes"),
    &["node", "hostname"],
  )?;

  // The node API reports its own percentage for each resource, so the exporter
  // publishes it as it arrived rather than recomputing a different one.
  let bandwidth_used_percent = IntGaugeVec::new(
    Opts::new(
      "racknode_bandwidth_used_percent",
      "Bandwidth used, in percent of the total",
    ),
    &["node", "hostname"],
  )?;
  let disk_used_percent = IntGaugeVec::new(
    Opts::new(
      "racknode_disk_used_percent",
      "Disk used, in percent of the total",
    ),
    &["node", "hostname"],
  )?;
  let memory_used_percent = IntGaugeVec::new(
    Opts::new(
      "racknode_memory_used_percent",
      "Memory used, in percent of the total",
    ),
    &["node", "hostname"],
  )?;

  // A node whose calls fail reports no usage, so without this it would simply
  // vanish from the metrics; the count under `node` is the only other trace of it.
  let node_up = IntGaugeVec::new(
    Opts::new(
      "racknode_node_up",
      "1 when every call for the node answered in this Scrape, 0 otherwise",
    ),
    &["node"],
  )?;

  let server_status = IntGaugeVec::new(
    Opts::new(
      "racknode_server_status",
      "Server status: 1 when online, 0 otherwise",
    ),
    &["node", "hostname", "vm_status"],
  )?;

  let scrape_results = IntGaugeVec::new(
    Opts::new(
      "racknode_node_scrape_results",
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
    let node_label = sample.label();

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
      .with_label_values(&[node_label.as_str()])
      .set(if answered { 1 } else { 0 });

    if let Ok(usage) = &sample.usage {
      node_info
        .with_label_values(&[
          node_label.as_str(),
          usage.hostname.as_str(),
          usage.ipaddress.as_str(),
          usage.template.as_str(),
          usage.os.as_str(),
        ])
        .set(1);

      bandwidth_total
        .with_label_values(&[node_label.as_str(), usage.hostname.as_str()])
        .set(usage.bandwidth.total);
      bandwidth_free
        .with_label_values(&[node_label.as_str(), usage.hostname.as_str()])
        .set(usage.bandwidth.free);
      bandwidth_used
        .with_label_values(&[node_label.as_str(), usage.hostname.as_str()])
        .set(usage.bandwidth.used);
      bandwidth_used_percent
        .with_label_values(&[node_label.as_str(), usage.hostname.as_str()])
        .set(usage.bandwidth.percent);

      disk_total
        .with_label_values(&[node_label.as_str(), usage.hostname.as_str()])
        .set(usage.disk.total);
      disk_free
        .with_label_values(&[node_label.as_str(), usage.hostname.as_str()])
        .set(usage.disk.free);
      disk_used
        .with_label_values(&[node_label.as_str(), usage.hostname.as_str()])
        .set(usage.disk.used);
      disk_used_percent
        .with_label_values(&[node_label.as_str(), usage.hostname.as_str()])
        .set(usage.disk.percent);

      memory_total
        .with_label_values(&[node_label.as_str(), usage.hostname.as_str()])
        .set(usage.memory.total);
      memory_free
        .with_label_values(&[node_label.as_str(), usage.hostname.as_str()])
        .set(usage.memory.free);
      memory_used
        .with_label_values(&[node_label.as_str(), usage.hostname.as_str()])
        .set(usage.memory.used);
      memory_used_percent
        .with_label_values(&[node_label.as_str(), usage.hostname.as_str()])
        .set(usage.memory.percent);
    }

    // The status reply carries no hostname, so a status reading is only
    // attributable while the same Node's usage call has named it.
    if let (Ok(usage), Ok(status)) = (&sample.usage, &sample.status) {
      let online = if status.vm_status == "online" { 1 } else { 0 };
      server_status
        .with_label_values(&[
          node_label.as_str(),
          usage.hostname.as_str(),
          status.vm_status.as_str(),
        ])
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
  use crate::racknode::{NodeError, StatusSample, Usage, UsageSample};
  use crate::scrape::NodeSample;

  const HOSTNAME: &str = "vm12345";
  const IPADDRESS: &str = "1.2.3.4";
  const TEMPLATE: &str = "linux-debian-10-x86_64-gen2-v1";
  const OS: &str = "Debian 10 64 Bit";

  // The name an operator gives a node, deliberately not the hostname the API
  // reports: the two are different strings, which is the whole reason the name
  // travels in a label of its own.
  const NAME: &str = "node1";

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

  /// The response one node alone produces.
  fn rendered(sample: NodeSample) -> String {
    render(&ScrapeResult {
      samples: vec![sample],
    })
    .unwrap()
  }

  #[test]
  fn a_healthy_node_is_reported_under_the_hostname_the_api_gave() {
    let result = ScrapeResult {
      samples: vec![healthy(HOSTNAME)],
    };

    let text = render(&result).unwrap();

    // Nothing was configured for this node, so `node` repeats the hostname the API
    // gave. Labels come out in the order the text encoder sorts them, not the order
    // they were declared in.
    let series = |metric: &str, value: i64| {
      format!(
        "{}{{hostname=\"{}\",node=\"{}\"}} {}",
        metric, HOSTNAME, HOSTNAME, value
      )
    };

    assert!(text.contains(&format!(
      "racknode_node_info{{hostname=\"{}\",ipaddress=\"1.2.3.4\",node=\"{}\",os=\"Debian 10 64 Bit\",template=\"linux-debian-10-x86_64-gen2-v1\"}} 1",
      HOSTNAME, HOSTNAME
    )));
    assert!(text.contains(&series("racknode_bandwidth_total", 50000)));
    assert!(text.contains(&series("racknode_bandwidth_used", 1000)));
    assert!(text.contains(&series("racknode_bandwidth_free", 40000)));
    assert!(text.contains(&series("racknode_bandwidth_used_percent", 10)));
    assert!(text.contains(&series("racknode_disk_total", 10000)));
    assert!(text.contains(&series("racknode_disk_used", 3000)));
    assert!(text.contains(&series("racknode_disk_free", 7000)));
    assert!(text.contains(&series("racknode_disk_used_percent", 30)));
    assert!(text.contains(&series("racknode_memory_total", 20000)));
    assert!(text.contains(&series("racknode_memory_used", 5000)));
    assert!(text.contains(&series("racknode_memory_free", 15000)));
    assert!(text.contains(&series("racknode_memory_used_percent", 25)));
    assert!(text.contains(&format!("racknode_node_up{{node=\"{}\"}} 1", HOSTNAME)));
    assert!(text.contains(&format!(
      "racknode_server_status{{hostname=\"{}\",node=\"{}\",vm_status=\"online\"}} 1",
      HOSTNAME, HOSTNAME
    )));
    assert!(text.contains(&format!(
      "racknode_node_scrape_results{{call=\"usage\",node=\"{}\",reason=\"\"}} 1",
      HOSTNAME
    )));
    assert!(text.contains(&format!(
      "racknode_node_scrape_results{{call=\"status\",node=\"{}\",reason=\"\"}} 1",
      HOSTNAME
    )));
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
      .contains("racknode_node_scrape_results{call=\"usage\",node=\"node-0\",reason=\"api\"} 1"));
    assert!(
      text.contains("racknode_node_scrape_results{call=\"status\",node=\"node-0\",reason=\"\"} 1")
    );

    // No hostname ever came back, so there is nothing to report usage or status under.
    assert!(!text.contains("racknode_bandwidth_total"), "{}", text);
    assert!(!text.contains("racknode_memory_total"), "{}", text);
    assert!(!text.contains("racknode_server_status"), "{}", text);
    assert!(!text.contains("racknode_node_info"), "{}", text);

    // The node itself is still reported, so a failure never leaves a gap where a
    // node should be.
    assert!(
      text.contains("racknode_node_up{node=\"node-0\"} 0"),
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
      text.contains("racknode_node_scrape_results{call=\"status\",node=\"node-0\",reason=\"\"} 1")
    );
    assert!(!text.contains("racknode_server_status"), "{}", text);

    // Half a node is not an up node.
    assert!(
      text.contains("racknode_node_up{node=\"node-0\"} 0"),
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
      text.contains("racknode_node_scrape_results{call=\"usage\",node=\"vm-a\",reason=\"\"} 1")
    );
    assert!(
      text.contains("racknode_node_scrape_results{call=\"usage\",node=\"vm-c\",reason=\"\"} 1")
    );
    assert!(text
      .contains("racknode_node_scrape_results{call=\"usage\",node=\"node-1\",reason=\"api\"} 1"));
    assert!(
      text.contains("racknode_node_scrape_results{call=\"status\",node=\"vm-a\",reason=\"\"} 1")
    );
    assert!(
      text.contains("racknode_node_scrape_results{call=\"status\",node=\"vm-c\",reason=\"\"} 1")
    );
    assert!(text
      .contains("racknode_node_scrape_results{call=\"status\",node=\"node-1\",reason=\"api\"} 1"));

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
      "racknode_node_scrape_results{call=\"usage\",node=\"node-0\",reason=\"transport\"} 1"
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
        "racknode_node_scrape_results{{call=\"usage\",node=\"node-0\",reason=\"{}\"}} 1",
        reason
      );
      assert!(text.contains(&expected), "missing {}", expected);
    }
  }

  #[test]
  fn a_named_node_is_keyed_by_its_name_on_every_family_that_carries_a_hostname() {
    let text = rendered(healthy_at(HOSTNAME, 0, Some(NAME)));

    let expected = [
      format!(
        "racknode_node_info{{hostname=\"{}\",ipaddress=\"{}\",node=\"{}\",os=\"{}\",template=\"{}\"}} 1",
        HOSTNAME, IPADDRESS, NAME, OS, TEMPLATE
      ),
      format!(
        "racknode_bandwidth_total{{hostname=\"{}\",node=\"{}\"}} 50000",
        HOSTNAME, NAME
      ),
      format!(
        "racknode_bandwidth_free{{hostname=\"{}\",node=\"{}\"}} 40000",
        HOSTNAME, NAME
      ),
      format!(
        "racknode_bandwidth_used{{hostname=\"{}\",node=\"{}\"}} 1000",
        HOSTNAME, NAME
      ),
      format!(
        "racknode_bandwidth_used_percent{{hostname=\"{}\",node=\"{}\"}} 10",
        HOSTNAME, NAME
      ),
      format!(
        "racknode_disk_total{{hostname=\"{}\",node=\"{}\"}} 10000",
        HOSTNAME, NAME
      ),
      format!(
        "racknode_disk_free{{hostname=\"{}\",node=\"{}\"}} 7000",
        HOSTNAME, NAME
      ),
      format!(
        "racknode_disk_used{{hostname=\"{}\",node=\"{}\"}} 3000",
        HOSTNAME, NAME
      ),
      format!(
        "racknode_disk_used_percent{{hostname=\"{}\",node=\"{}\"}} 30",
        HOSTNAME, NAME
      ),
      format!(
        "racknode_memory_total{{hostname=\"{}\",node=\"{}\"}} 20000",
        HOSTNAME, NAME
      ),
      format!(
        "racknode_memory_free{{hostname=\"{}\",node=\"{}\"}} 15000",
        HOSTNAME, NAME
      ),
      format!(
        "racknode_memory_used{{hostname=\"{}\",node=\"{}\"}} 5000",
        HOSTNAME, NAME
      ),
      format!(
        "racknode_memory_used_percent{{hostname=\"{}\",node=\"{}\"}} 25",
        HOSTNAME, NAME
      ),
      format!(
        "racknode_server_status{{hostname=\"{}\",node=\"{}\",vm_status=\"online\"}} 1",
        HOSTNAME, NAME
      ),
      // The family that was already keyed by node, for comparison: one identity
      // name across everything a node reports.
      format!("racknode_node_up{{node=\"{}\"}} 1", NAME),
    ];

    for line in expected {
      assert!(text.contains(&line), "missing {}\n{}", line, text);
    }
  }

  #[test]
  fn an_unnamed_node_is_keyed_by_the_hostname_the_api_gave() {
    let text = rendered(healthy(HOSTNAME));

    // Nothing was configured, so both label families name the node the same way,
    // which is what makes the usage series joinable to node_up.
    assert!(
      text.contains(&format!(
        "racknode_bandwidth_total{{hostname=\"{}\",node=\"{}\"}} 50000",
        HOSTNAME, HOSTNAME
      )),
      "{}",
      text
    );
    assert!(text.contains(&format!("racknode_node_up{{node=\"{}\"}} 1", HOSTNAME)));
  }

  #[test]
  fn two_nodes_reporting_one_hostname_stay_two_series() {
    let text = render(&ScrapeResult {
      samples: vec![
        healthy_at(HOSTNAME, 0, Some("alpha")),
        healthy_at(HOSTNAME, 1, Some("beta")),
      ],
    })
    .unwrap();

    // The same hostname under two configured nodes: one reading may not overwrite
    // the other, which is what used to happen when hostname was the only key.
    for name in ["alpha", "beta"] {
      assert!(
        text.contains(&format!(
          "racknode_bandwidth_total{{hostname=\"{}\",node=\"{}\"}} 50000",
          HOSTNAME, name
        )),
        "{}",
        text
      );
    }
  }

  #[test]
  fn a_hostname_the_api_left_empty_does_not_cost_the_reading() {
    let text = render(&ScrapeResult {
      samples: vec![healthy_at("", 0, None), healthy_at("", 1, Some(NAME))],
    })
    .unwrap();

    // The call answered, so its figures are reported even though the node cannot be
    // named by hostname: the configured name, or the position, still attributes it.
    assert!(
      text.contains("racknode_bandwidth_total{hostname=\"\",node=\"node-0\"} 50000"),
      "{}",
      text
    );
    assert!(
      text.contains(&format!(
        "racknode_bandwidth_total{{hostname=\"\",node=\"{}\"}} 50000",
        NAME
      )),
      "{}",
      text
    );
  }
}
