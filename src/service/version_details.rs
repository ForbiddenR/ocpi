use axum::http::HeaderMap;
use serde_json::{Value, json};

use super::{HubService, ServiceError};

impl HubService {
    pub fn version_details(&self, headers: &HeaderMap) -> Result<Value, ServiceError> {
        self.authenticate(headers)?;
        let url = self
            .config
            .public_url
            .join("ocpi/2.2/locations/")
            .expect("validated public URL");
        Ok(
            json!({"version":"2.2", "endpoints":[{"identifier":"locations", "role":"SENDER", "url":url.as_str()}]}),
        )
    }
}
