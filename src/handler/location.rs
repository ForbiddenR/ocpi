use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, Uri, header},
    response::Response,
};

use crate::service::HubService;

use super::error;

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
