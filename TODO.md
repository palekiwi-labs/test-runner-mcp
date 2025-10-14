# Test Runner MCP - TODO

## PR #4 - File Path with Line Numbers Feature

### High Priority
- [ ] use JSON format and parse it so it doesn't block on `binding.pry`
- [x] Parse file path with optional line numbers from input (e.g., 'file.rb:37:87')
- [x] Add file path format validation for rspec (_spec.rb endings, optional './' prefix)
- [x] Implement validation logic with proper error handling for invalid inputs

### Medium Priority
- [x] Update TestRunnerArgs struct to support parsed file path and line numbers
- [x] Update rspec command execution to pass file path with line numbers correctly
- [x] Add comprehensive unit tests for parsing and validation functionality

### Low Priority
- [ ] Update tool description to document new file path with line numbers feature
