# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.18.0] - 2026-08-19

### Added
- **`torudo show <id>`**: print one item as JSON, detail md included, without knowing which mode holds it. It sweeps every GTD mode and then `done.txt`, and reports where it landed in a `mode` field (`done` for something already completed), so looking a completed item up before a `reopen` no longer means guessing at file paths. The md is always included — there is no flag to pass — and an unknown id exits non-zero. Unlike `focus` it only reads the files, so it needs no running TUI and it honours `--todotxt-dir`
- **`--with-md` on `<mode> list`, `done list`, and `search`**: add each item's raw `todos/{id}.md` body to the JSON as an `md` field, so reviewing a whole mode's detail takes one command instead of one read per item. It is opt-in because a mode's worth of md bodies is a lot of output when all you wanted was an id, and it requires `--json` — a raw body has no place in the one-line output. An item whose detail file is missing, or that has no `id:`, carries no `md` key at all rather than a null one. `add`, `set`, `promote`, `complete`, and `reopen` are unchanged: they have always included `md`. `done list --with-md` returns md too, since completing an item leaves its `todos/{id}.md` in place
- **`torudo <mode> set <id> --threshold` / `--due`**: set or move an item's `t:` and `due:` dates from the CLI, so pushing something out to next Monday no longer means editing the `.txt` by hand. Dates are absolute `YYYY-MM-DD` only — a relative date has to be resolved first — and `--threshold none` / `--due none` clear the tag, spelled in whatever case you like. A future `t:` still shows up in `list`: it sorts to the bottom and renders dimmed in the TUI rather than disappearing
- **`torudo <mode> set <id> --priority`**: set an item's priority to `(A)` through `(E)` (`a`-`e`, uppercase works too) without opening the TUI, replacing whatever it had before. `--priority none` clears it. Combine freely with `--time`, `--energy`, and `--rec` in the same command

### Changed
- **`list --json` prints its keys in alphabetical order**: `<mode> list --json` and `done list --json` now order each object's fields the way `add`, `set`, and `show` always have (`completed`, `completion_date`, `contexts`, ...) instead of following the struct's declaration order. The fields themselves are unchanged, so anything reading the JSON as JSON is unaffected — only output that was compared byte-for-byte would notice

### Fixed
- **`--rec NONE` was rejected**: the keyword that clears a tag is now case-insensitive for `--rec`, the way it already was for `--priority`, so `--rec NONE` and `--rec None` stop an item repeating just like `--rec none`
- **Skill: starting work opened the tab in the wrong place**: the new terminal tab is now created in the same herdr workspace as the session you asked from, instead of the default one, so you don't have to go looking for it
- **`install.sh` behind a restricted network**: the installer no longer gives up when `api.github.com` is unreachable — it falls back to the repository's tags to find the latest release. Set `TORUDO_VERSION` (e.g. `TORUDO_VERSION=v0.17.0`) to install a specific release instead

## [0.17.0] - 2026-08-15

### Added
- **`torudo search <query>`**: find an item across every mode by its title, its `+project` / `@context` tags, or its detail-md text, without opening the TUI. Case-insensitive substring match; `--mode` narrows it to one mode, `--json` gives structured output for scripting. `done.txt` is never searched
- **`torudo focus <id>`**: move the running TUI's cursor onto a specific item, switching modes and clearing the Now filter automatically if it's hidden. Requires the TUI to already be running
- **Recurring items (`rec:` tag)**: completing an item tagged `rec:1w` (or `rec:2m`, `rec:1y`, ...) automatically creates its next occurrence, due one period after completion. Prefix with `+` (`rec:+1m`) to count from the item's original due date instead, so a fixed schedule survives a late completion — and if it has fallen months behind, the next occurrence still lands in the future rather than in the past, so one completion is enough to catch up. The new occurrence keeps the detail md, with its checklist unchecked again. A `↻` chip on the card shows the pattern; `torudo <mode> set <id> --rec <pattern>` adds or changes it on an existing item, and `--rec none` stops it repeating

## [0.16.0] - 2026-08-15

### Added
- **CLI for every mode**: `torudo <mode> list` shows what a mode holds (`--json` for scripting), `torudo <mode> set <id> --time/--energy` sets the GTD tags, `torudo <mode> promote <id> --to <mode>` moves an item between modes, and `torudo <mode> complete <id>` finishes one into `done.txt` (Todo / Waiting only, as in the TUI). A completion can be undone with `torudo <mode> reopen <id>`, and `torudo done list` shows the archive. `add` now works on any mode, not just the inbox. All of it runs without the TUI open
- **Claude Code skill**: `skills/torudo/SKILL.md` lets Claude drive the above from plain language ("inboxに〜を追加して", "短時間にしてtodoに移して"). Given a URL it looks up a readable title and confirms the line before adding, and it can write the `- [ ]` checklist in an item's detail md. It can also start work on a task — creating the git worktree, opening a terminal tab there, and launching a Claude session for it

## [0.15.2] - 2026-08-04

### Fixed
- **Colors on Todo cards**: The priority prefix `(A)`/`(B)`/`(C)` (red/yellow/blue), the `✓` completion mark, the `🔗` link mark, and `@context` tags now actually render in color. They were styled but the color was dropped when card text was wrapped

## [0.15.1] - 2026-04-25

### Fixed
- **nvim cursor jump on md save**: Saving a long detail md no longer resets the Neovim cursor/scroll position. Regression introduced in 0.15.0

## [0.15.0] - 2026-04-22

### Added
- **`t` prefix for GTD tags**: `ti{s,m,l}` sets `time:short/medium/long`, `te{l,h}` sets `energy:low/high`. Existing values are replaced. Namespace reserved for future tag ops (e.g. `tt` threshold, `td` due)
- **GTD chips on Todo cards**: Colored `S`/`M`/`L` (time) and `↓`/`↑` (energy) chips at the card's bottom-right, left of the `{done}/{total} {elapsed}` badge
- **`n` Now-filter (Todo mode)**: `n` → energy (`l`/`h`/`a`) → time (`s`/`m`/`l`/`a`) narrows the view to matching items; `a` = any, `nc` clears. A `[Filter ↓ S]` chip appears on the tab bar while active
- **`v` Pick layout (Todo mode)**: Toggles between Project and Pick columns — `(A)` / `High` (L→M→S) / `Low` (S→M→L) / `No Energy`. Priority-A items also appear in the matching energy column. Threshold-pending items sink to each column's bottom. Composes with `n` filter. A `[Pick]` chip appears on the tab bar while active

## [0.14.0] - 2026-04-17

### Added
- **Vimium-like `f` hint mode**: Press `f` to overlay short yellow labels on every visible todo card; type the label to jump selection to that card. Labels are adaptive — `a`–`z` when ≤26 cards are visible, `aa`–`zz` when more, `aaa`–`zzz` beyond that, with no prefix collisions. Works in all modes. `Esc` or any non-matching key cancels

### Changed
- **`x` completion and md preview in Waiting mode**: The `x` completion key and detail md preview (top unchecked items + progress/elapsed badge) now also work in Waiting mode, matching Todo-mode behavior

## [0.13.0] - 2026-04-16

### Added
- **Detail md preview on Todo cards**: Each Todo-tab card now shows the top 3 unchecked `- [ ]` items from its detail md (`{todotxt_dir}/todos/{id}.md`) inline at the bottom of the card, dimmed
- **Detail md badge (done/total elapsed)**: Each Todo-tab card now shows a right-aligned bottom-border badge with checkbox progress and time since the detail md was last modified, e.g. `2/7  5m`. The elapsed part uses a 3-character format (`Ns` / `Nm` / `Nh`, capped at `99h`) and updates every redraw. When the md has no checkboxes the progress part is omitted; when the md is missing the badge is hidden
- **File-watching for `todos/*.md`**: The watcher now also watches the `todos/` subdirectory. Edits to detail md files refresh previews and badges in place (debounced 200ms) without reloading `todo.txt`

## [0.12.0] - 2026-04-14

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

[Unreleased]: https://github.com/maedana/torudo/compare/v0.18.0...HEAD
[0.18.0]: https://github.com/maedana/torudo/compare/v0.17.0...v0.18.0
[0.17.0]: https://github.com/maedana/torudo/compare/v0.16.0...v0.17.0
[0.16.0]: https://github.com/maedana/torudo/compare/v0.15.2...v0.16.0
[0.15.2]: https://github.com/maedana/torudo/compare/v0.15.1...v0.15.2
[0.15.1]: https://github.com/maedana/torudo/compare/v0.15.0...v0.15.1
[0.12.0]: https://github.com/maedana/torudo/compare/v0.11.1...v0.12.0
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
