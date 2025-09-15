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
    #[schemars(
        description = "RSpec test file path (must end with '_spec.rb')",
        example = "spec/models/user_spec.rb"
    )]
    pub file: String,

    #[schemars(
        description = "Optional line numbers to target specific tests",
        example = "[37, 87]"
    )]
    pub line_numbers: Option<Vec<i32>>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CypressArgs {
    #[schemars(
        description = "Cypress test file path (must end with '.cy.js' or '.cy.ts')",
        example = "cypress/e2e/user-login.cy.js"
    )]
    pub file: String,
}

#[derive(Debug)]
pub struct ParsedFilePath {
    pub file_path: String,
    pub line_numbers: Vec<i32>,
}

impl ParsedFilePath {
    fn from_args(file_path: &str, line_numbers: Vec<i32>) -> Result<Self, String> {
        if file_path.is_empty() {
            return Err("Empty file path".to_string());
        }

        // Validate file path format
        Self::validate_file_path(file_path)?;

        // Validate line numbers
        for line_num in &line_numbers {
            if *line_num <= 0 {
                return Err(format!(
                    "Line numbers must be positive integers, got: {}",
                    line_num
                ));
            }
        }

        Ok(ParsedFilePath {
            file_path: file_path.to_string(),
            line_numbers,
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

        // Must end with _spec.rb and have content before it
        if !clean_path.ends_with("_spec.rb") {
            return Err("File must be an RSpec test file (*_spec.rb)".to_string());
        }

        if clean_path == "_spec.rb" {
            return Err("Invalid file path format".to_string());
        }

        Ok(())
    }

    fn validate_cypress_file_path(path: &str) -> Result<(), String> {
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

        // Must end with .cy.js or .cy.ts and have content before it
        if !clean_path.ends_with(".cy.js") && !clean_path.ends_with(".cy.ts") {
            return Err("File must be a Cypress test file (*.cy.js or *.cy.ts)".to_string());
        }

        if clean_path == ".cy.js" || clean_path == ".cy.ts" {
            return Err("Invalid file path format".to_string());
        }

        Ok(())
    }

    fn from_cypress_args(file_path: &str) -> Result<Self, String> {
        if file_path.is_empty() {
            return Err("Empty file path".to_string());
        }

        // Validate file path format
        Self::validate_cypress_file_path(file_path)?;

        Ok(ParsedFilePath {
            file_path: file_path.to_string(),
            line_numbers: vec![], // Cypress doesn't use line numbers
        })
    }
}

#[derive(Clone)]
pub struct TestRunner {
    tool_router: ToolRouter<TestRunner>,
    rspec_command: String,
    cypress_command: String,
}

#[tool_router]
impl TestRunner {
    pub fn new(rspec_command: String, cypress_command: String) -> Self {
        Self {
            tool_router: Self::tool_router(),
            rspec_command,
            cypress_command,
        }
    }

    #[tool(
        description = "Run RSpec tests for a specific file with optional line number targeting. Accepts file paths ending in '_spec.rb' with optional array of line numbers"
    )]
    async fn run_rspec(
        &self,
        Parameters(args): Parameters<TestRunnerArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Parse the file path and validate format
        let line_numbers = args.line_numbers.unwrap_or_default();
        let parsed_file = match ParsedFilePath::from_args(&args.file, line_numbers) {
            Ok(parsed) => parsed,
            Err(e) => {
                return Err(McpError::invalid_params(
                    format!("Invalid parameters: {}", e),
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

        // Build the RSpec file argument from parsed components
        let rspec_arg = if parsed_file.line_numbers.is_empty() {
            parsed_file.file_path.clone()
        } else {
            format!(
                "{}:{}",
                parsed_file.file_path,
                parsed_file
                    .line_numbers
                    .iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(":")
            )
        };
        cmd.arg(&rspec_arg);

        match cmd.output().await {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let status = output.status.code().unwrap_or(-1);

                let result_text = format!(
                    "Test Results for: {}\nExit Code: {}\n\nOutput:\n{}\n\nErrors:\n{}",
                    rspec_arg, status, stdout, stderr
                );

                Ok(CallToolResult::success(vec![Content::text(result_text)]))
            }
            Err(e) => Err(McpError::internal_error(
                format!("Command failed: {}", e),
                None,
            )),
        }
    }

    #[tool(
        description = "Run Cypress tests for a specific file. Accepts file paths ending in '.cy.js' or '.cy.ts'"
    )]
    async fn run_cypress(
        &self,
        Parameters(args): Parameters<CypressArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Parse the file path and validate format
        let parsed_file = match ParsedFilePath::from_cypress_args(&args.file) {
            Ok(parsed) => parsed,
            Err(e) => {
                return Err(McpError::invalid_params(
                    format!("Invalid parameters: {}", e),
                    None,
                ));
            }
        };

        let command_parts: Vec<&str> = self.cypress_command.split_whitespace().collect();
        let mut cmd = Command::new(command_parts[0]);

        // Add the rest of the command parts as arguments
        for part in &command_parts[1..] {
            cmd.arg(part);
        }

        // Add the Cypress file argument
        cmd.arg(&parsed_file.file_path);

        match cmd.output().await {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let status = output.status.code().unwrap_or(-1);

                let result_text = format!(
                    "Test Results for: {}\nExit Code: {}\n\nOutput:\n{}\n\nErrors:\n{}",
                    parsed_file.file_path, status, stdout, stderr
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
                "Test runner server using configurable commands. Tools: run_rspec (run RSpec tests), run_cypress (run Cypress tests)."
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
        let router = TestRunner::new("bundle exec rspec".to_string(), "npx cypress run --spec".to_string()).tool_router;

        let tools = router.list_all();
        assert_eq!(tools.len(), 2);

        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert!(tool_names.contains(&"run_rspec"));
        assert!(tool_names.contains(&"run_cypress"));
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
        assert_eq!(args.line_numbers, None);
    }

    #[test]
    fn test_test_runner_args_with_line_numbers() {
        let json = r#"
        {
            "file": "spec/models/user_spec.rb",
            "line_numbers": [37, 87]
        }
        "#;

        let args: TestRunnerArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.file, "spec/models/user_spec.rb");
        assert_eq!(args.line_numbers, Some(vec![37, 87]));
    }

    #[test]
    fn test_from_args_without_line_numbers() {
        let parsed = ParsedFilePath::from_args("spec/models/user_spec.rb", vec![]).unwrap();
        assert_eq!(parsed.file_path, "spec/models/user_spec.rb");
        assert!(parsed.line_numbers.is_empty());
    }

    #[test]
    fn test_from_args_with_line_numbers() {
        let parsed = ParsedFilePath::from_args("spec/models/user_spec.rb", vec![37, 87]).unwrap();
        assert_eq!(parsed.file_path, "spec/models/user_spec.rb");
        assert_eq!(parsed.line_numbers, vec![37, 87]);
    }

    #[test]
    fn test_from_args_with_zero_line_number() {
        let result = ParsedFilePath::from_args("spec/models/user_spec.rb", vec![0]);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Line numbers must be positive integers, got: 0"
        );
    }

    #[test]
    fn test_from_args_with_negative_line_number() {
        let result = ParsedFilePath::from_args("spec/models/user_spec.rb", vec![-5]);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Line numbers must be positive integers, got: -5"
        );
    }

    #[test]
    fn test_from_args_empty_file_path() {
        let result = ParsedFilePath::from_args("", vec![]);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Empty file path");
    }

    #[test]
    fn test_validate_rspec_file_extension() {
        let result = ParsedFilePath::from_args("spec/models/user.rb", vec![]);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "File must be an RSpec test file (*_spec.rb)"
        );
    }

    #[test]
    fn test_validate_rspec_file_with_optional_prefix() {
        let parsed = ParsedFilePath::from_args("./spec/models/user_spec.rb", vec![]).unwrap();
        assert_eq!(parsed.file_path, "./spec/models/user_spec.rb");
        assert!(parsed.line_numbers.is_empty());
    }

    #[test]
    fn test_validate_path_traversal_prevention() {
        let result = ParsedFilePath::from_args("../spec/user_spec.rb", vec![]);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Path traversal not allowed");
    }

    #[test]
    fn test_validate_only_spec_extension() {
        let result = ParsedFilePath::from_args("_spec.rb", vec![]);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid file path format");
    }

    #[test]
    fn test_validate_dangerous_characters() {
        let result = ParsedFilePath::from_args("spec/user_spec.rb\0", vec![]);
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
            let result = ParsedFilePath::from_args(case, vec![]);
            assert!(result.is_err(), "Should reject {}", case);
            assert_eq!(
                result.unwrap_err(),
                "File must be an RSpec test file (*_spec.rb)"
            );
        }
    }

    #[test]
    fn test_cypress_args_deserialization() {
        let json = r#"
        {
            "file": "cypress/e2e/user-login.cy.js"
        }
        "#;

        let args: CypressArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.file, "cypress/e2e/user-login.cy.js");
    }

    #[test]
    fn test_from_cypress_args_valid_js() {
        let parsed = ParsedFilePath::from_cypress_args("cypress/e2e/user-login.cy.js").unwrap();
        assert_eq!(parsed.file_path, "cypress/e2e/user-login.cy.js");
        assert!(parsed.line_numbers.is_empty());
    }

    #[test]
    fn test_from_cypress_args_valid_ts() {
        let parsed = ParsedFilePath::from_cypress_args("cypress/e2e/user-login.cy.ts").unwrap();
        assert_eq!(parsed.file_path, "cypress/e2e/user-login.cy.ts");
        assert!(parsed.line_numbers.is_empty());
    }

    #[test]
    fn test_from_cypress_args_with_optional_prefix() {
        let parsed = ParsedFilePath::from_cypress_args("./cypress/e2e/user-login.cy.js").unwrap();
        assert_eq!(parsed.file_path, "./cypress/e2e/user-login.cy.js");
        assert!(parsed.line_numbers.is_empty());
    }

    #[test]
    fn test_from_cypress_args_empty_file_path() {
        let result = ParsedFilePath::from_cypress_args("");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Empty file path");
    }

    #[test]
    fn test_validate_cypress_file_extension() {
        let test_cases = vec![
            "cypress/e2e/user-login.js",
            "cypress/e2e/user-login.ts",
            "cypress/e2e/user-login.spec.js",
            "cypress/e2e/user-login.test.js",
            "cypress/e2e/user-login.rb",
        ];

        for case in test_cases {
            let result = ParsedFilePath::from_cypress_args(case);
            assert!(result.is_err(), "Should reject {}", case);
            assert_eq!(
                result.unwrap_err(),
                "File must be a Cypress test file (*.cy.js or *.cy.ts)"
            );
        }
    }

    #[test]
    fn test_validate_cypress_path_traversal_prevention() {
        let result = ParsedFilePath::from_cypress_args("../cypress/user-login.cy.js");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Path traversal not allowed");
    }

    #[test]
    fn test_validate_cypress_only_extension() {
        let test_cases = vec![".cy.js", ".cy.ts"];

        for case in test_cases {
            let result = ParsedFilePath::from_cypress_args(case);
            assert!(result.is_err(), "Should reject {}", case);
            assert_eq!(result.unwrap_err(), "Invalid file path format");
        }
    }

    #[test]
    fn test_validate_cypress_dangerous_characters() {
        let test_cases = vec![
            "cypress/user-login.cy.js\0",
            "cypress/user-login.cy.js\n",
        ];

        for case in test_cases {
            let result = ParsedFilePath::from_cypress_args(case);
            assert!(result.is_err(), "Should reject {}", case);
            assert_eq!(result.unwrap_err(), "Invalid characters in file path");
        }
    }
}
