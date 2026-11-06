use axum::http::StatusCode;

// Axum otherwise routes HEAD to the GET handler, potentially forwarding it.
pub(crate) async fn reject_head() -> StatusCode {
    StatusCode::METHOD_NOT_ALLOWED
}
