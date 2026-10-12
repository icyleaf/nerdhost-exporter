use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio::time::{timeout_at, Instant};

use crate::racknode::{Credentials, NodeClient, NodeError, StatusSample, Transport, UsageSample};

/// The two calls a Scrape makes per Node. They answer independently, so each has
/// its own outcome: a Node can report its usage and fail to report its status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Call {
  Usage,
  Status,
}

impl Call {
  pub const ALL: [Call; 2] = [Call::Usage, Call::Status];

  /// The `call` label this call is reported under.
  pub fn as_str(self) -> &'static str {
    match self {
      Call::Usage => "usage",
      Call::Status => "status",
    }
  }
}

/// What one Node contributes to a Scrape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeIdentity {
  pub name: Option<String>,
  pub index: usize,
}

impl NodeIdentity {
  pub fn display_name(&self) -> String {
    match &self.name {
      Some(name) if !name.is_empty() => name.clone(),
      _ => format!("node-{}", self.index),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeSample {
  pub identity: NodeIdentity,
  pub usage: Result<UsageSample, NodeError>,
  pub status: Result<StatusSample, NodeError>,
}

impl NodeSample {
  /// The identity every series for this Node is reported under: what the operator
  /// named it, else the hostname the API gave, else its position. The metrics
  /// renderer asks for this rather than resolving it a second way.
  ///
  /// An empty string counts as absent at both steps, so a node configured with
  /// `name: ""` is reported like an unnamed one instead of jumping straight to its
  /// position while a hostname was there to be used.
  pub fn label(&self) -> String {
    let hostname = || {
      self
        .usage
        .as_ref()
        .ok()
        .map(|usage| usage.hostname.as_str())
        .filter(|hostname| !hostname.is_empty())
    };

    self
      .identity
      .name
      .as_deref()
      .filter(|name| !name.is_empty())
      .or_else(hostname)
      .map(|label| label.to_string())
      .unwrap_or_else(|| self.identity.display_name())
  }

  /// The outcome of one call, as the failure counts need to see it.
  pub fn outcome(&self, call: Call) -> Result<(), &NodeError> {
    match call {
      Call::Usage => self.usage.as_ref().map(|_| ()),
      Call::Status => self.status.as_ref().map(|_| ()),
    }
  }
}

/// Everything one Scrape obtained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrapeResult {
  pub samples: Vec<NodeSample>,
}

/// A call that has been started and has to be collected.
type PendingCall = (usize, Call, JoinHandle<(usize, CallOutcome)>);

/// What one spawned call came back with.
enum CallOutcome {
  Usage(Result<UsageSample, NodeError>),
  Status(Result<StatusSample, NodeError>),
}

fn failed(call: Call, error: NodeError) -> CallOutcome {
  match call {
    Call::Usage => CallOutcome::Usage(Err(error)),
    Call::Status => CallOutcome::Status(Err(error)),
  }
}

/// One Scrape: every Node, polled within the concurrency bound and the deadline,
/// then reported as exactly the results that Scrape obtained.
pub async fn run<T>(
  client: Arc<NodeClient<T>>,
  nodes: &[Credentials],
  max_concurrency: usize,
  deadline: Duration,
) -> ScrapeResult
where
  T: Transport + 'static,
{
  let permits = Arc::new(Semaphore::new(max_concurrency.max(1)));
  let mut calls: Vec<PendingCall> = Vec::with_capacity(nodes.len() * Call::ALL.len());

  for (position, credentials) in nodes.iter().enumerate() {
    for call in Call::ALL {
      let client = Arc::clone(&client);
      let permits = Arc::clone(&permits);
      let credentials = credentials.clone();

      calls.push((
        position,
        call,
        tokio::spawn(async move {
          let _permit = permits
            .acquire_owned()
            .await
            .expect("the concurrency semaphore is never closed");

          let outcome = match call {
            Call::Usage => CallOutcome::Usage(client.usage(&credentials).await),
            Call::Status => CallOutcome::Status(client.status(&credentials).await),
          };

          (position, outcome)
        }),
      ));
    }
  }

  let until = Instant::now() + deadline;
  let mut usage: HashMap<usize, Result<UsageSample, NodeError>> = HashMap::new();
  let mut status: HashMap<usize, Result<StatusSample, NodeError>> = HashMap::new();

  // Every call is already running, so awaiting the handles in turn collects
  // concurrent results rather than serialising the work.
  for (position, call, mut handle) in calls {
    let outcome = match timeout_at(until, &mut handle).await {
      Ok(Ok((_, outcome))) => outcome,
      Ok(Err(join_error)) => failed(
        call,
        NodeError::Transport(format!("call task failed: {}", join_error)),
      ),
      // The deadline has passed. A call that finished anyway still counts.
      Err(_) if handle.is_finished() => match handle.await {
        Ok((_, outcome)) => outcome,
        Err(join_error) => failed(
          call,
          NodeError::Transport(format!("call task failed: {}", join_error)),
        ),
      },
      Err(_) => {
        handle.abort();
        failed(
          call,
          NodeError::Transport(format!(
            "Scrape deadline of {}s elapsed",
            deadline.as_secs()
          )),
        )
      }
    };

    match outcome {
      CallOutcome::Usage(result) => {
        usage.insert(position, result);
      }
      CallOutcome::Status(result) => {
        status.insert(position, result);
      }
    }
  }

  let samples: Vec<NodeSample> = nodes
    .iter()
    .enumerate()
    .map(|(position, creds)| NodeSample {
      identity: NodeIdentity {
        name: creds.name.clone(),
        index: position,
      },
      usage: usage
        .remove(&position)
        .unwrap_or_else(|| Err(NodeError::Transport("call never ran".to_string()))),
      status: status
        .remove(&position)
        .unwrap_or_else(|| Err(NodeError::Transport("call never ran".to_string()))),
    })
    .collect();

  for sample in samples.iter() {
    for call in Call::ALL {
      if let Err(error) = sample.outcome(call) {
        log_failure(&sample.identity, call, error);
      }
    }
  }

  ScrapeResult { samples }
}

/// The `reason` label says which kind of failure it was; only the log says what it
/// actually said, because a message is not safe to put in a label.
fn log_failure(identity: &NodeIdentity, call: Call, error: &NodeError) {
  eprintln!(
    "scrape: {} {} failed ({}): {}",
    identity.display_name(),
    call.as_str(),
    error.reason(),
    error.detail()
  );
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::racknode::testing::{status_body, usage_body, ScriptedTransport};
  use crate::racknode::{BoxFuture, Usage};
  use std::sync::Mutex;

  fn credentials(key: &str) -> Credentials {
    Credentials {
      api_key: key.to_string(),
      api_secret: "abcdef".to_string(),
      name: None,
    }
  }

  /// A transport that answers `info` and `status` correctly, taking `delay` over each call.
  fn healthy(delay: Duration) -> NodeClient<ScriptedTransport> {
    NodeClient::new(
      ScriptedTransport::new(move |url| {
        let body = if url.contains("action=info") {
          usage_body("vm1", "1.2.3.4", 1000)
        } else {
          status_body("online")
        };
        (Ok(body), delay)
      }),
      "https://ctrl.example.com",
      Duration::from_millis(500),
    )
  }

  /// Counts how many calls are in flight at once, so the bound is observable.
  #[derive(Default)]
  struct ConcurrencyCounters {
    in_flight: Mutex<usize>,
    peak: Mutex<usize>,
  }

  struct CountingTransport {
    counters: Arc<ConcurrencyCounters>,
  }

  impl Transport for CountingTransport {
    fn get<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<String, NodeError>> {
      Box::pin(async move {
        let current = {
          let mut in_flight = self.counters.in_flight.lock().unwrap();
          *in_flight += 1;
          *in_flight
        };
        {
          let mut peak = self.counters.peak.lock().unwrap();
          *peak = (*peak).max(current);
        }

        tokio::time::sleep(Duration::from_millis(30)).await;

        *self.counters.in_flight.lock().unwrap() -= 1;

        let body = if url.contains("action=info") {
          usage_body("vm1", "1.2.3.4", 1)
        } else {
          status_body("online")
        };
        Ok(body)
      })
    }
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
  async fn one_scrape_reports_every_node() {
    let client = Arc::new(healthy(Duration::ZERO));
    let nodes = vec![credentials("11111111"), credentials("22222222")];

    let result = run(client, &nodes, 4, Duration::from_secs(1)).await;

    assert_eq!(result.samples.len(), 2);

    for sample in &result.samples {
      assert_eq!(sample.usage.as_ref().unwrap().bandwidth.used, 1000);
      assert_eq!(sample.status.as_ref().unwrap().vm_status, "online");
      assert_eq!(sample.outcome(Call::Usage), Ok(()));
      assert_eq!(sample.outcome(Call::Status), Ok(()));
    }
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
  async fn the_two_calls_fail_independently() {
    let client = Arc::new(NodeClient::new(
      ScriptedTransport::new(|url| {
        if url.contains("action=info") {
          (
            Err(NodeError::Transport("connection refused".to_string())),
            Duration::ZERO,
          )
        } else {
          (Ok(status_body("online")), Duration::ZERO)
        }
      }),
      "https://ctrl.example.com",
      Duration::from_millis(500),
    ));
    let nodes = vec![credentials("11111111")];

    let result = run(client, &nodes, 4, Duration::from_secs(1)).await;
    let sample = &result.samples[0];

    let usage_error = sample.usage.as_ref().unwrap_err();
    assert_eq!(usage_error.reason(), "transport");
    assert_eq!(sample.status.as_ref().unwrap().vm_status, "online");
    assert!(sample.outcome(Call::Usage).is_err());
    assert_eq!(sample.outcome(Call::Status), Ok(()));
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
  async fn calls_past_the_deadline_become_node_failures() {
    let client = Arc::new(healthy(Duration::from_millis(400)));
    let nodes = vec![credentials("11111111"), credentials("22222222")];

    let result = run(client, &nodes, 4, Duration::from_millis(40)).await;

    assert_eq!(result.samples.len(), 2);
    for sample in &result.samples {
      for call in Call::ALL {
        let error = match call {
          Call::Usage => sample.usage.as_ref().unwrap_err(),
          Call::Status => sample.status.as_ref().unwrap_err(),
        };
        assert_eq!(error.reason(), "transport");
        assert!(error.detail().contains("deadline"), "{}", error);
      }
    }
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
  async fn the_concurrency_bound_is_respected() {
    let counters = Arc::new(ConcurrencyCounters::default());
    let client = Arc::new(NodeClient::new(
      CountingTransport {
        counters: Arc::clone(&counters),
      },
      "https://ctrl.example.com",
      Duration::from_secs(2),
    ));
    let nodes = vec![
      credentials("1"),
      credentials("2"),
      credentials("3"),
      credentials("4"),
    ];

    let result = run(client, &nodes, 2, Duration::from_secs(5)).await;

    assert!(result.samples.iter().all(|s| s.usage.is_ok()));
    assert!(result.samples.iter().all(|s| s.status.is_ok()));
    let peak = *counters.peak.lock().unwrap();
    assert!(peak <= 2, "peak in flight was {}", peak);
  }

  /// One resource as the node API hands it back: the figures a label test does not
  /// care about, only the ones that are actually read.
  fn a_resource() -> Usage {
    Usage {
      total: 1000,
      used: 100,
      free: 900,
      percent: 10,
    }
  }

  fn usage_of(hostname: &str) -> UsageSample {
    UsageSample {
      hostname: hostname.to_string(),
      ipaddress: "1.2.3.4".to_string(),
      template: "linux-debian-10-x86_64-gen2-v1".to_string(),
      os: "Debian 10 64 Bit".to_string(),
      bandwidth: a_resource(),
      memory: a_resource(),
      disk: a_resource(),
    }
  }

  fn sample(index: usize, name: Option<&str>, usage: Result<&str, NodeError>) -> NodeSample {
    NodeSample {
      identity: NodeIdentity {
        name: name.map(|name| name.to_string()),
        index,
      },
      usage: usage.map(usage_of),
      status: Err(NodeError::Transport("not read by a label".to_string())),
    }
  }

  // The label is what every series for a node is keyed by, so these are the
  // precedence rules the metrics renderer relies on rather than re-derives.

  #[test]
  fn a_named_node_is_reported_under_its_name() {
    let sample = sample(0, Some("node1"), Ok("vm12345"));

    // The API hostname is not what the operator called this node.
    assert_eq!(sample.label(), "node1");
  }

  #[test]
  fn an_unnamed_node_is_reported_under_the_hostname_the_api_gave() {
    let sample = sample(0, None, Ok("vm12345"));

    assert_eq!(sample.label(), "vm12345");
  }

  #[test]
  fn a_node_with_no_name_whose_call_failed_is_reported_under_its_position() {
    let sample = sample(3, None, Err(NodeError::Transport("refused".to_string())));

    assert_eq!(sample.label(), "node-3");
  }

  #[test]
  fn an_empty_name_counts_as_no_name() {
    assert_eq!(sample(0, Some(""), Ok("vm-a")).label(), "vm-a");
    assert_eq!(
      sample(
        0,
        Some(""),
        Err(NodeError::Transport("refused".to_string()))
      )
      .label(),
      "node-0"
    );
  }

  #[test]
  fn an_empty_hostname_counts_as_no_hostname() {
    // A configured name still wins, because it never came from the API.
    assert_eq!(sample(1, Some("named"), Ok("")).label(), "named");
    assert_eq!(sample(0, None, Ok("")).label(), "node-0");
  }
}
