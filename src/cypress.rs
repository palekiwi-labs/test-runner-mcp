use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CypressStats {
    pub suites: u32,
    pub tests: u32,
    pub passes: u32,
    pub pending: u32,
    pub failures: u32,
    pub start: String,
    pub end: String,
    pub duration: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CypressCodeFrame {
    pub line: u32,
    pub column: u32,
    #[serde(rename = "originalFile")]
    pub original_file: String,
    #[serde(rename = "relativeFile")]
    pub relative_file: String,
    #[serde(rename = "absoluteFile")]
    pub absolute_file: String,
    pub frame: String,
    pub language: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CypressError {
    pub message: String,
    pub name: String,
    #[serde(rename = "codeFrame")]
    pub code_frame: Option<CypressCodeFrame>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CypressTest {
    pub title: String,
    #[serde(rename = "fullTitle")]
    pub full_title: String,
    pub file: Option<String>,
    pub duration: Option<u32>,
    #[serde(rename = "currentRetry")]
    pub current_retry: u32,
    pub err: Option<CypressError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CypressResults {
    pub stats: CypressStats,
    pub tests: Vec<CypressTest>,
    pub pending: Vec<CypressTest>,
    pub failures: Vec<CypressTest>,
    pub passes: Vec<CypressTest>,
}

pub fn extract_json_from_output(output: &str) -> Result<String, String> {
    // Find the first opening brace which marks the start of JSON
    if let Some(start_pos) = output.find('{') {
        let json_portion = &output[start_pos..];
        Ok(json_portion.to_string())
    } else {
        Err("No JSON found in Cypress output".to_string())
    }
}

pub fn parse_results(json_str: &str) -> Result<CypressResults, String> {
    serde_json::from_str(json_str)
        .map_err(|e| format!("Failed to parse Cypress JSON: {}", e))
}

pub fn filter_results(results: CypressResults) -> CypressResults {
    let filter_test = |test: CypressTest| -> CypressTest {
        CypressTest {
            title: test.title,
            full_title: test.full_title,
            file: test.file,
            duration: test.duration,
            current_retry: test.current_retry,
            err: test.err.map(|err| CypressError {
                message: err.message,
                name: err.name,
                code_frame: err.code_frame,
            }),
        }
    };

    CypressResults {
        stats: results.stats,
        tests: results.tests.into_iter().map(filter_test).collect(),
        pending: results.pending.into_iter().map(filter_test).collect(),
        failures: results.failures.into_iter().map(filter_test).collect(),
        passes: results.passes.into_iter().map(filter_test).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_from_output() {
        let output = r#"Warning: The following browser launch options were provided but are not supported by electron

 - args
[3977:0915/103024.520574:ERROR:dbus/bus.cc:408] Failed to connect to the bus: Address does not contain a colon
{
  "stats": {
    "suites": 1,
    "tests": 1,
    "passes": 0,
    "pending": 0,
    "failures": 1
  }
}"#;

        let result = extract_json_from_output(output);
        assert!(result.is_ok());
        
        let json_str = result.unwrap();
        assert!(json_str.starts_with('{'));
        assert!(json_str.contains("stats"));
    }

    #[test]
    fn test_extract_json_no_json_found() {
        let output = "Some output without JSON";
        let result = extract_json_from_output(output);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "No JSON found in Cypress output");
    }

    #[test]
    fn test_parse_results() {
        let json_str = r#"{
            "stats": {
                "suites": 1,
                "tests": 1,
                "passes": 0,
                "pending": 0,
                "failures": 1,
                "start": "2025-09-15T10:30:26.416Z",
                "end": "2025-09-15T10:30:40.850Z",
                "duration": 14434
            },
            "tests": [
                {
                    "title": "Test title",
                    "fullTitle": "Full test title",
                    "file": null,
                    "duration": 1000,
                    "currentRetry": 0,
                    "err": {
                        "message": "Test error message",
                        "name": "CypressError",
                        "codeFrame": {
                            "line": 23,
                            "column": 47,
                            "originalFile": "test.cy.js",
                            "relativeFile": "test.cy.js",
                            "absoluteFile": "/path/test.cy.js",
                            "frame": "test code frame",
                            "language": "js"
                        }
                    }
                }
            ],
            "pending": [],
            "failures": [],
            "passes": []
        }"#;

        let result = parse_results(json_str);
        assert!(result.is_ok());
        
        let parsed = result.unwrap();
        assert_eq!(parsed.stats.suites, 1);
        assert_eq!(parsed.stats.tests, 1);
        assert_eq!(parsed.tests.len(), 1);
        assert_eq!(parsed.tests[0].title, "Test title");
        assert!(parsed.tests[0].err.is_some());
    }
}