use axum::http::HeaderMap;
use serde_json::{Value, json};

use super::{HubService, ServiceError};

impl HubService {
    pub fn versions(&self, headers: &HeaderMap) -> Result<Value, ServiceError> {
        self.authenticate(headers)?;
        let url = self
            .config
            .public_url
            .join("ocpi/2.2")
            .expect("validated public URL");
        Ok(json!([{"version":"2.2", "url":url.as_str()}]))
    }
}
