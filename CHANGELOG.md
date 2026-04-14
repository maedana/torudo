# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **`p` prefix for priority**: `pa`–`pe` sets the selected todo's priority to `(A)`–`(E)`; `px` clears it. Works in all modes
- **Key/value tag extraction**: `Item::parse` now pulls `key:value` tags (e.g. `due:2026-05-30`) into a dedicated `key_values` field instead of leaving them in the description. URLs are excluded from detection. Canonical todo.txt format with priority after the completion marker (`x (A) <completion> <creation> ...`) is now parsed correctly
- **Auto creation date on `torudo inbox add`**: Items added via `torudo inbox add <text>` get today's date inserted as the creation date (after the priority if present). An explicit creation date in the input is preserved
- **`t:YYYY-MM-DD` threshold dates**: Items with a future threshold sort to the bottom of their project column and render dimmed. Matches topydo semantics — items become active on the threshold date. Invalid or missing `t:` has no effect
- **`due:YYYY-MM-DD` overdue highlighting**: Items whose due date has arrived (`today >= due`) render with a red border to draw attention. Precedence: selected (yellow) > overdue (red) > dimmed (gray) > normal. Sort order unchanged

### Changed
- **hjkl navigation**: Now cycles at edges instead of stopping — `j` at the last item wraps to the first, `l` at the rightmost column wraps to the leftmost, and vice versa for `k`/`h`

## [0.11.1] - 2026-04-12

### Fixed
- File-watcher reload no longer skipped auto-assigning UUIDs to todo.txt lines missing `id:`. External edits that added new items without an id left them unidentified, which in turn prevented Neovim from opening a detail `.md` when focusing the new todo. `reload_todos` now calls `add_missing_ids` on every reload path

## [0.11.0] - 2026-04-12

### Added
- **GTD modes**: Added Inbox, Someday, and Waiting modes alongside existing Todo and Ref modes, each backed by its own file (`inbox.txt`, `someday.txt`, `waiting.txt`)
- **Tab bar UI**: Top tab bar shows all modes with per-mode item counts using ratatui `Tabs` widget; counts refresh on reload and on external file changes via file watcher
- **`s` (send to) prefix**: Send the selected item to any mode's file — `si` (send to inbox), `ss` (someday), `sw` (waiting), `sr` (ref), `st` (todo)
- **`torudo inbox add` subcommand**: Capture items to `inbox.txt` from external tools (launchers, shell scripts, editor bindings) without the TUI running. Auto-generates a UUID id (or preserves an explicit `id:xxx`) and prints the added item as JSON in the same format as `torudo current`. The running TUI picks up the new item via file watcher

### Changed
- **Mode switching**: Use `Tab` (next) / `Shift+Tab` (previous) to cycle modes; the previous `m` prefix is gone
- **Mode order**: Reordered to GTD workflow — Inbox, Todo, Waiting, Ref, Someday
- **Selection indicator**: Replaced the DarkGray background highlight on the selected todo and active project column with a bold yellow `> ` marker, matching crmux's focus style
- **Footer**: Shortened labels (`Complete`→`Done`, `Open URL`→`URL`), dropped spaces after colons, merged `Tab/S-Tab:Mode` into a single entry placed next to `hjkl` so navigation keys sit together

### Removed
- **`r` key** (move to ref.txt) — replaced by `sr` (send to ref) under the new `s` prefix

### Fixed
- Extra closing parenthesis in project column titles (e.g., `aegis (1))` → `aegis (1)`), a regression introduced when `main.rs` was split into modules

## [0.10.0] - 2026-04-12

### Added
- **ref.txt support**: GTD reference material management with `ref.txt`
  - `r` key moves selected todo from `todo.txt` to `ref.txt`
  - `m` submenu for mode switching (`mt`: todo mode, `mr`: ref mode)
  - REF mode disables todo-only keys (`x`, `r`, `c`) and updates help/footer dynamically

### Removed
- **Project column hide/show** (`v`/`V` keys and `--hide-projects` CLI option)
- **Manual reload** (`r` key) — replaced by ref.txt move; auto-reload via file watcher is sufficient

## [0.9.0] - 2026-04-12

### Added
- **`--todotxt-dir` CLI option**: Specify todotxt directory via command line (priority: CLI > `TODOTXT_DIR` env var > `~/todotxt`)
- **URL support**: URLs in todo descriptions are stripped from display with 🔗 icon indicator, press `o` to open all URLs in browser
- **`torudo update` subcommand**: Self-update via GitHub Releases (`--check` to check only, `--force` to force re-download)
- **Project column hide/show**: `v` to hide current project column, `V` to show all hidden projects
- **Vertical scrolling**: Project columns now scroll when todos overflow the visible area
### Fixed
- CJK text rendering and wrapping issues
- Scroll offset safety for edge cases

### Changed
- Custom character-level text wrapping replaces ratatui's built-in Wrap for correct CJK handling

## [0.8.0] - 2026-04-04

### Added
- **RPC server**: TUI process listens on Unix domain socket (`/tmp/torudo-{uid}.sock`) for msgpack-rpc requests
- **`torudo current` subcommand**: Outputs currently selected todo as JSON via RPC
  - Includes all todo fields (`title`, `id`, `priority`, `projects`, `contexts`, etc.)
  - Includes detail markdown content in `md` field if the file exists
  - Combine with `jq` for flexible extraction (e.g., `torudo current | jq -r .title`)

### Changed
- **Plan modal**: Plans sorted by file mtime (newest first), list now scrolls with j/k navigation

## [0.7.0] - 2026-03-08

### Added
- **cgp (Get Plans)**: Import plans from crmux as todo items via `cgp` keybinding
  - Plans are imported as todo.txt items with project tags and linked markdown files
  - Duplicate detection prevents importing the same plan twice
  - Requires crmux >= 0.11.0; keybinding hidden when unavailable
- **clp/cli (Launch Claude)**: Launch claude in tmux window with `clp` (plan) / `cli` (implement)
  - Uses `cwd` from todo detail md frontmatter as working directory (required)

### Changed
- **Keybindings**: Unified to 3-char `c`-prefixed sequences (`csp`, `csi`, `cgp`, `clp`, `cli`)
- **crmux detection**: Refactored from boolean `is_available()` to version-aware `detect()` for granular feature gating
- **UI**: Removed header, merged status message into footer with version display (`torudo v0.x.x`)

## [0.6.0] - 2026-03-07

### Changed
- **crmux integration**: Use crmux `mode` parameter for permission mode switching instead of `/plan` prefix
  - `sp` now switches to plan mode automatically
  - `si` now switches to accept-edits mode automatically

## [0.5.0] - 2026-03-07

### Added
- **crmux integration**: Send prompts to Claude Code sessions via crmux RPC
  - `sp`: Send plan prompt (with `/plan` prefix) to an idle Claude Code session matching the project
  - `si`: Send implement prompt to an idle Claude Code session matching the project
  - Prompts include the todo description and contents of `todos/{id}.md` if available
  - Requires crmux >= 0.10.0 running; keybindings hidden when unavailable
- **Status bar**: Shows send results and pending key state (`s-`)

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

[Unreleased]: https://github.com/maedana/torudo/compare/v0.11.1...HEAD
[0.11.1]: https://github.com/maedana/torudo/compare/v0.11.0...v0.11.1
[0.11.0]: https://github.com/maedana/torudo/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/maedana/torudo/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/maedana/torudo/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/maedana/torudo/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/maedana/torudo/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/maedana/torudo/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/maedana/torudo/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/maedana/torudo/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/maedana/torudo/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/maedana/torudo/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/maedana/torudo/releases/tag/v0.1.0
