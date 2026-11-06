use axum::{
    Router,
    routing::{MethodFilter, on},
};

use crate::{config::Config, handler, service::HubService};

/// The local demo hub, with its service shared by all HTTP routes.
#[derive(Clone)]
pub struct Hub {
    service: HubService,
}

impl Hub {
    pub fn new(config: Config) -> anyhow::Result<Self> {
        Ok(Self {
            service: HubService::new(config)?,
        })
    }

    pub fn router(self) -> Router {
        Router::new()
            .route(
                "/ocpi/versions",
                on(MethodFilter::GET, handler::versions).head(handler::reject_head),
            )
            .route(
                "/ocpi/2.2",
                on(MethodFilter::GET, handler::version_details).head(handler::reject_head),
            )
            .route(
                "/ocpi/2.2/locations/{location_id}",
                on(MethodFilter::GET, handler::location).head(handler::reject_head),
            )
            .with_state(self.service)
    }
}
