use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars,
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use tokio::process::Command;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct TestRunnerArgs {
    /// Test file to run (e.g., "spec/models/user_spec.rb")
    pub file: String,
}

#[derive(Clone)]
pub struct TestRunner {
    tool_router: ToolRouter<TestRunner>,
    rspec_command: String,
}

#[tool_router]
impl TestRunner {
    pub fn new(rspec_command: String) -> Self {
        Self {
            tool_router: Self::tool_router(),
            rspec_command,
        }
    }

    #[tool(description = "Run tests using the configured command")]
    async fn run_rspec(
        &self,
        Parameters(args): Parameters<TestRunnerArgs>,
    ) -> Result<CallToolResult, McpError> {
        let command_parts: Vec<&str> = self.rspec_command.split_whitespace().collect();
        let mut cmd = Command::new(command_parts[0]);

        // Add the rest of the command parts as arguments
        for part in &command_parts[1..] {
            cmd.arg(part);
        }

        // Add the file argument
        cmd.arg(&args.file);

        match cmd.output().await {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let status = output.status.code().unwrap_or(-1);

                let result_text = format!(
                    "Test Results for: {}\nExit Code: {}\n\nOutput:\n{}\n\nErrors:\n{}",
                    args.file, status, stdout, stderr
                );

                Ok(CallToolResult::success(vec![Content::text(result_text)]))
            }
            Err(e) => Err(McpError::internal_error(
                format!("Command failed: {}", e),
                None,
            )),
        }
    }
}

#[tool_handler]
impl ServerHandler for TestRunner {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Test runner server using configurable command. Tool: run_rspec (run tests for a file)."
                    .to_string(),
            ),
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        if let Some(http_request_part) = context.extensions.get::<axum::http::request::Parts>() {
            let initialize_headers = &http_request_part.headers;
            let initialize_uri = &http_request_part.uri;
            tracing::info!(?initialize_headers, %initialize_uri, "initialize from http server");
        }
        Ok(self.get_info())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_run_rspec_tool() {
        let router = TestRunner::new("bundle exec rspec".to_string()).tool_router;

        let tools = router.list_all();
        assert_eq!(tools.len(), 1);

        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert!(tool_names.contains(&"run_rspec"));
    }

    #[test]
    fn test_test_runner_args_deserialization() {
        let json = r#"
        {
            "file": "spec/models/user_spec.rb"
        }
        "#;

        let args: TestRunnerArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.file, "spec/models/user_spec.rb");
    }
}
