use std::collections::HashSet;
use std::fmt;
use std::path::Path;
use std::time::Duration;

use config::{Config, ConfigError, Environment, File, Map};
use serde::Deserialize;

use crate::racknerd::Credentials;

/// The host the node API lives on. The call path is appended by the node module,
/// so an endpoint carrying that path would double it.
const DEFAULT_ENDPOINT: &str = "https://ctrl.racknerd.com";

fn default_node_timeout_seconds() -> u64 {
  5
}

/// Under the metrics scraper's own default scrape timeout of 10s.
fn default_scrape_deadline_seconds() -> u64 {
  8
}

fn default_max_concurrency() -> usize {
  8
}

/// One Node the exporter watches.
#[derive(Debug, Deserialize, Clone)]
pub struct Node {
  pub api_key: String,
  pub api_secret: String,
  #[serde(default)]
  pub name: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Configuration {
  pub endpoint: String,

  #[serde(default = "default_node_timeout_seconds")]
  pub node_timeout_seconds: u64,

  #[serde(default = "default_scrape_deadline_seconds")]
  pub scrape_deadline_seconds: u64,

  #[serde(default = "default_max_concurrency")]
  pub max_concurrency: usize,

  pub nodes: Vec<Node>,
}

#[derive(Debug)]
pub enum ConfigurationError {
  Load(ConfigError),
  Invalid(String),
}

impl fmt::Display for ConfigurationError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      ConfigurationError::Load(error) => write!(f, "cannot read the configuration: {}", error),
      ConfigurationError::Invalid(reason) => write!(f, "{}", reason),
    }
  }
}

impl std::error::Error for ConfigurationError {}

impl Configuration {
  /// Read and check the configuration once, before anything is served. Anything
  /// that would make a Scrape meaningless fails here instead of at scrape time.
  pub fn load(path: &Path) -> Result<Self, ConfigurationError> {
    Self::load_with_env(path, None)
  }

  /// `env` stands in for the process environment: production passes nothing, which
  /// reads the real one, and a test passes the values it wants read, so the prefix
  /// rules below can be asserted without writing to an environment every other test
  /// shares.
  fn load_with_env(
    path: &Path,
    env: Option<Map<String, String>>,
  ) -> Result<Self, ConfigurationError> {
    let path = path.to_str().ok_or_else(|| {
      ConfigurationError::Invalid(format!("config path {:?} is not valid utf-8", path))
    })?;

    // The old exporter name still works as a fallback prefix, so existing deployments
    // do not need to re-tag their variables. Both prefixes are read; where they name
    // one key the new exporter prefix wins, because sources are merged in the order
    // they are added.
    let config = Config::builder()
      .add_source(File::with_name(path))
      .add_source(Environment::with_prefix("RACKNERD").source(env))
      .set_default("endpoint", DEFAULT_ENDPOINT)
      .map_err(ConfigurationError::Load)?
      .build()
      .map_err(ConfigurationError::Load)?;

    let configuration: Configuration =
      config.try_deserialize().map_err(ConfigurationError::Load)?;
    configuration.validate()?;

    Ok(configuration)
  }

  fn validate(&self) -> Result<(), ConfigurationError> {
    if self.nodes.is_empty() {
      return Err(ConfigurationError::Invalid(
        "no nodes configured".to_string(),
      ));
    }

    // The name is what a Node is reported under, so two Nodes sharing one would
    // report each other's outcomes under a single series.
    let mut names: HashSet<&str> = HashSet::new();
    for node in &self.nodes {
      if let Some(name) = node.name.as_deref().filter(|name| !name.is_empty()) {
        if !names.insert(name) {
          return Err(ConfigurationError::Invalid(format!(
            "two nodes are both named {:?}, which would report their outcomes as one node",
            name
          )));
        }
      }
    }

    if self.max_concurrency == 0 {
      return Err(ConfigurationError::Invalid(
        "max_concurrency must be at least 1".to_string(),
      ));
    }

    if self.node_timeout_seconds == 0 {
      return Err(ConfigurationError::Invalid(
        "node_timeout_seconds must be at least 1".to_string(),
      ));
    }

    if self.scrape_deadline_seconds == 0 {
      return Err(ConfigurationError::Invalid(
        "scrape_deadline_seconds must be at least 1".to_string(),
      ));
    }

    if self.node_timeout_seconds > self.scrape_deadline_seconds {
      return Err(ConfigurationError::Invalid(format!(
        "node_timeout_seconds ({}s) is longer than scrape_deadline_seconds ({}s), so no call could ever reach its own timeout",
        self.node_timeout_seconds, self.scrape_deadline_seconds
      )));
    }

    Ok(())
  }

  pub fn credentials(&self) -> Vec<Credentials> {
    self
      .nodes
      .iter()
      .map(|node| Credentials {
        api_key: node.api_key.clone(),
        api_secret: node.api_secret.clone(),
        name: node.name.clone(),
      })
      .collect()
  }

  pub fn node_timeout(&self) -> Duration {
    Duration::from_secs(self.node_timeout_seconds)
  }

  pub fn scrape_deadline(&self) -> Duration {
    Duration::from_secs(self.scrape_deadline_seconds)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;
  use std::path::PathBuf;
  use std::sync::atomic::{AtomicUsize, Ordering};

  static COUNTER: AtomicUsize = AtomicUsize::new(0);

  fn write_config(contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
      "racknerd-exporter-test-{}-{}.yml",
      std::process::id(),
      COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    fs::write(&path, contents).unwrap();
    path
  }

  const VALID: &str = "\
endpoint: https://ctrl.racknerd.com
nodes:
  - api_key: 12345678
    api_secret: abcdef
  - api_key: 87654321
    api_secret: fedcba
";

  #[test]
  fn a_valid_configuration_loads_and_defaults_its_bounds() {
    let path = write_config(VALID);

    let configuration = Configuration::load(&path).unwrap();

    assert_eq!(configuration.endpoint, "https://ctrl.racknerd.com");
    assert_eq!(configuration.node_timeout(), Duration::from_secs(5));
    assert_eq!(configuration.scrape_deadline(), Duration::from_secs(8));
    assert_eq!(configuration.max_concurrency, 8);
    assert_eq!(configuration.credentials().len(), 2);
    assert_eq!(configuration.credentials()[0].api_key, "12345678");

    fs::remove_file(path).unwrap();
  }

  #[test]
  fn the_bounds_are_overridable() {
    let path = write_config(&format!(
      "{}node_timeout_seconds: 2\nscrape_deadline_seconds: 3\nmax_concurrency: 1\n",
      VALID
    ));

    let configuration = Configuration::load(&path).unwrap();

    assert_eq!(configuration.node_timeout(), Duration::from_secs(2));
    assert_eq!(configuration.scrape_deadline(), Duration::from_secs(3));
    assert_eq!(configuration.max_concurrency, 1);

    fs::remove_file(path).unwrap();
  }

  #[test]
  fn an_empty_node_list_is_rejected() {
    let path = write_config("nodes: []\n");

    let error = Configuration::load(&path).unwrap_err();

    assert!(
      error.to_string().contains("no nodes configured"),
      "{}",
      error
    );

    fs::remove_file(path).unwrap();
  }

  #[test]
  fn an_out_of_range_bound_is_rejected() {
    let path = write_config(&format!("{}max_concurrency: 0\n", VALID));

    let error = Configuration::load(&path).unwrap_err();

    assert!(
      error
        .to_string()
        .contains("max_concurrency must be at least 1"),
      "{}",
      error
    );

    fs::remove_file(path).unwrap();
  }

  #[test]
  fn a_node_timeout_longer_than_the_scrape_deadline_is_rejected() {
    let path = write_config(&format!(
      "{}node_timeout_seconds: 30\nscrape_deadline_seconds: 10\n",
      VALID
    ));

    let error = Configuration::load(&path).unwrap_err();

    assert!(
      error
        .to_string()
        .contains("is longer than scrape_deadline_seconds"),
      "{}",
      error
    );

    fs::remove_file(path).unwrap();
  }

  #[test]
  fn a_node_needs_no_name_to_be_configured() {
    let path = write_config("nodes:\n  - api_key: 12345678\n    api_secret: abcdef\n");

    let configuration = Configuration::load(&path).unwrap();

    assert_eq!(configuration.credentials().len(), 1);

    fs::remove_file(path).unwrap();
  }

  #[test]
  fn two_nodes_may_share_the_absence_of_a_name() {
    let path = write_config(
      "nodes:\n  - api_key: 1\n    api_secret: a\n  - api_key: 2\n    api_secret: b\n",
    );

    // Unnamed nodes are told apart by their position, so they need no name.
    let configuration = Configuration::load(&path).unwrap();

    assert_eq!(configuration.credentials().len(), 2);
    assert_eq!(configuration.credentials()[0].name, None);

    fs::remove_file(path).unwrap();
  }

  #[test]
  fn two_nodes_with_the_same_name_are_rejected() {
    let path = write_config(
      "nodes:\n  - api_key: 1\n    api_secret: a\n    name: web\n  - api_key: 2\n    api_secret: b\n    name: web\n",
    );

    let error = Configuration::load(&path).unwrap_err();

    assert!(
      error
        .to_string()
        .contains("two nodes are both named \"web\""),
      "{}",
      error
    );

    fs::remove_file(path).unwrap();
  }

  #[test]
  fn an_empty_name_is_treated_as_no_name() {
    let path = write_config(
      "nodes:\n  - api_key: 1\n    api_secret: a\n    name: \"\"\n  - api_key: 2\n    api_secret: b\n    name: \"\"\n",
    );

    // An empty name falls back to the position, so it cannot collide with itself.
    let configuration = Configuration::load(&path).unwrap();

    assert_eq!(configuration.credentials().len(), 2);

    fs::remove_file(path).unwrap();
  }

  #[test]
  fn the_default_endpoint_carries_no_path() {
    let path = write_config("nodes:\n  - api_key: 1\n    api_secret: a\n");

    let configuration = Configuration::load(&path).unwrap();

    // Pinned to the literal host, not to the constant: the point of the default is
    // which host it names, and comparing it with itself could never notice a move.
    assert_eq!(configuration.endpoint, "https://ctrl.racknerd.com");
    // The node module appends the call path, so a path here would double it.
    assert!(!configuration.endpoint.contains("command.php"));

    fs::remove_file(path).unwrap();
  }

  /// The environment a test wants read, in place of the process one.
  fn environment(pairs: &[(&str, &str)]) -> Map<String, String> {
    pairs
      .iter()
      .map(|(key, value)| (key.to_string(), value.to_string()))
      .collect()
  }

  #[test]
  fn the_racknerd_prefix_wins_over_the_old_exporter_prefix() {
    let path = write_config("nodes:\n  - api_key: 1\n    api_secret: a\n");

    let configuration = Configuration::load_with_env(
      &path,
      Some(environment(&[(
        "RACKNERD_ENDPOINT",
        "https://new.example.com",
      )])),
    )
    .unwrap();

    assert_eq!(configuration.endpoint, "https://new.example.com");

    fs::remove_file(path).unwrap();
  }

  #[test]
  fn a_deployment_that_still_sets_the_old_exporter_prefix_keeps_working() {
    let path = write_config("nodes:\n  - api_key: 1\n    api_secret: a\n");

    let configuration = Configuration::load_with_env(
      &path,
      Some(environment(&[(
        "RACKNERD_ENDPOINT",
        "https://old.example.com",
      )])),
    )
    .unwrap();

    assert_eq!(configuration.endpoint, "https://old.example.com");

    fs::remove_file(path).unwrap();
  }

  #[test]
  fn the_environment_overrides_the_file() {
    let path = write_config(
      "endpoint: https://file.example.com\nnodes:\n  - api_key: 1\n    api_secret: a\n",
    );

    let configuration = Configuration::load_with_env(
      &path,
      Some(environment(
        &[("RACKNERD_ENDPOINT", "https://env.example.com")][..],
      )),
    )
    .unwrap();

    assert_eq!(configuration.endpoint, "https://env.example.com");

    fs::remove_file(path).unwrap();
  }
}
