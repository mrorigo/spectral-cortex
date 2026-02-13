// Rust guideline compliant 2026-02-13

mod tools;

use rmcp::{model::*, tool_handler, transport::stdio, ServerHandler, ServiceExt};

use crate::tools::SpectralCortexMcpServer;

#[tool_handler]
impl ServerHandler for SpectralCortexMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Spectral Cortex MCP Server: compact markdown tools for querying an SMG file. Prefer small top_k values to keep responses token efficient.".into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = SpectralCortexMcpServer::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
