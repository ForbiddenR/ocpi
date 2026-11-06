mod location;
mod method;
mod version_details;
mod versions;

pub(crate) use location::location;
pub(crate) use method::reject_head;
pub(crate) use version_details::version_details;
pub(crate) use versions::versions;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde_json::{Value, json};

use crate::service::ServiceError;

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
