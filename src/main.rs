use anyhow::Context;
use ocpi::{config::Config, router::Hub};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config_path = std::env::args_os()
        .nth(1)
        .context("usage: ocpi <config.json>")?;
    let config = Config::load(config_path)?;
    let bind = config.bind;
    let hub = Hub::new(config)?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    println!("OCPI learning hub listening on http://{bind}");
    axum::serve(listener, hub.router()).await?;
    Ok(())
}
