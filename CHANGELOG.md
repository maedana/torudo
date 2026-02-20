# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2026-02-20

### Changed
- **Neovim communication**: Switched from `nvim --server --remote-send` subprocess to direct msgpack-rpc over Unix socket for more reliable IPC

## [0.3.0] - 2025-07-26

### Fixed
- **done.txt format**: Completion format now follows todo.txt spec (priority moved after completion date)

### Changed
- **Neovim socket configuration**: Replaced `TORUDO_NVIM_SOCKET` environment variable with `--nvim-listen` CLI option
  - Priority: CLI option → `NVIM_LISTEN_ADDRESS` env var → default `/tmp/nvim.sock`
  - `TORUDO_NVIM_SOCKET` environment variable is no longer supported

### Technical
- Moved `send_vim_command` into `AppState` as a method
- Added `nvim_socket` field to `AppState` struct

## [0.2.0] - 2025-06-10

### Added
- **Priority-based sorting**: Todos are now automatically sorted by priority (A, B, C) then by file line number within each project column
- **Dynamic text wrapping**: Long todo titles automatically wrap to multiple lines based on terminal width for full visibility
- **Smart setup**: Application now prompts to create missing todotxt directory and todo.txt file on first run
- **Enhanced display**: Smart height calculation for todo items based on content length
- Debug mode with `-d` flag for detailed logging
- Line number tracking for todos to maintain original file order within same priority levels

### Changed
- Todo display now uses dynamic height calculation instead of fixed 3-line height
- Improved user experience with confirmation prompts for file/directory creation
- Enhanced README with comprehensive feature documentation and crates.io installation instructions

### Technical
- Added `line_number` field to `Item` struct
- Modified `load_todos` function to implement priority-based sorting
- Updated all test cases to support new `line_number` field
- Added comprehensive unit tests for sorting functionality

## [0.1.0] - 2025-06-06

### Added
- Initial release
- Interactive TUI interface for browsing todo.txt files
- Project-based column view for organized task management
- Vim integration for editing individual todo items
- Real-time file watching for automatic updates
- Support for todo.txt format with priorities, projects, and contexts
- Task completion with automatic archiving to done.txt
- Keyboard navigation (j/k for vertical, h/l for horizontal)
- Automatic UUID generation for todos without IDs
- Support for standard todo.txt format features:
  - Priority levels (A), (B), (C)
  - Completion status with dates
  - Creation dates
  - Projects (+project_name)
  - Contexts (@context_name)
  - Custom IDs (id:unique_identifier)

[0.4.0]: https://github.com/maedana/torudo/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/maedana/torudo/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/maedana/torudo/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/maedana/torudo/releases/tag/v0.1.0
