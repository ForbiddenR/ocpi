//! Outbound HTTP communication with the configured CPO.
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::{
    StatusCode, Url,
    header::{self, HeaderValue},
};
use uuid::Uuid;

use crate::config::Config;

const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// Values already checked by the hub service before forwarding.
pub(crate) struct ForwardRequest<'a> {
    pub location_id: &'a str,
    pub route_headers: [(&'static str, &'a str); 4],
    pub correlation_id: &'a str,
}

pub(crate) struct CpoResponse {
    pub status: StatusCode,
    pub content_type: Option<HeaderValue>,
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub(crate) enum ClientError {
    Redirect,
    Timeout,
    RequestFailed,
    ResponseTooLarge,
    ResponseFailed,
}

#[derive(Clone)]
pub(crate) struct CpoClient {
    http: reqwest::Client,
    locations_url: Url,
    token: String,
}

impl CpoClient {
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(5))
            .build()?;
        Ok(Self {
            http,
            locations_url: config.locations_url.clone(),
            token: config.outbound_token.clone(),
        })
    }

    pub async fn get_location(
        &self,
        request: ForwardRequest<'_>,
    ) -> Result<CpoResponse, ClientError> {
        let url = self
            .locations_url
            .join(request.location_id)
            .expect("validated location identifier");
        let mut outbound = self
            .http
            .get(url)
            .header(
                header::AUTHORIZATION,
                format!("Token {}", STANDARD.encode(self.token.as_bytes())),
            )
            .header("x-request-id", Uuid::new_v4().to_string())
            .header("x-correlation-id", request.correlation_id);
        for (name, value) in request.route_headers {
            outbound = outbound.header(name, value);
        }
        let mut upstream = match outbound.send().await {
            Ok(response) if !response.status().is_redirection() => response,
            Ok(_) => return Err(ClientError::Redirect),
            Err(err) if err.is_timeout() => return Err(ClientError::Timeout),
            Err(_) => return Err(ClientError::RequestFailed),
        };
        let status = upstream.status();
        let content_type = upstream.headers().get(header::CONTENT_TYPE).cloned();
        let mut body = Vec::new();
        loop {
            match upstream.chunk().await {
                Ok(Some(chunk)) if chunk.len() <= MAX_RESPONSE_BYTES.saturating_sub(body.len()) => {
                    body.extend_from_slice(&chunk)
                }
                Ok(Some(_)) => return Err(ClientError::ResponseTooLarge),
                Ok(None) => break,
                Err(err) if err.is_timeout() => return Err(ClientError::Timeout),
                Err(_) => return Err(ClientError::ResponseFailed),
            }
        }
        Ok(CpoResponse {
            status,
            content_type,
            body,
        })
    }
}
