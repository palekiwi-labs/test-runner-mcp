use rmcp::transport::sse_server::{SseServer, SseServerConfig};
use tracing_subscriber::{
    layer::SubscriberExt,
    util::SubscriberInitExt,
    {self},
};

mod test_runner;
use crate::test_runner::TestRunner;

const BIND_ADDRESS: &str = "0.0.0.0:30301";

/// Docker test runner MCP server over HTTP with SSE
/// Usage: cargo run -p mcp-server-examples --example test_runner_sse
/// Then connect via HTTP to http://127.0.0.1:8001/sse for SSE endpoint
/// and POST to http://127.0.0.1:8001/message for sending messages
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug".to_string().into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Docker Test Runner MCP server on {}", BIND_ADDRESS);

    let config = SseServerConfig {
        bind: BIND_ADDRESS.parse()?,
        sse_path: "/sse".to_string(),
        post_path: "/message".to_string(),
        ct: tokio_util::sync::CancellationToken::new(),
        sse_keep_alive: None,
    };

    let (sse_server, router) = SseServer::new(config);

    // Do something with the router, e.g., add routes or middleware

    let listener = tokio::net::TcpListener::bind(sse_server.config.bind).await?;

    let ct = sse_server.config.ct.child_token();

    let server = axum::serve(listener, router).with_graceful_shutdown(async move {
        ct.cancelled().await;
        tracing::info!("sse server cancelled");
    });

    tokio::spawn(async move {
        if let Err(e) = server.await {
            tracing::error!(error = %e, "sse server shutdown with error");
        }
    });

    let ct = sse_server.with_service(TestRunner::new);

    tracing::info!("Test Runner MCP server is running!");
    tracing::info!("SSE endpoint: http://{}/sse", BIND_ADDRESS);
    tracing::info!("Message endpoint: http://{}/message", BIND_ADDRESS);
    tracing::info!("Press Ctrl+C to stop");

    tokio::signal::ctrl_c().await?;
    ct.cancel();
    Ok(())
}
