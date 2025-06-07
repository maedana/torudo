# Torudo

A terminal-based todo.txt viewer and manager written in Rust with TUI interface.

## Features

- Interactive TUI interface for browsing todo.txt files
- Project-based column view for organized task management
- Vim integration for editing individual todo items
- Real-time file watching for automatic updates
- Support for todo.txt format with priorities, projects, and contexts
- Task completion with automatic archiving to done.txt

## Installation

### Prerequisites

- Rust (latest stable version)
- Git

### Build from source

```bash
git clone https://github.com/maedana/torudo.git
cd torudo
cargo build --release
```

The binary will be available at `target/release/torudo`.

## Configuration

Torudo uses environment variables for configuration:

- `TODOTXT_DIR`: Directory containing your todo.txt file (default: `~/todotxt`)
- `NVIM_LISTEN_ADDRESS`: Neovim socket path for vim integration (default: `/tmp/nvim.sock`)

## Usage

### Basic Usage

```bash
# Run torudo (looks for todo.txt in $TODOTXT_DIR or ~/todotxt)
torudo
```

### Keyboard Controls

- `j/k`: Navigate up/down within a project column
- `h/l`: Switch between project columns
- `x`: Mark selected todo as complete and move to done.txt
- `r`: Reload todo.txt file
- `q`: Quit application

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

### Vim Integration

If you have Neovim running with a socket, Torudo can automatically open todo detail files when navigating. Each todo item can have an associated markdown file in `$TODOTXT_DIR/todos/{id}.md`.

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