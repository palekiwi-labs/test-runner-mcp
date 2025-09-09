use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars,
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use tokio::process::Command;

const ALLOWED_RSPEC_FLAGS: &[&str] = &[
    "--format",
    "-f",
    "--out",
    "-o",
    "--require",
    "-r",
    "--pattern",
    "-P",
    "--tag",
    "-t",
    "--example",
    "-e",
    "--line_number",
    "-l",
    "--order",
    "--seed",
    "--backtrace",
    "-b",
    "--color",
    "--no-color",
    "--profile",
    "-p",
    "--dry-run",
    "-d",
    "--fail-fast",
    "-x",
    "--no-fail-fast",
    "--warnings",
    "--deprecation-out",
];

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RspecFileArgs {
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
pub struct RspecArgs {
    #[schemars(
        description = "Raw arguments to pass to RSpec command",
        example = r#"["--format", "json", "spec/models/", "--tag", "~slow"]"#
    )]
    pub args: Vec<String>,
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

    fn validate_output_path(path: &str) -> Result<(), String> {
        // Block dangerous characters
        if path.contains('\0') || path.contains('\n') {
            return Err("Invalid characters in output path".to_string());
        }

        // Prevent path traversal
        if path.contains("../") {
            return Err("Path traversal not allowed in output path".to_string());
        }

        // Block absolute paths
        if path.starts_with('/') {
            return Err("Absolute paths not allowed for output".to_string());
        }

        // Only allow safe directories
        let allowed_prefixes = ["tmp/", "reports/", "output/", "coverage/"];
        if !allowed_prefixes
            .iter()
            .any(|prefix| path.starts_with(prefix))
        {
            return Err(
                "Output path must be in allowed directory (tmp/, reports/, output/, coverage/)"
                    .to_string(),
            );
        }

        Ok(())
    }

    fn validate_require_path(path: &str) -> Result<(), String> {
        // Block dangerous characters
        if path.contains('\0') || path.contains('\n') {
            return Err("Invalid characters in require path".to_string());
        }

        // Prevent path traversal
        if path.contains("../") {
            return Err("Path traversal not allowed in require path".to_string());
        }

        // Block absolute paths outside project
        if path.starts_with('/') {
            return Err("Absolute paths not allowed for require".to_string());
        }

        // Must be .rb file
        if !path.ends_with(".rb") {
            return Err("Require path must be a Ruby file (.rb)".to_string());
        }

        Ok(())
    }

    fn sanitize_value(value: &str) -> Result<(), String> {
        // Check for dangerous shell metacharacters
        let dangerous_chars = [';', '&', '|', '$', '`', '(', ')', '<', '>', '"', '\''];
        for &ch in &dangerous_chars {
            if value.contains(ch) {
                return Err(format!("Argument contains dangerous character: '{}'", ch));
            }
        }

        // Limit argument length
        if value.len() > 1000 {
            return Err("Argument too long (max 1000 characters)".to_string());
        }

        Ok(())
    }

    fn validate_numeric_value(value: &str, flag_name: &str) -> Result<(), String> {
        if value.parse::<u32>().is_err() {
            return Err(format!(
                "Invalid numeric value for {}: {}",
                flag_name, value
            ));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct TestRunner {
    tool_router: ToolRouter<TestRunner>,
    rspec_command: String,
}

fn validate_rspec_args(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Ok(());
    }

    // Limit total number of arguments
    if args.len() > 50 {
        return Err("Too many arguments (max 50)".to_string());
    }

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        // Sanitize all arguments
        ParsedFilePath::sanitize_value(arg)?;

        // Check if it's a flag
        if arg.starts_with('-') {
            if !ALLOWED_RSPEC_FLAGS.contains(&arg.as_str()) {
                return Err(format!("Disallowed flag: {}", arg));
            }

            // Validate flag-specific values
            match arg.as_str() {
                "--out" | "-o" => {
                    i += 1;
                    if i >= args.len() {
                        return Err("Missing value for --out flag".to_string());
                    }
                    ParsedFilePath::sanitize_value(&args[i])?;
                    ParsedFilePath::validate_output_path(&args[i])?;
                }
                "--require" | "-r" => {
                    i += 1;
                    if i >= args.len() {
                        return Err("Missing value for --require flag".to_string());
                    }
                    ParsedFilePath::sanitize_value(&args[i])?;
                    ParsedFilePath::validate_require_path(&args[i])?;
                }
                "--seed" => {
                    i += 1;
                    if i >= args.len() {
                        return Err("Missing value for --seed flag".to_string());
                    }
                    ParsedFilePath::sanitize_value(&args[i])?;
                    ParsedFilePath::validate_numeric_value(&args[i], "--seed")?;
                }
                "--profile" | "-p" => {
                    // Check if next argument exists and is numeric (optional for --profile)
                    if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                        i += 1;
                        ParsedFilePath::sanitize_value(&args[i])?;
                        ParsedFilePath::validate_numeric_value(&args[i], "--profile")?;
                    }
                }
                "--format" | "-f" | "--tag" | "-t" | "--example" | "-e" | "--pattern" | "-P"
                | "--order" | "--deprecation-out" => {
                    i += 1;
                    if i >= args.len() {
                        return Err(format!("Missing value for {} flag", arg));
                    }
                    ParsedFilePath::sanitize_value(&args[i])?;
                }
                // Flags that don't require values
                "--backtrace" | "-b" | "--color" | "--no-color" | "--dry-run" | "-d"
                | "--fail-fast" | "-x" | "--no-fail-fast" | "--warnings" => {
                    // No additional validation needed
                }
                _ => {
                    // This shouldn't happen due to allowlist check, but be safe
                    return Err(format!("Unhandled flag: {}", arg));
                }
            }
        } else {
            // Treat as file path - apply same validation as run_rspec_file
            ParsedFilePath::validate_file_path(arg)?;
        }
        i += 1;
    }
    Ok(())
}

#[tool_router]
impl TestRunner {
    pub fn new(rspec_command: String) -> Self {
        Self {
            tool_router: Self::tool_router(),
            rspec_command,
        }
    }

    #[tool(description = "Run RSpec with raw arguments for full command-line flexibility")]
    async fn run_rspec(
        &self,
        Parameters(args): Parameters<RspecArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Validate arguments for security
        if let Err(e) = validate_rspec_args(&args.args) {
            return Err(McpError::invalid_params(
                format!("Invalid RSpec arguments: {}", e),
                None,
            ));
        }

        let command_parts: Vec<&str> = self.rspec_command.split_whitespace().collect();
        let mut cmd = Command::new(command_parts[0]);

        // Add the rest of the command parts as arguments
        for part in &command_parts[1..] {
            cmd.arg(part);
        }

        // Add user-provided arguments
        for arg in &args.args {
            cmd.arg(arg);
        }

        match cmd.output().await {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let status = output.status.code().unwrap_or(-1);

                let result_text = format!(
                    "RSpec Command: {} {}\nExit Code: {}\n\nOutput:\n{}\n\nErrors:\n{}",
                    self.rspec_command,
                    args.args.join(" "),
                    status,
                    stdout,
                    stderr
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
        description = "Run RSpec tests for a specific file with optional line number targeting. Accepts file paths ending in '_spec.rb' with optional array of line numbers"
    )]
    async fn run_rspec_file(
        &self,
        Parameters(args): Parameters<RspecFileArgs>,
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
                "Test runner server using configurable command. Tools: run_rspec (raw command access), run_rspec_file (run tests for a file)."
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
        assert_eq!(tools.len(), 2);

        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert!(tool_names.contains(&"run_rspec"));
        assert!(tool_names.contains(&"run_rspec_file"));
    }

    #[test]
    fn test_rspec_file_args_deserialization() {
        let json = r#"
        {
            "file": "spec/models/user_spec.rb"
        }
        "#;

        let args: RspecFileArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.file, "spec/models/user_spec.rb");
        assert_eq!(args.line_numbers, None);
    }

    #[test]
    fn test_rspec_file_args_with_line_numbers() {
        let json = r#"
        {
            "file": "spec/models/user_spec.rb",
            "line_numbers": [37, 87]
        }
        "#;

        let args: RspecFileArgs = serde_json::from_str(json).unwrap();
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
    fn test_validate_rspec_args_allowed_flags() {
        let valid_args = vec![
            "--format".to_string(),
            "json".to_string(),
            "--tag".to_string(),
            "slow".to_string(),
        ];
        assert!(validate_rspec_args(&valid_args).is_ok());
    }

    #[test]
    fn test_validate_rspec_args_disallowed_flag() {
        let invalid_args = vec!["--invalid-flag".to_string()];
        let result = validate_rspec_args(&invalid_args);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Disallowed flag: --invalid-flag")
        );
    }

    #[test]
    fn test_validate_rspec_args_dangerous_characters() {
        let dangerous_args = vec!["--format".to_string(), "json; rm -rf /".to_string()];
        let result = validate_rspec_args(&dangerous_args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("dangerous character"));
    }

    #[test]
    fn test_validate_output_path_safe() {
        assert!(ParsedFilePath::validate_output_path("tmp/test_results.xml").is_ok());
        assert!(ParsedFilePath::validate_output_path("reports/coverage.html").is_ok());
    }

    #[test]
    fn test_validate_output_path_unsafe() {
        assert!(ParsedFilePath::validate_output_path("../../../etc/passwd").is_err());
        assert!(ParsedFilePath::validate_output_path("/tmp/test.xml").is_err());
        assert!(ParsedFilePath::validate_output_path("home/test.xml").is_err());
    }

    #[test]
    fn test_validate_require_path() {
        assert!(ParsedFilePath::validate_require_path("spec/support/helper.rb").is_ok());
        assert!(ParsedFilePath::validate_require_path("../malicious.rb").is_err());
        assert!(ParsedFilePath::validate_require_path("helper.py").is_err());
    }

    #[test]
    fn test_sanitize_value() {
        assert!(ParsedFilePath::sanitize_value("normal_value").is_ok());
        assert!(ParsedFilePath::sanitize_value("value_with_numbers_123").is_ok());
        assert!(ParsedFilePath::sanitize_value("value; rm -rf /").is_err());
        assert!(ParsedFilePath::sanitize_value("value$(malicious)").is_err());
    }

    #[test]
    fn test_validate_numeric_value() {
        assert!(ParsedFilePath::validate_numeric_value("123", "--seed").is_ok());
        assert!(ParsedFilePath::validate_numeric_value("0", "--seed").is_ok());
        assert!(ParsedFilePath::validate_numeric_value("abc", "--seed").is_err());
        assert!(ParsedFilePath::validate_numeric_value("-123", "--seed").is_err());
    }

    #[test]
    fn test_validate_rspec_args_too_many() {
        let too_many_args: Vec<String> = (0..51).map(|i| format!("arg{}", i)).collect();
        let result = validate_rspec_args(&too_many_args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Too many arguments"));
    }

    #[test]
    fn test_validate_rspec_args_file_paths() {
        let args_with_files = vec![
            "spec/user_spec.rb".to_string(),
            "spec/models/product_spec.rb".to_string(),
        ];
        assert!(validate_rspec_args(&args_with_files).is_ok());

        let args_with_invalid_files = vec!["spec/user.rb".to_string()];
        let result = validate_rspec_args(&args_with_invalid_files);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("File must be an RSpec test file")
        );
    }
}
