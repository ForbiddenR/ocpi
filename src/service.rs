//! OCPI authentication, routing policy, and forwarding orchestration.
use std::sync::Arc;

use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    client::{ClientError, CpoClient, ForwardRequest},
    config::Config,
};

#[derive(Debug, PartialEq, Eq)]
pub enum ServiceError {
    Unauthorized,
    InvalidLocation,
    InvalidRoutingHeader,
    UnauthorizedSender,
    UnknownDestination,
    InvalidRequestId,
    InvalidCorrelationId,
    UpstreamRedirect,
    UpstreamTimeout,
    UpstreamRequestFailed,
    UpstreamTooLarge,
    UpstreamResponseFailed,
}

impl From<ClientError> for ServiceError {
    fn from(error: ClientError) -> Self {
        match error {
            ClientError::Redirect => Self::UpstreamRedirect,
            ClientError::Timeout => Self::UpstreamTimeout,
            ClientError::RequestFailed => Self::UpstreamRequestFailed,
            ClientError::ResponseTooLarge => Self::UpstreamTooLarge,
            ClientError::ResponseFailed => Self::UpstreamResponseFailed,
        }
    }
}

pub struct LocationResponse {
    pub status: StatusCode,
    pub content_type: Option<HeaderValue>,
    pub body: Vec<u8>,
    pub request_id: String,
    pub correlation_id: String,
}

#[derive(Clone)]
pub struct HubService {
    config: Arc<Config>,
    cpo: CpoClient,
}

struct PreparedLocation<'a> {
    route_headers: [(&'static str, &'a str); 4],
    request_id: String,
    correlation_id: String,
}

impl HubService {
    pub fn new(config: Config) -> anyhow::Result<Self> {
        // Config can also be constructed directly, without Config::load.
        config.validate()?;
        let cpo = CpoClient::new(&config)?;
        Ok(Self {
            config: Arc::new(config),
            cpo,
        })
    }

    fn authenticate(&self, headers: &HeaderMap) -> Result<(), ServiceError> {
        let Ok(Some(value)) = one_header(headers, "authorization") else {
            return Err(ServiceError::Unauthorized);
        };
        let Some(encoded) = value.strip_prefix("Token ") else {
            return Err(ServiceError::Unauthorized);
        };
        if STANDARD
            .decode(encoded)
            .is_ok_and(|token| token == self.config.inbound_token.as_bytes())
        {
            Ok(())
        } else {
            Err(ServiceError::Unauthorized)
        }
    }

    pub fn versions(&self, headers: &HeaderMap) -> Result<Value, ServiceError> {
        self.authenticate(headers)?;
        let url = self
            .config
            .public_url
            .join("ocpi/2.2")
            .expect("validated public URL");
        Ok(json!([{"version":"2.2", "url":url.as_str()}]))
    }

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

    fn prepare<'a>(
        &self,
        id: &str,
        uri: &Uri,
        headers: &'a HeaderMap,
    ) -> Result<PreparedLocation<'a>, ServiceError> {
        self.authenticate(headers)?;
        if uri.query().is_some()
            || uri.path().contains('%')
            || id.is_empty()
            || id.len() > 36
            || id == "."
            || id == ".."
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        {
            return Err(ServiceError::InvalidLocation);
        }

        // Check each identity before inspecting the next field, preserving error precedence.
        let mut values = [""; 4];
        for (index, (name, expected, mismatch)) in [
            (
                "ocpi-from-country-code",
                &self.config.sender_country,
                ServiceError::UnauthorizedSender,
            ),
            (
                "ocpi-from-party-id",
                &self.config.sender_party,
                ServiceError::UnauthorizedSender,
            ),
            (
                "ocpi-to-country-code",
                &self.config.destination_country,
                ServiceError::UnknownDestination,
            ),
            (
                "ocpi-to-party-id",
                &self.config.destination_party,
                ServiceError::UnknownDestination,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let value = match one_header(headers, name) {
                Ok(Some(value))
                    if value.len() == expected.len()
                        && value.bytes().all(|b| b.is_ascii_alphanumeric()) =>
                {
                    value
                }
                _ => return Err(ServiceError::InvalidRoutingHeader),
            };
            if !value.eq_ignore_ascii_case(expected) {
                return Err(mismatch);
            }
            values[index] = value;
        }
        let request_id = trace_id(headers, "x-request-id").ok_or(ServiceError::InvalidRequestId)?;
        let correlation_id =
            trace_id(headers, "x-correlation-id").ok_or(ServiceError::InvalidCorrelationId)?;
        Ok(PreparedLocation {
            route_headers: [
                ("ocpi-from-country-code", values[0]),
                ("ocpi-from-party-id", values[1]),
                ("ocpi-to-country-code", values[2]),
                ("ocpi-to-party-id", values[3]),
            ],
            request_id,
            correlation_id,
        })
    }

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

// A header must appear exactly once, be UTF-8, and have a bounded length.
fn one_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, ()> {
    let mut values = headers.get_all(name).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(());
    }
    let value = value.to_str().map_err(|_| ())?;
    if value.is_empty() || value.len() > 256 {
        return Err(());
    }
    Ok(Some(value))
}

fn trace_id(headers: &HeaderMap, name: &str) -> Option<String> {
    match one_header(headers, name) {
        Ok(Some(value)) if value.len() <= 128 && value.bytes().all(|b| (33..=126).contains(&b)) => {
            Some(value.to_owned())
        }
        Ok(None) => Some(Uuid::new_v4().to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> HubService {
        let config: Config =
            serde_json::from_str(include_str!("../examples/routing-demo.json")).unwrap();
        HubService::new(config).unwrap()
    }

    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            "Token ZGVtby1pbmJvdW5kLXRva2Vu".parse().unwrap(),
        );
        headers.insert("ocpi-from-country-code", "nl".parse().unwrap());
        headers.insert("ocpi-from-party-id", "msp".parse().unwrap());
        headers.insert("ocpi-to-country-code", "NL".parse().unwrap());
        headers.insert("ocpi-to-party-id", "CPO".parse().unwrap());
        headers
    }

    #[test]
    fn validates_request_in_order_and_generates_trace_ids() {
        let service = service();
        let mut headers = headers();
        let uri: Uri = "/ocpi/2.2/locations/LOC1".parse().unwrap();
        let invalid_uri: Uri = "/ocpi/2.2/locations/LOC1?x=1".parse().unwrap();
        let prepared = service.prepare("LOC1", &uri, &headers).ok().unwrap();
        assert_eq!(prepared.route_headers[1].1, "msp");
        assert!(Uuid::parse_str(&prepared.request_id).is_ok());
        assert!(Uuid::parse_str(&prepared.correlation_id).is_ok());
        assert!(service.versions(&headers).is_ok());
        headers.insert("authorization", "wrong".parse().unwrap());
        assert!(matches!(
            service.versions(&headers),
            Err(ServiceError::Unauthorized)
        ));
        assert!(matches!(
            service.prepare("LOC1", &invalid_uri, &headers),
            Err(ServiceError::Unauthorized)
        ));
        headers.insert(
            "authorization",
            "Token ZGVtby1pbmJvdW5kLXRva2Vu".parse().unwrap(),
        );
        assert!(matches!(
            service.prepare("LOC1", &invalid_uri, &headers),
            Err(ServiceError::InvalidLocation)
        ));
        headers.insert("ocpi-from-party-id", "BAD".parse().unwrap());
        headers.append("ocpi-to-party-id", "CPO".parse().unwrap());
        assert!(matches!(
            service.prepare("LOC1", &uri, &headers),
            Err(ServiceError::UnauthorizedSender)
        ));
        headers.insert("ocpi-from-party-id", "MSP".parse().unwrap());
        assert!(matches!(
            service.prepare("LOC1", &uri, &headers),
            Err(ServiceError::InvalidRoutingHeader)
        ));
        headers.insert("ocpi-to-party-id", "BAD".parse().unwrap());
        assert!(matches!(
            service.prepare("LOC1", &uri, &headers),
            Err(ServiceError::UnknownDestination)
        ));
        headers.insert("ocpi-to-party-id", "CPO".parse().unwrap());
        headers.insert("x-request-id", "bad id".parse().unwrap());
        headers.append("x-correlation-id", "a".parse().unwrap());
        headers.append("x-correlation-id", "b".parse().unwrap());
        assert!(matches!(
            service.prepare("LOC1", &uri, &headers),
            Err(ServiceError::InvalidRequestId)
        ));
        headers.insert("x-request-id", "request-id".parse().unwrap());
        assert!(matches!(
            service.prepare("LOC1", &uri, &headers),
            Err(ServiceError::InvalidCorrelationId)
        ));
    }

    #[test]
    fn rejects_invalid_directly_constructed_config() {
        let mut config: Config =
            serde_json::from_str(include_str!("../examples/routing-demo.json")).unwrap();
        config.bind = "0.0.0.0:8080".parse().unwrap();
        assert!(HubService::new(config).is_err());
    }
}
