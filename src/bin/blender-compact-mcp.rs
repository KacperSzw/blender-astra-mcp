use rmcp::{ServiceExt, transport::stdio};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    compact_mcp::server::Bridge
        .serve(stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}
