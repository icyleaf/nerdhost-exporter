use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Empty};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::Deserialize;

/// Every node API call hangs off this path.
const API_PATH: &str = "/api/client/command.php";

/// A boxed future, written out by hand so `Transport` stays object-safe: a future
/// returned by `async fn` in a trait cannot be dispatched through a trait object.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Why a call to the node API did not produce its part of a Node sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeError {
  /// The call never completed: DNS, TCP, TLS, a per-Node timeout, or the Scrape deadline.
  Transport(String),
  /// The node API answered, and said no.
  Api(String),
  /// The node API answered with something that could not be read.
  Parse(String),
}

impl NodeError {
  /// The `reason` label this failure is reported under.
  pub fn reason(&self) -> &'static str {
    match self {
      NodeError::Transport(_) => "transport",
      NodeError::Api(_) => "api",
      NodeError::Parse(_) => "parse",
    }
  }

  pub fn detail(&self) -> &str {
    match self {
      NodeError::Transport(detail) | NodeError::Api(detail) | NodeError::Parse(detail) => detail,
    }
  }
}

impl fmt::Display for NodeError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}: {}", self.reason(), self.detail())
  }
}

impl std::error::Error for NodeError {}

/// What a Node is authenticated with. An optional configured name is carried
/// through to identify the node when the API cannot supply a hostname.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
  pub api_key: String,
  pub api_secret: String,
  pub name: Option<String>,
}

/// The only thing that varies between production and tests: how a URL becomes a body.
pub trait Transport: Send + Sync {
  fn get<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<String, NodeError>>;
}

/// The production adapter: HTTPS over the native root store. hyper-rustls 0.27
/// speaks hyper 1, so this is a hyper-util client while the server beside it still
/// serves through hyper 0.14: two majors, and the lock carries both.
pub struct HttpTransport {
  client: Client<HttpsConnector<HttpConnector>, Empty<Bytes>>,
}

impl HttpTransport {
  pub fn new() -> Self {
    let https = hyper_rustls::HttpsConnectorBuilder::new()
      .with_native_roots()
      .expect("the host has no native certificate store, so no HTTPS could be trusted")
      .https_only()
      .enable_http1()
      .build();

    HttpTransport {
      client: Client::builder(TokioExecutor::new()).build(https),
    }
  }
}

impl Default for HttpTransport {
  fn default() -> Self {
    HttpTransport::new()
  }
}

impl Transport for HttpTransport {
  fn get<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<String, NodeError>> {
    Box::pin(async move {
      let uri = url
        .parse()
        .map_err(|e| NodeError::Transport(format!("bad url {}: {}", url, e)))?;

      let response = self
        .client
        .get(uri)
        .await
        .map_err(|e| NodeError::Transport(e.to_string()))?;

      let status = response.status();
      let bytes = response
        .into_body()
        .collect()
        .await
        .map_err(|e| NodeError::Transport(e.to_string()))?
        .to_bytes();

      if !status.is_success() {
        return Err(NodeError::Transport(format!("HTTP {}", status)));
      }

      String::from_utf8(bytes.to_vec())
        .map_err(|e| NodeError::Parse(format!("response body is not utf-8: {}", e)))
    })
  }
}

/// What the node API reports for one metered resource: the total, used and free
/// figures in bytes, and how full it is, in whole percent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
  pub total: i64,
  pub used: i64,
  pub free: i64,
  pub percent: i64,
}

impl Usage {
  /// The node API reports `total,used,free,percentused`, as one CSV field per
  /// resource. All four slots are read: the percentage is the API's own figure, so
  /// nothing here has to guess how it rounds.
  fn parse(raw: &str, field: &str) -> Result<Usage, NodeError> {
    let mut slots = raw.split(',');
    let mut slot = |label: &str| -> Result<i64, NodeError> {
      slots
        .next()
        .ok_or_else(|| NodeError::Parse(format!("{}: no {} slot in {:?}", field, label, raw)))?
        .trim()
        .parse::<i64>()
        .map_err(|e| NodeError::Parse(format!("{}: {} is not a number: {}", field, label, e)))
    };

    Ok(Usage {
      total: slot("total")?,
      used: slot("used")?,
      free: slot("free")?,
      percent: slot("percentused")?,
    })
  }
}

/// What the usage call contributes to a Node sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageSample {
  pub hostname: String,
  pub ipaddress: String,
  pub template: String,
  pub os: String,
  pub bandwidth: Usage,
  pub memory: Usage,
  pub disk: Usage,
}

/// What the status call contributes to a Node sample. The node API answers this
/// call with the VM status alone, so a status reply can never carry an identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusSample {
  pub vm_status: String,
}

#[derive(Deserialize)]
struct InfoEnvelope {
  status: String,
  #[serde(default)]
  statusmsg: String,
  hostname: Option<String>,
  ipaddress: Option<String>,
  hdd: Option<String>,
  bw: Option<String>,
  mem: Option<String>,
  template: Option<String>,
  os: Option<String>,
}

impl InfoEnvelope {
  fn into_sample(self) -> Result<UsageSample, NodeError> {
    reject_api_error(&self.status, &self.statusmsg)?;

    Ok(UsageSample {
      hostname: require(self.hostname, "hostname")?,
      ipaddress: require(self.ipaddress, "ipaddress")?,
      // The reply carries these whether or not they were asked for, so a node that
      // stops reporting one loses a label rather than the whole usage call.
      template: self.template.unwrap_or_default(),
      os: self.os.unwrap_or_default(),
      bandwidth: Usage::parse(&require(self.bw, "bw")?, "bw")?,
      disk: Usage::parse(&require(self.hdd, "hdd")?, "hdd")?,
      memory: Usage::parse(&require(self.mem, "mem")?, "mem")?,
    })
  }
}

#[derive(Deserialize)]
struct StatusEnvelope {
  status: String,
  #[serde(default)]
  statusmsg: String,
  vmstat: Option<String>,
}

impl StatusEnvelope {
  fn into_sample(self) -> Result<StatusSample, NodeError> {
    reject_api_error(&self.status, &self.statusmsg)?;

    Ok(StatusSample {
      vm_status: require(self.vmstat, "vmstat")?,
    })
  }
}

fn require(field: Option<String>, name: &str) -> Result<String, NodeError> {
  field.ok_or_else(|| NodeError::Parse(format!("response has no {}", name)))
}

/// The node API reports its own refusals in the body, with HTTP 200. Read them,
/// rather than letting a missing field masquerade as a parse failure.
fn reject_api_error(status: &str, statusmsg: &str) -> Result<(), NodeError> {
  if status == "success" {
    return Ok(());
  }

  let detail = if statusmsg.trim().is_empty() {
    format!("status: {}", status)
  } else {
    format!("{} (status: {})", statusmsg.trim(), status)
  };

  Err(NodeError::Api(detail))
}

/// Percent-encode one query value. Credentials are whatever the node API issued,
/// and the ones that carry `+` are why this exists: sent as they are, the API reads
/// the `+` as a space and answers `Invalid key or hash` for a key that is fine.
fn encode(value: &str) -> String {
  let mut encoded = String::with_capacity(value.len());

  for byte in value.bytes() {
    match byte {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
        encoded.push(byte as char)
      }
      _ => encoded.push_str(&format!("%{:02X}", byte)),
    }
  }

  encoded
}

/// Calls the node API for one Node's usage and status.
pub struct NodeClient<T: Transport> {
  transport: T,
  endpoint: String,
  timeout: Duration,
}

impl<T: Transport> NodeClient<T> {
  pub fn new(transport: T, endpoint: impl Into<String>, timeout: Duration) -> Self {
    NodeClient {
      transport,
      endpoint: endpoint.into(),
      timeout,
    }
  }

  /// The URL for one call. Kept separate from the call so the request layout is
  /// testable without a transport at all.
  fn url(&self, credentials: &Credentials, action: &str, extra: &str) -> String {
    format!(
      "{}{}?key={}&hash={}&action={}{}",
      self.endpoint.trim_end_matches('/'),
      API_PATH,
      encode(&credentials.api_key),
      encode(&credentials.api_secret),
      action,
      extra
    )
  }

  /// One call, bounded by the per-Node timeout.
  async fn call(&self, url: String) -> Result<String, NodeError> {
    match tokio::time::timeout(self.timeout, self.transport.get(&url)).await {
      Ok(result) => result,
      Err(_) => Err(NodeError::Transport(format!(
        "no answer within {}s",
        self.timeout.as_secs()
      ))),
    }
  }

  pub async fn usage(&self, credentials: &Credentials) -> Result<UsageSample, NodeError> {
    let url = self.url(
      credentials,
      "info",
      "&bw=true&hdd=true&mem=true&ipaddr=true",
    );
    let body = self.call(url).await?;

    parse_document(&body).map(InfoEnvelope::into_sample)?
  }

  pub async fn status(&self, credentials: &Credentials) -> Result<StatusSample, NodeError> {
    let url = self.url(credentials, "status", "");
    let body = self.call(url).await?;

    parse_document(&body).map(StatusEnvelope::into_sample)?
  }
}

/// The node API answers with a complete XML document: an `<?xml?>` declaration
/// followed by a root element. Parsed as it arrives, so the declaration stays at
/// the start of the document where XML requires it.
fn parse_document<E>(body: &str) -> Result<E, NodeError>
where
  E: serde::de::DeserializeOwned,
{
  serde_xml_rs::from_str(body.trim()).map_err(|e| NodeError::Parse(e.to_string()))
}

#[cfg(test)]
pub mod testing {
  use super::{BoxFuture, NodeError, Transport};
  use std::sync::{Arc, Mutex};
  use std::time::Duration;

  /// What a scripted call answers with, and how long it takes to answer.
  pub type Reply = (Result<String, NodeError>, Duration);

  type Script = dyn Fn(&str) -> Reply + Send + Sync;

  /// A `Transport` that answers from a script instead of the network. This is the
  /// second adapter, the one that makes the seam real.
  pub struct ScriptedTransport {
    script: Box<Script>,
    urls: Arc<Mutex<Vec<String>>>,
  }

  impl ScriptedTransport {
    pub fn new(script: impl Fn(&str) -> Reply + Send + Sync + 'static) -> Self {
      ScriptedTransport {
        script: Box::new(script),
        urls: Arc::new(Mutex::new(vec![])),
      }
    }

    /// A handle on the URLs asked for, usable once this has moved into a client.
    pub fn urls(&self) -> Arc<Mutex<Vec<String>>> {
      Arc::clone(&self.urls)
    }
  }

  impl Transport for ScriptedTransport {
    fn get<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<String, NodeError>> {
      Box::pin(async move {
        self.urls.lock().unwrap().push(url.to_string());
        let (reply, delay) = (self.script)(url);

        if !delay.is_zero() {
          tokio::time::sleep(delay).await;
        }

        reply
      })
    }
  }

  /// A usage reply in the shape the node API actually answers with today.
  /// Values are chosen so every slot is distinguishable.
  pub fn usage_body(hostname: &str, ipaddress: &str, bw_used: i64) -> String {
    format!(
      "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ctrl><status>success</status>\
       <statusmsg></statusmsg><hostname>{}</hostname><ipaddress>{}</ipaddress>\
       <template>linux-debian-10-x86_64-gen2-v1</template><os>Debian 10 64 Bit</os>\
       <vmstat></vmstat><ipaddr>{}</ipaddr><bw>50000,{},40000,10</bw>\
       <hdd>37580963840,20992491520,16588472320,56</hdd>\
       <mem>1073741824,566231040,507510784,53</mem></ctrl>",
      hostname, ipaddress, ipaddress, bw_used
    )
  }

  /// The status reply carries the VM status and nothing else, not even a hostname.
  pub fn status_body(vm_status: &str) -> String {
    format!(
      "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ctrl><status>success</status>\
       <statusmsg>{}</statusmsg><vmstat>{}</vmstat></ctrl>",
      vm_status, vm_status
    )
  }
}

#[cfg(test)]
mod tests {
  use super::testing::{status_body, usage_body, ScriptedTransport};
  use super::*;

  fn credentials() -> Credentials {
    Credentials {
      api_key: "12345678".to_string(),
      api_secret: "abcdef".to_string(),
      name: None,
    }
  }

  fn client(
    script: impl Fn(&str) -> testing::Reply + Send + Sync + 'static,
    timeout_ms: u64,
  ) -> NodeClient<ScriptedTransport> {
    NodeClient::new(
      ScriptedTransport::new(script),
      "https://nerdvm.example.com/",
      Duration::from_millis(timeout_ms),
    )
  }

  /// The bytes https://ctrl.racknerd.com answered with on 2026-10-12, with the
  /// hostname and address replaced, kept so a change in the node API's shape fails
  /// a test instead of a scrape.
  const REAL_INFO: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ctrl><status>success</status><statusmsg></statusmsg><hostname>vm12345</hostname><ipaddress>1.2.3.4</ipaddress><template>linux-debian-10-x86_64-gen2-v1</template><os>Debian 10 64 Bit</os><vmstat></vmstat><ipaddr>1.2.3.4</ipaddr><bw>2684354560000,19170066432,2665184493568,1</bw><hdd>37580963840,20996685824,16584278016,56</hdd><mem>2147483648,1680867328,466616320,78</mem></ctrl>";
  const REAL_STATUS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ctrl><status>success</status><statusmsg>online</statusmsg><vmstat>online</vmstat></ctrl>";
  const REAL_REFUSAL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ctrl><status>error</status><statusmsg>Invalid key or hash</statusmsg></ctrl>";

  #[test]
  fn the_real_usage_reply_parses() {
    let sample: UsageSample = parse_document::<InfoEnvelope>(REAL_INFO)
      .unwrap()
      .into_sample()
      .unwrap();

    assert_eq!(sample.hostname, "vm12345");
    assert_eq!(sample.ipaddress, "1.2.3.4");
    assert_eq!(sample.bandwidth.total, 2684354560000);
    assert_eq!(sample.bandwidth.used, 19170066432);
    assert_eq!(sample.bandwidth.free, 2665184493568);
    assert_eq!(sample.disk.used, 20996685824);
    assert_eq!(sample.disk.free, 16584278016);
    assert_eq!(sample.memory.total, 2147483648);
    assert_eq!(sample.memory.used, 1680867328);
    assert_eq!(sample.memory.free, 466616320);
  }

  #[test]
  fn the_real_status_reply_parses_without_an_identity() {
    let sample: StatusSample = parse_document::<StatusEnvelope>(REAL_STATUS)
      .unwrap()
      .into_sample()
      .unwrap();

    assert_eq!(sample.vm_status, "online");
  }

  #[test]
  fn the_real_refusal_is_read_as_an_api_error() {
    let error = parse_document::<StatusEnvelope>(REAL_REFUSAL)
      .unwrap()
      .into_sample()
      .unwrap_err();

    assert_eq!(error.reason(), "api");
    assert_eq!(error.detail(), "Invalid key or hash (status: error)");
  }

  #[test]
  fn usage_parses_all_four_slots() {
    let usage = Usage::parse("50000,1000,40000,10", "bw").unwrap();

    assert_eq!(
      usage,
      Usage {
        total: 50000,
        used: 1000,
        free: 40000,
        percent: 10
      }
    );
  }

  #[test]
  fn usage_rejects_a_missing_slot_and_a_non_number() {
    let missing = Usage::parse("50000,1000", "bw").unwrap_err();
    assert_eq!(missing.reason(), "parse");
    assert!(missing.detail().contains("free"), "{}", missing);

    let no_percent = Usage::parse("50000,1000,40000", "mem").unwrap_err();
    assert_eq!(no_percent.reason(), "parse");
    assert!(
      no_percent.detail().contains("percentused"),
      "{}",
      no_percent
    );

    let not_a_number = Usage::parse("50000,nope,40000,10", "hdd").unwrap_err();
    assert_eq!(not_a_number.reason(), "parse");
    assert!(not_a_number.detail().contains("used"), "{}", not_a_number);
  }

  #[test]
  fn the_request_asks_only_for_what_it_reads() {
    let client = client(
      |_| (Err(NodeError::Api("unused".into())), Duration::ZERO),
      1000,
    );

    assert_eq!(
      client.url(&credentials(), "info", "&bw=true&hdd=true&mem=true&ipaddr=true"),
      "https://nerdvm.example.com/api/client/command.php?key=12345678&hash=abcdef&action=info&bw=true&hdd=true&mem=true&ipaddr=true"
    );
    assert_eq!(
      client.url(&credentials(), "status", ""),
      "https://nerdvm.example.com/api/client/command.php?key=12345678&hash=abcdef&action=status"
    );
  }

  #[test]
  fn a_credential_carrying_url_syntax_is_percent_encoded() {
    let client = client(
      |_| (Err(NodeError::Api("unused".into())), Duration::ZERO),
      1000,
    );
    let credentials = Credentials {
      api_key: "key+with/syntax".to_string(),
      api_secret: "a+b/c=d e&f".to_string(),
      name: None,
    };

    // Sent raw, the `+` reaches the API as a space and the key is refused.
    assert_eq!(
      client.url(&credentials, "status", ""),
      "https://nerdvm.example.com/api/client/command.php?key=key%2Bwith%2Fsyntax&hash=a%2Bb%2Fc%3Dd%20e%26f&action=status"
    );
  }

  #[test]
  fn a_credential_is_encoded_byte_by_byte() {
    assert_eq!(encode("AZaz09-._~"), "AZaz09-._~");
    assert_eq!(encode("+"), "%2B");
    assert_eq!(encode("\u{4f60}"), "%E4%BD%A0");
  }

  #[tokio::test]
  async fn a_successful_usage_call_yields_the_sample() {
    let client = client(
      |url| {
        let body = if url.contains("action=info") {
          usage_body("vm12345", "1.2.3.4", 1195035440)
        } else {
          status_body("online")
        };
        (Ok(body), Duration::ZERO)
      },
      1000,
    );

    let sample = client.usage(&credentials()).await.unwrap();

    assert_eq!(sample.hostname, "vm12345");
    assert_eq!(sample.bandwidth.used, 1195035440);
    assert_eq!(sample.disk.free, 16588472320);
  }

  #[tokio::test]
  async fn an_api_refusal_is_read_from_the_body_not_inferred() {
    let client = client(|_| (Ok(REAL_REFUSAL.to_string()), Duration::ZERO), 1000);

    let error = client.usage(&credentials()).await.unwrap_err();

    assert_eq!(error.reason(), "api");
    assert_eq!(error.detail(), "Invalid key or hash (status: error)");
  }

  #[tokio::test]
  async fn a_response_missing_a_field_is_a_parse_error_naming_that_field() {
    let client = client(
      |_| {
        (
          Ok(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ctrl><status>success</status><statusmsg></statusmsg><hostname>vm1</hostname></ctrl>"
              .to_string(),
          ),
          Duration::ZERO,
        )
      },
      1000,
    );

    let error = client.usage(&credentials()).await.unwrap_err();

    assert_eq!(error.reason(), "parse");
    assert_eq!(error.detail(), "response has no ipaddress");
  }

  #[tokio::test]
  async fn the_usage_request_asks_only_for_what_it_reads() {
    let transport =
      ScriptedTransport::new(|_| (Ok(usage_body("vm12345", "1.2.3.4", 1)), Duration::ZERO));
    let urls = transport.urls();
    let client = NodeClient::new(
      transport,
      "https://nerdvm.example.com",
      Duration::from_millis(500),
    );

    client.usage(&credentials()).await.unwrap();

    let recorded = urls.lock().unwrap().clone();
    assert_eq!(recorded.len(), 1);
    assert!(
      recorded[0].contains("bw=true&hdd=true&mem=true&ipaddr=true"),
      "{}",
      recorded[0]
    );
    // The status call covers the state, so the usage call does not ask for it.
    assert!(!recorded[0].contains("status=1"), "{}", recorded[0]);
  }

  #[tokio::test]
  async fn a_slow_call_is_bounded_by_the_node_timeout() {
    let client = client(
      |_| {
        (
          Ok(usage_body("vm1", "1.2.3.4", 1)),
          Duration::from_millis(500),
        )
      },
      20,
    );

    let error = client.usage(&credentials()).await.unwrap_err();

    assert_eq!(error.reason(), "transport");
    assert!(error.detail().contains("no answer within"), "{}", error);
  }
}
