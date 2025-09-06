use clap::Parser;
use rmcp::transport::sse_server::{SseServer, SseServerConfig};
use std::net::SocketAddr;
use tracing_subscriber::{
    layer::SubscriberExt,
    util::SubscriberInitExt,
    {self},
};

mod test_runner;
use crate::test_runner::TestRunner;

#[derive(Parser, Debug)]
#[command(name = "test-runner-mcp")]
#[command(about = "Test runner MCP server over HTTP with SSE")]
struct Cli {
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    hostname: String,
    
    #[arg(short, long, default_value = "30301")]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let bind_address: SocketAddr = format!("{}:{}", cli.hostname, cli.port).parse()?;
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug".to_string().into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Docker Test Runner MCP server on {}", bind_address);

    let config = SseServerConfig {
        bind: bind_address,
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
    tracing::info!("SSE endpoint: http://{}/sse", bind_address);
    tracing::info!("Message endpoint: http://{}/message", bind_address);
    tracing::info!("Press Ctrl+C to stop");

    tokio::signal::ctrl_c().await?;
    ct.cancel();
    Ok(())
}
