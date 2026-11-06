use axum::{extract::State, http::HeaderMap, response::Response};

use crate::service::HubService;

use super::discovery_response;

pub(crate) async fn versions(State(service): State<HubService>, headers: HeaderMap) -> Response {
    discovery_response(service.versions(&headers))
}
