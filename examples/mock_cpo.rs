//! Local demonstration only: the token and response are public examples.
use axum::{
    Json, Router,
    extract::Path,
    http::{HeaderMap, StatusCode, header},
    routing::get,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

async fn location(Path(id): Path<String>, headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    let expected = format!("Token {}", STANDARD.encode("demo-outbound-token"));
    if headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        != Some(expected.as_str())
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(Json(json!({
        "status_code": 1000,
        "status_message": "Success",
        "timestamp": "2026-01-01T00:00:00Z",
        "data": {"id": id, "country_code": "NL", "party_id": "CPO", "name": "Example charging location", "last_updated": "2026-01-01T00:00:00Z"}
    })))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:9001").await?;
    println!("Mock CPO listening on http://127.0.0.1:9001");
    axum::serve(
        listener,
        Router::new().route("/ocpi/2.2/locations/{id}", get(location)),
    )
    .await?;
    Ok(())
}
