use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::Full;
use hyper::header::CONTENT_TYPE;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;

use crate::configuration::Configuration;
use crate::metrics;
use crate::racknerd::{NodeClient, Transport};
use crate::scrape;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHOR: &str = env!("CARGO_PKG_AUTHORS");

/// Everything serving needs, resolved once at startup.
pub struct AppState<T: Transport> {
  pub configuration: Configuration,
  pub metrics_path: String,
  pub client: Arc<NodeClient<T>>,
}

/// One Scrape, rendered. The scrape module owns the polling; this only asks for it.
async fn metrics_body<T: Transport + 'static>(state: &AppState<T>) -> Result<String, String> {
  let nodes = state.configuration.credentials();

  let result = scrape::run(
    Arc::clone(&state.client),
    &nodes,
    state.configuration.max_concurrency,
    state.configuration.scrape_deadline(),
  )
  .await;

  metrics::render(&result).map_err(|error| error.to_string())
}

fn respond(status: StatusCode, content_type: &'static str, body: String) -> Response<Full<Bytes>> {
  Response::builder()
    .status(status)
    .header(CONTENT_TYPE, content_type)
    .body(Full::new(Bytes::from(body)))
    .expect("a response with a static status and header cannot fail to build")
}

/// The request body is never read, so this stays generic over it: the server hands
/// it the `Incoming` body of a served connection, and a test can hand it any body it
/// can build.
pub async fn handle<T, B>(request: Request<B>, state: Arc<AppState<T>>) -> Response<Full<Bytes>>
where
  T: Transport + 'static,
{
  let is_get = request.method() == Method::GET;

  if !is_get || request.uri().path() != state.metrics_path {
    let status = if is_get {
      StatusCode::NOT_FOUND
    } else {
      StatusCode::METHOD_NOT_ALLOWED
    };
    let body = format!(
      "<h3>Prometheus Racknerd Exporter version {} by {}.</h3><p>Path: <a href='{}'>{}</a></p>",
      VERSION, AUTHOR, state.metrics_path, state.metrics_path
    );

    return respond(status, "text/html", body);
  }

  match metrics_body(&state).await {
    Ok(body) => respond(StatusCode::OK, "text/plain", body),
    Err(render_error) => respond(
      StatusCode::INTERNAL_SERVER_ERROR,
      "text/plain",
      format!("# scrape could not be rendered: {}\n", render_error),
    ),
  }
}

/// Serves until the process is stopped. A port that cannot be taken is reported to
/// the caller instead of being swallowed, so the process can exit non-zero.
pub async fn run_web_server<T>(addr: SocketAddr, state: AppState<T>) -> std::io::Result<()>
where
  T: Transport + 'static,
{
  let state = Arc::new(state);
  println!("Starting exporter on http://{}{}", addr, state.metrics_path);

  let listener = tokio::net::TcpListener::bind(addr).await?;

  loop {
    let (stream, _) = listener.accept().await?;
    let state = Arc::clone(&state);

    // One task per connection: a slow client must not hold up the next scrape.
    tokio::spawn(async move {
      let service = service_fn(move |request| {
        let state = Arc::clone(&state);
        async move { Ok::<_, Infallible>(handle(request, state).await) }
      });

      if let Err(error) = http1::Builder::new()
        .serve_connection(TokioIo::new(stream), service)
        .await
      {
        eprintln!("connection error: {}", error);
      }
    });
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::configuration::Node;
  use crate::racknerd::testing::{status_body, usage_body, ScriptedTransport};
  use crate::racknerd::NodeError;
  use http_body_util::{BodyExt, Empty};
  use std::time::Duration;

  fn state<T: Transport + 'static>(client: NodeClient<T>, metrics_path: &str) -> Arc<AppState<T>> {
    let configuration = Configuration {
      endpoint: "https://ctrl.example.com".to_string(),
      node_timeout_seconds: 5,
      scrape_deadline_seconds: 8,
      max_concurrency: 8,
      nodes: vec![Node {
        api_key: "1".to_string(),
        api_secret: "a".to_string(),
        name: None,
      }],
    };

    Arc::new(AppState {
      configuration,
      metrics_path: metrics_path.to_string(),
      client: Arc::new(client),
    })
  }

  fn healthy_client() -> NodeClient<ScriptedTransport> {
    NodeClient::new(
      ScriptedTransport::new(|url| {
        let body = if url.contains("action=info") {
          usage_body("vm12345", "1.2.3.4", 1195035440)
        } else {
          status_body("online")
        };
        (Ok(body), Duration::ZERO)
      }),
      "https://ctrl.example.com",
      Duration::from_millis(500),
    )
  }

  #[tokio::test]
  async fn the_metrics_path_answers_with_the_scrape_it_just_made() {
    let state = state(healthy_client(), "/metrics");
    let request = Request::builder()
      .uri("/metrics")
      .body(Empty::<Bytes>::new())
      .unwrap();

    let response = handle(request, state).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
      response.headers()[CONTENT_TYPE].to_str().unwrap(),
      "text/plain"
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();

    assert!(
      text.contains("racknerd_bandwidth_used{hostname=\"vm12345\",node=\"vm12345\"} 1195035440"),
      "{}",
      text
    );
    assert!(
      text.contains("racknerd_node_scrape_results{call=\"usage\",node=\"vm12345\",reason=\"\"} 1"),
      "{}",
      text
    );
  }

  #[tokio::test]
  async fn an_unreachable_node_still_answers_200_with_a_failure_series() {
    let client = NodeClient::new(
      ScriptedTransport::new(|_| {
        (
          Err(NodeError::Transport("connection refused".to_string())),
          Duration::ZERO,
        )
      }),
      "https://ctrl.example.com",
      Duration::from_millis(500),
    );
    let state = state(client, "/metrics");
    let request = Request::builder()
      .uri("/metrics")
      .body(Empty::<Bytes>::new())
      .unwrap();

    let response = handle(request, state).await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();

    assert!(
      text.contains(
        "racknerd_node_scrape_results{call=\"usage\",node=\"node-0\",reason=\"transport\"} 1"
      ),
      "{}",
      text
    );
  }

  #[tokio::test]
  async fn the_help_page_keeps_its_two_statuses() {
    let state = state(healthy_client(), "/metrics");

    let not_found = handle(
      Request::builder()
        .uri("/somewhere-else")
        .body(Empty::<Bytes>::new())
        .unwrap(),
      Arc::clone(&state),
    )
    .await;
    assert_eq!(not_found.status(), StatusCode::NOT_FOUND);
    assert_eq!(
      not_found.headers()[CONTENT_TYPE].to_str().unwrap(),
      "text/html"
    );

    let wrong_method = handle(
      Request::builder()
        .method("POST")
        .uri("/metrics")
        .body(Empty::<Bytes>::new())
        .unwrap(),
      state,
    )
    .await;
    assert_eq!(wrong_method.status(), StatusCode::METHOD_NOT_ALLOWED);
  }
}
