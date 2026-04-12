# Torudo

[![Crates.io](https://img.shields.io/crates/v/torudo.svg)](https://crates.io/crates/torudo)

A terminal-based todo.txt viewer and manager written in Rust with TUI interface.

## Features

- Project-based column view with priority sorting
- Vim integration and real-time file watching
- URL detection (🔗) and browser open (`o`)
- Self-update via GitHub Releases (`torudo update`)
- [crmux](https://github.com/maedana/crmux) / Claude Code integration

## Demo
![gif][1]

## Installation

### Quick install

```sh
curl -sSL https://raw.githubusercontent.com/maedana/torudo/main/install.sh | sh
```

### From crates.io

```sh
cargo install torudo --locked
```

### Build from source

```sh
git clone https://github.com/maedana/torudo.git
cd torudo
cargo build --release
```

## Configuration

### Command Line Options

- `--todotxt-dir <PATH>`: Directory containing your todo.txt file (default: `~/todotxt`, fallback: `TODOTXT_DIR` env var)
- `--nvim-listen <PATH>`: Neovim socket path set by `nvim --listen` (default: `/tmp/nvim.sock`, fallback: `NVIM_LISTEN_ADDRESS` env var)

## Usage

### First Time Setup

When you run Torudo for the first time, it will check for the required directory and files:

1. If `~/todotxt` directory doesn't exist, it will ask permission to create it
2. If `todo.txt` doesn't exist, it will ask permission to create an empty file
3. If you decline either creation, the application will exit

This ensures you have control over where your todo files are stored.

### Basic Usage

```bash
# Run torudo (looks for todo.txt in $TODOTXT_DIR or ~/todotxt)
torudo

# Specify todotxt directory
torudo --todotxt-dir ~/my-todos

# Run with debug mode for detailed logging
torudo -d

# Specify Neovim socket path
torudo --nvim-listen /tmp/my-nvim.sock
```

### Updating

```bash
# Check for updates
torudo update --check

# Update to latest version
torudo update

# Force re-download
torudo update --force
```

### Keyboard Controls

Press `?` in the TUI or run `torudo -h` to see all keyboard shortcuts.

### Todo.txt Format

Torudo supports the standard todo.txt format:

```
(A) Call Mom +family @phone
x 2024-01-15 2024-01-10 (B) Review quarterly report +work @office
(C) Buy groceries +personal @errands
Learn Rust programming +learning @coding id:abc123
```

Features supported:
- Priority levels: `(A)`, `(B)`, `(C)`
- Completion status: `x` prefix with completion date
- Creation date: `YYYY-MM-DD` format
- Projects: `+project_name`
- Contexts: `@context_name`
- Unique IDs: `id:unique_identifier` (automatically added if missing)

### Todo Sorting

Todos are automatically sorted within each project column using the following priority:

1. **Priority level**: (A) items first, then (B), then (C)
2. **File line number**: Within the same priority level, todos maintain their original file order

This ensures high-priority items are always visible at the top while preserving your intended ordering for items of the same priority.

### Display Features

**Dynamic Text Wrapping**: Todo titles and descriptions automatically wrap to multiple lines based on the terminal width. This ensures that long todo items are fully visible without truncation, making it easy to read comprehensive task descriptions.

**Smart Height Calculation**: Each todo item's display height is calculated dynamically based on its content length, with a reasonable maximum to prevent excessive screen usage.

### Vim Integration

If you have Neovim running with a socket, Torudo can automatically open todo detail files when navigating. Each todo item can have an associated markdown file in `$TODOTXT_DIR/todos/{id}.md`.

### Todo Detail Frontmatter

Todo detail files (`todos/{id}.md`) support YAML frontmatter with a `cwd` field to specify the working directory for `clp`/`cli` claude launch:

```markdown
---
cwd: /home/user/src/my-project
---
# Task details here
```

The `cwd` field is required for `clp`/`cli` — an error is shown if it is not set.

## File Structure

```
~/todotxt/
├── todo.txt          # Main todo file
├── done.txt          # Completed todos
└── todos/            # Individual todo detail files
    ├── abc123.md
    └── def456.md
```

## Development

### Running in Development

```bash
cargo run

# With debug mode
cargo run -- -d
```

### Running Tests

```bash
cargo test
```

### Code Quality

```bash
cargo clippy
cargo fmt
```

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests and linting
5. Submit a pull request

## Acknowledgments

- Inspired by the todo.txt format by Gina Trapani
- Built with [Ratatui](https://github.com/ratatui-org/ratatui) for the terminal UI
- Uses [crossterm](https://github.com/crossterm-rs/crossterm) for cross-platform terminal handling

[1]: https://raw.githubusercontent.com/maedana/torudo/main/docs/demo.gif
