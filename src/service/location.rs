use axum::http::{HeaderMap, Uri};

use crate::client::ForwardRequest;

use super::{HubService, LocationResponse, ServiceError};

impl HubService {
    pub async fn location(
        &self,
        id: &str,
        uri: &Uri,
        headers: &HeaderMap,
    ) -> Result<LocationResponse, ServiceError> {
        let prepared = self.prepare(id, uri, headers)?;
        let response = self
            .cpo
            .get_location(ForwardRequest {
                location_id: id,
                route_headers: prepared.route_headers,
                correlation_id: &prepared.correlation_id,
            })
            .await?;
        Ok(LocationResponse {
            status: response.status,
            content_type: response.content_type,
            body: response.body,
            request_id: prepared.request_id,
            correlation_id: prepared.correlation_id,
        })
    }
}
