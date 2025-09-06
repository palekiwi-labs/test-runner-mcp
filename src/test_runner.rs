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
    /// RSpec test file to run with optional line numbers (e.g., "spec/models/user_spec.rb:37:87")
    pub file: String,
}

#[derive(Debug)]
pub struct ParsedFilePath {
    pub file_path: String,
    pub line_numbers: Vec<u32>,
    pub original_input: String,
}

impl ParsedFilePath {
    fn parse(input: &str) -> Result<Self, String> {
        let parts: Vec<&str> = input.split(':').collect();

        if parts.is_empty() || parts[0].is_empty() {
            return Err("Empty file path".to_string());
        }

        let file_path = parts[0].to_string();

        // Validate file path format
        Self::validate_file_path(&file_path)?;

        let mut line_numbers = Vec::new();

        // Parse line numbers (skip first part which is file path)
        for part in &parts[1..] {
            match part.parse::<u32>() {
                Ok(line_num) if line_num > 0 => line_numbers.push(line_num),
                _ => return Err(format!("Invalid line number: {}", part)),
            }
        }

        Ok(ParsedFilePath {
            file_path,
            line_numbers,
            original_input: input.to_string(),
        })
    }

    fn validate_file_path(path: &str) -> Result<(), String> {
        // Block dangerous characters first
        if path.contains('\0') || path.contains('\n') {
            return Err("Invalid characters in file path".to_string());
        }

        // Prevent path traversal
        if path.contains("../") {
            return Err("Path traversal not allowed".to_string());
        }

        // Remove optional "./" prefix for validation
        let clean_path = path.strip_prefix("./").unwrap_or(path);

        // Basic format validation - must have more than just "_spec.rb"
        if clean_path.len() <= "_spec.rb".len() || clean_path == "_spec.rb" {
            return Err("Invalid file path format".to_string());
        }

        // Must end with _spec.rb
        if !clean_path.ends_with("_spec.rb") {
            return Err("File must be an RSpec test file (*_spec.rb)".to_string());
        }

        Ok(())
    }
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
        // Parse the file path and validate format
        let parsed_file = match ParsedFilePath::parse(&args.file) {
            Ok(parsed) => parsed,
            Err(e) => {
                return Err(McpError::invalid_params(
                    format!("Invalid file path format: {}", e),
                    None,
                ));
            }
        };

        let command_parts: Vec<&str> = self.rspec_command.split_whitespace().collect();
        let mut cmd = Command::new(command_parts[0]);

        // Add the rest of the command parts as arguments
        for part in &command_parts[1..] {
            cmd.arg(part);
        }

        // Add the file argument (use original input to preserve rspec format)
        cmd.arg(&parsed_file.original_input);

        match cmd.output().await {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let status = output.status.code().unwrap_or(-1);

                let result_text = format!(
                    "Test Results for: {}\nExit Code: {}\n\nOutput:\n{}\n\nErrors:\n{}",
                    parsed_file.original_input, status, stdout, stderr
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

    #[test]
    fn test_parse_file_path_without_line_numbers() {
        let parsed = ParsedFilePath::parse("spec/models/user_spec.rb").unwrap();
        assert_eq!(parsed.file_path, "spec/models/user_spec.rb");
        assert!(parsed.line_numbers.is_empty());
        assert_eq!(parsed.original_input, "spec/models/user_spec.rb");
    }

    #[test]
    fn test_parse_file_path_with_single_line_number() {
        let parsed = ParsedFilePath::parse("spec/models/user_spec.rb:37").unwrap();
        assert_eq!(parsed.file_path, "spec/models/user_spec.rb");
        assert_eq!(parsed.line_numbers, vec![37]);
        assert_eq!(parsed.original_input, "spec/models/user_spec.rb:37");
    }

    #[test]
    fn test_parse_file_path_with_multiple_line_numbers() {
        let parsed = ParsedFilePath::parse("spec/models/user_spec.rb:37:87").unwrap();
        assert_eq!(parsed.file_path, "spec/models/user_spec.rb");
        assert_eq!(parsed.line_numbers, vec![37, 87]);
        assert_eq!(parsed.original_input, "spec/models/user_spec.rb:37:87");
    }

    #[test]
    fn test_parse_empty_file_path() {
        let result = ParsedFilePath::parse("");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Empty file path");
    }

    #[test]
    fn test_parse_invalid_line_number() {
        let result = ParsedFilePath::parse("spec/file_spec.rb:abc");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid line number: abc");
    }

    #[test]
    fn test_parse_zero_line_number() {
        let result = ParsedFilePath::parse("spec/file_spec.rb:0");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid line number: 0");
    }

    #[test]
    fn test_parse_negative_line_number() {
        let result = ParsedFilePath::parse("spec/file_spec.rb:-5");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid line number: -5");
    }

    #[test]
    fn test_validate_rspec_file_extension() {
        let result = ParsedFilePath::parse("spec/models/user.rb");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "File must be an RSpec test file (*_spec.rb)"
        );
    }

    #[test]
    fn test_validate_rspec_file_with_optional_prefix() {
        let parsed = ParsedFilePath::parse("./spec/models/user_spec.rb").unwrap();
        assert_eq!(parsed.file_path, "./spec/models/user_spec.rb");
        assert!(parsed.line_numbers.is_empty());
    }

    #[test]
    fn test_validate_path_traversal_prevention() {
        let result = ParsedFilePath::parse("../spec/user_spec.rb");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Path traversal not allowed");
    }

    #[test]
    fn test_validate_only_spec_extension() {
        let result = ParsedFilePath::parse("_spec.rb");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid file path format");
    }

    #[test]
    fn test_validate_dangerous_characters() {
        let result = ParsedFilePath::parse("spec/user_spec.rb\0");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid characters in file path");
    }

    #[test]
    fn test_validate_non_rspec_extensions() {
        let test_cases = vec![
            "spec/user_test.rb",
            "spec/user.rb",
            "spec/user_spec.py",
            "spec/user_spec.js",
        ];

        for case in test_cases {
            let result = ParsedFilePath::parse(case);
            assert!(result.is_err(), "Should reject {}", case);
            assert_eq!(
                result.unwrap_err(),
                "File must be an RSpec test file (*_spec.rb)"
            );
        }
    }
}
