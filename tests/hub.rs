use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::get,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ocpi::{config::Config, hub::Hub};
use reqwest::Client;

#[derive(Clone, Default)]
struct Observed(Arc<Mutex<Vec<HeaderMap>>>);

async fn mock(
    Path(id): Path<String>,
    State(observed): State<Observed>,
    headers: HeaderMap,
) -> Response {
    observed.0.lock().unwrap().push(headers);
    match id.as_str() {
        "redirect" => Response::builder().status(302).header("location", "http://example.com/").body(Body::empty()).unwrap(),
        "large" => Response::builder().status(200).body(Body::from(vec![b'x'; 1024 * 1024 + 1])).unwrap(),
        "slow" => { tokio::time::sleep(Duration::from_secs(6)).await; Response::new(Body::from("slow")) },
        "upstream-error" => Response::builder().status(503).header("content-type", "text/plain").body(Body::from("upstream unavailable")).unwrap(),
        _ => Response::builder().status(200).header("content-type", "application/json").body(Body::from(r#"{"status_code":1000,"data":{"id":"loc1","last_updated":"2026-01-01T00:00:00Z"}}"#)).unwrap(),
    }
}

async fn start() -> (
    String,
    Client,
    Observed,
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
) {
    let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    let observed = Observed::default();
    let upstream_router = Router::new()
        .route("/locations/{id}", get(mock))
        .with_state(observed.clone());
    let upstream_task = tokio::spawn(async move {
        axum::serve(upstream, upstream_router).await.unwrap();
    });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let config = Config {
        bind: addr,
        public_url: format!("http://{addr}/").parse().unwrap(),
        sender_country: "NL".into(),
        sender_party: "MSP".into(),
        inbound_token: "inbound".into(),
        destination_country: "NL".into(),
        destination_party: "CPO".into(),
        outbound_token: "outbound".into(),
        locations_url: format!("http://{upstream_addr}/locations/")
            .parse()
            .unwrap(),
    };
    let hub = Hub::new(config).unwrap();
    let hub_task = tokio::spawn(async move {
        axum::serve(listener, hub.router()).await.unwrap();
    });
    (
        format!("http://{addr}"),
        Client::builder().no_proxy().build().unwrap(),
        observed,
        hub_task,
        upstream_task,
    )
}

fn routed(client: &Client, url: &str) -> reqwest::RequestBuilder {
    routed_for(client, url, "MSP", "CPO")
}

fn routed_for(
    client: &Client,
    url: &str,
    sender: &str,
    destination: &str,
) -> reqwest::RequestBuilder {
    client
        .get(url)
        .header(
            "authorization",
            format!("Token {}", STANDARD.encode("inbound")),
        )
        .header("ocpi-from-country-code", "NL")
        .header("ocpi-from-party-id", sender)
        .header("ocpi-to-country-code", "NL")
        .header("ocpi-to-party-id", destination)
}

#[tokio::test]
async fn discovery_routing_and_invalid_requests() {
    let (base, client, observed, hub_task, upstream_task) = start().await;
    let versions_url = format!("{base}/ocpi/versions");
    assert_eq!(
        client.get(&versions_url).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let versions: serde_json::Value = client
        .get(&versions_url)
        .header(
            "authorization",
            format!("Token {}", STANDARD.encode("inbound")),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(versions["status_code"], 1000);
    assert_eq!(versions["data"][0]["version"], "2.2");
    assert_eq!(versions["data"][0]["url"], format!("{base}/ocpi/2.2"));
    let details: serde_json::Value = client
        .get(format!("{base}/ocpi/2.2"))
        .header(
            "authorization",
            format!("Token {}", STANDARD.encode("inbound")),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(details["data"]["endpoints"][0]["identifier"], "locations");

    let url = format!("{base}/ocpi/2.2/locations/loc1");
    let response = routed(&client, &url)
        .header("x-request-id", "caller-1")
        .header("x-correlation-id", "correlation-1")
        .header("x-untrusted", "should-not-forward")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["x-request-id"], "caller-1");
    assert_eq!(response.headers()["x-correlation-id"], "correlation-1");
    let body = response.text().await.unwrap();
    assert_eq!(
        body,
        r#"{"status_code":1000,"data":{"id":"loc1","last_updated":"2026-01-01T00:00:00Z"}}"#
    );
    {
        let requests = observed.0.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let headers = &requests[0];
        assert_eq!(
            headers["authorization"],
            format!("Token {}", STANDARD.encode("outbound"))
        );
        assert_ne!(headers["x-request-id"], "caller-1");
        assert_eq!(headers["x-correlation-id"], "correlation-1");
        assert_eq!(headers["ocpi-from-party-id"], "MSP");
        assert_eq!(headers["ocpi-to-party-id"], "CPO");
        assert!(!headers.contains_key("x-untrusted"));
    }
    assert_eq!(
        client.get(&url).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        routed(&client, &url)
            .header("authorization", "Token d3Jvbmc=")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        routed(&client, &url)
            .header("ocpi-from-party-id", "BAD")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        client
            .get(&url)
            .header(
                "authorization",
                format!("Token {}", STANDARD.encode("inbound"))
            )
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        routed_for(&client, &url, "BAD", "CPO")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        routed_for(&client, &url, "MSP", "BAD")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .get(&url)
            .header("authorization", "Token d3Jvbmc=")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        routed(&client, &format!("{base}/ocpi/2.2/locations/bad%2Fid"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        routed(&client, &format!("{url}?x=1"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        client.post(&url).send().await.unwrap().status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        client.head(&url).send().await.unwrap().status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(observed.0.lock().unwrap().len(), 1);
    hub_task.abort();
    upstream_task.abort();
}

#[tokio::test]
async fn service_boundary_preserves_validation_order_and_headers() {
    let (base, client, observed, hub_task, upstream_task) = start().await;
    let url = format!("{base}/ocpi/2.2/locations/loc1");

    let response = client
        .get(format!("{url}?bad=1"))
        .header("authorization", "Token d3Jvbmc=")
        .header("ocpi-from-party-id", "MSP")
        .header("ocpi-from-party-id", "MSP")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap()["status_message"],
        "Unauthorized"
    );

    let response = routed_for(&client, &url, "BAD", "CPO")
        .header("ocpi-to-party-id", "CPO")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap()["status_message"],
        "Unknown or unauthorized party"
    );
    let response = routed(&client, &url)
        .header("x-request-id", "bad id")
        .header("x-correlation-id", "one")
        .header("x-correlation-id", "two")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap()["status_message"],
        "Invalid request ID"
    );
    let response = routed(&client, &url)
        .header("x-request-id", "valid")
        .header("x-correlation-id", "one")
        .header("x-correlation-id", "two")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap()["status_message"],
        "Invalid correlation ID"
    );
    assert_eq!(observed.0.lock().unwrap().len(), 0);

    let response = routed_for(&client, &url, "msp", "cpo")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/json");
    let request_id = response.headers()["x-request-id"]
        .to_str()
        .unwrap()
        .to_owned();
    let correlation_id = response.headers()["x-correlation-id"]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(uuid::Uuid::parse_str(&request_id).is_ok());
    assert!(uuid::Uuid::parse_str(&correlation_id).is_ok());
    let requests = observed.0.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["ocpi-from-party-id"], "msp");
    assert_eq!(requests[0]["ocpi-to-party-id"], "cpo");
    assert_eq!(requests[0]["x-correlation-id"], correlation_id);
    assert_ne!(requests[0]["x-request-id"], request_id);
    drop(requests);
    hub_task.abort();
    upstream_task.abort();
}

#[tokio::test]
async fn upstream_boundaries() {
    let (base, client, _observed, hub_task, upstream_task) = start().await;
    for (id, expected) in [
        ("redirect", StatusCode::BAD_GATEWAY),
        ("large", StatusCode::BAD_GATEWAY),
        ("slow", StatusCode::GATEWAY_TIMEOUT),
        ("upstream-error", StatusCode::SERVICE_UNAVAILABLE),
    ] {
        let url = format!("{base}/ocpi/2.2/locations/{id}");
        assert_eq!(
            routed(&client, &url).send().await.unwrap().status(),
            expected,
            "{id}"
        );
    }
    let url = format!("{base}/ocpi/2.2/locations/upstream-error");
    let response = routed(&client, &url).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.headers()["content-type"], "text/plain");
    assert_eq!(response.text().await.unwrap(), "upstream unavailable");
    upstream_task.abort();
    let _ = upstream_task.await;
    let url = format!("{base}/ocpi/2.2/locations/loc1");
    assert_eq!(
        routed(&client, &url).send().await.unwrap().status(),
        StatusCode::BAD_GATEWAY
    );
    hub_task.abort();
}
