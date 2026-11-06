use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde_json::{Value, json};

use crate::service::{HubService, ServiceError};

// Axum otherwise routes HEAD to the GET handler, potentially forwarding it.
pub(crate) async fn reject_head() -> StatusCode {
    StatusCode::METHOD_NOT_ALLOWED
}

fn envelope(data: Option<Value>, code: u32, message: &str) -> Value {
    let mut value = json!({"status_code": code, "status_message": message, "timestamp": Utc::now().to_rfc3339()});
    if let Some(data) = data {
        value["data"] = data;
    }
    value
}

fn error(err: ServiceError) -> Response {
    let (status, message) = match err {
        ServiceError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized"),
        ServiceError::InvalidLocation => (
            StatusCode::BAD_REQUEST,
            "Invalid location identifier or query",
        ),
        ServiceError::InvalidRoutingHeader => (StatusCode::BAD_REQUEST, "Invalid routing header"),
        ServiceError::UnauthorizedSender => {
            (StatusCode::FORBIDDEN, "Unknown or unauthorized party")
        }
        ServiceError::UnknownDestination => {
            (StatusCode::NOT_FOUND, "Unknown or unauthorized party")
        }
        ServiceError::InvalidRequestId => (StatusCode::BAD_REQUEST, "Invalid request ID"),
        ServiceError::InvalidCorrelationId => (StatusCode::BAD_REQUEST, "Invalid correlation ID"),
        ServiceError::UpstreamRedirect => (StatusCode::BAD_GATEWAY, "Upstream redirect rejected"),
        ServiceError::UpstreamTimeout => (StatusCode::GATEWAY_TIMEOUT, "Upstream timeout"),
        ServiceError::UpstreamRequestFailed => (StatusCode::BAD_GATEWAY, "Upstream request failed"),
        ServiceError::UpstreamTooLarge => (StatusCode::BAD_GATEWAY, "Upstream response too large"),
        ServiceError::UpstreamResponseFailed => {
            (StatusCode::BAD_GATEWAY, "Upstream response failed")
        }
    };
    let code = if status.is_server_error() { 3000 } else { 2000 };
    (status, Json(envelope(None, code, message))).into_response()
}

fn discovery_response(result: Result<Value, ServiceError>) -> Response {
    match result {
        Ok(data) => Json(envelope(Some(data), 1000, "Success")).into_response(),
        Err(err) => error(err),
    }
}

pub(crate) async fn versions(State(service): State<HubService>, headers: HeaderMap) -> Response {
    discovery_response(service.versions(&headers))
}

pub(crate) async fn version_details(
    State(service): State<HubService>,
    headers: HeaderMap,
) -> Response {
    discovery_response(service.version_details(&headers))
}

pub(crate) async fn location(
    State(service): State<HubService>,
    Path(id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    match service.location(&id, &uri, &headers).await {
        Err(err) => error(err),
        Ok(result) => {
            let mut response = Response::new(Body::from(result.body));
            *response.status_mut() = result.status;
            if let Some(value) = result.content_type {
                response.headers_mut().insert(header::CONTENT_TYPE, value);
            }
            response.headers_mut().insert(
                "x-request-id",
                HeaderValue::from_str(&result.request_id).expect("validated request ID"),
            );
            response.headers_mut().insert(
                "x-correlation-id",
                HeaderValue::from_str(&result.correlation_id).expect("validated correlation ID"),
            );
            response
        }
    }
}
