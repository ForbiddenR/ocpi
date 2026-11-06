use std::{
    net::{IpAddr, SocketAddr},
    path::Path,
};

use anyhow::{Context, bail};
use reqwest::Url;
use serde::Deserialize;

/// Configuration for the local demonstration, not production credentials.
#[derive(Clone, Deserialize)]
pub struct Config {
    pub bind: SocketAddr,
    pub public_url: Url,
    pub sender_country: String,
    pub sender_party: String,
    pub inbound_token: String,
    pub destination_country: String,
    pub destination_party: String,
    pub outbound_token: String,
    pub locations_url: Url,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path).context("reading hub configuration")?;
        let config: Self = serde_json::from_str(&content).context("parsing hub configuration")?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if !self.bind.ip().is_loopback() {
            bail!("the demo must bind to a loopback address");
        }
        validate_url(&self.public_url)?;
        validate_url(&self.locations_url)?;
        if self.public_url.path() != "/" {
            bail!("public_url must not contain a path");
        }
        if self
            .public_url
            .host_str()
            .and_then(|s| s.trim_matches(['[', ']']).parse::<IpAddr>().ok())
            != Some(self.bind.ip())
            || self.public_url.port_or_known_default() != Some(self.bind.port())
        {
            bail!("public_url must match the bind address");
        }
        if !self.locations_url.path().ends_with('/') {
            bail!("locations_url must end with /");
        }
        if self.locations_url.host_str() == self.public_url.host_str()
            && self.locations_url.port_or_known_default() == self.public_url.port_or_known_default()
        {
            bail!("the destination must not be the hub itself");
        }
        for (value, len) in [
            (&self.sender_country, 2),
            (&self.sender_party, 3),
            (&self.destination_country, 2),
            (&self.destination_party, 3),
        ] {
            if value.len() != len
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            {
                bail!(
                    "identities must be uppercase ASCII letters or digits of the expected length"
                );
            }
        }
        for token in [&self.inbound_token, &self.outbound_token] {
            if token.is_empty()
                || token.len() > 64
                || !token.bytes().all(|b| (33..=126).contains(&b))
            {
                bail!("tokens must be 1–64 printable ASCII characters without spaces");
            }
        }
        if self.inbound_token == self.outbound_token {
            bail!("inbound and outbound tokens must differ");
        }
        Ok(())
    }
}

fn validate_url(url: &Url) -> anyhow::Result<()> {
    let host = url
        .host_str()
        .and_then(|s| s.trim_matches(['[', ']']).parse::<IpAddr>().ok());
    if url.scheme() != "http"
        || !host.is_some_and(|ip| ip.is_loopback())
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("URLs must be HTTP loopback IP addresses without credentials, query or fragment");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> Config {
        serde_json::from_str(include_str!("../examples/routing-demo.json")).unwrap()
    }

    #[test]
    fn validates_demo_and_rejects_unsafe_settings() {
        let config = valid_config();
        assert!(config.validate().is_ok());

        let mut invalid = config.clone();
        invalid.bind = "0.0.0.0:8080".parse().unwrap();
        assert!(invalid.validate().is_err());
        let mut invalid = config.clone();
        invalid.locations_url = "http://example.com/locations/".parse().unwrap();
        assert!(invalid.validate().is_err());
        let mut invalid = config.clone();
        invalid.locations_url = "http://127.0.0.1:8080/locations/".parse().unwrap();
        assert!(invalid.validate().is_err());
        let mut invalid = config.clone();
        invalid.sender_party = "msp".into();
        assert!(invalid.validate().is_err());
        let mut invalid = config.clone();
        invalid.outbound_token = invalid.inbound_token.clone();
        assert!(invalid.validate().is_err());
    }
}
