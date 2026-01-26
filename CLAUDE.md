# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

### Build and Development
- `cargo build --release` - Build optimized release binary
- `cargo run` - Run the application in development mode
- `cargo test` - Run all tests
- `cargo clippy` - Run linter (configured with strict lints)
- `cargo fmt` - Format code

### Application Usage
- `torudo` - Launch the TUI application
- Environment variables:
  - `TODOTXT_DIR` - Directory containing todo.txt (default: ~/todotxt)
  - `TORUDO_NVIM_SOCKET` - Neovim socket path (default: /tmp/nvim.sock)
  - `NVIM_LISTEN_ADDRESS` - Legacy fallback (deprecated)

## Architecture

### Core Components
- **main.rs** - TUI event loop, terminal setup, file watching, and UI rendering
- **todo.rs** - Todo item parsing, file I/O operations, and completion handling
- **app_state.rs** - Application state management and navigation logic

### Key Design Patterns
- **Project-based organization**: Todos are grouped by project tags (+project_name) and displayed in columns
- **File watching**: Uses notify crate to automatically reload when todo.txt changes
- **Vim integration**: Automatically opens corresponding .md files in Neovim when navigating todos
- **Todo.txt format compliance**: Supports standard todo.txt format with priorities, dates, projects, contexts, and IDs

### Data Flow
1. Load todos from todo.txt file, auto-generating UUIDs for items missing IDs
2. Group todos by project tags into HashMap<String, Vec<Item>>
3. Display projects as columns with navigation between and within columns
4. Send vim commands to open detail files when selection changes
5. Handle completion by moving items to done.txt with completion date

### File Structure
- `~/todotxt/todo.txt` - Main todo file
- `~/todotxt/done.txt` - Completed todos archive
- `~/todotxt/todos/{id}.md` - Individual todo detail files