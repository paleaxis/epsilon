# Epsilon

A fast, lightweight, offline-first native desktop whiteboard written entirely in
Rust (egui + eframe). Inspired by Excalidraw; not a wrapper and not a web app.

**Status: pre-release.** Drawing, editing, history, save/open, recovery and
PNG/SVG/Excalidraw export work natively. See `project/ROADMAP.md` for the
current task and progress.

## Requirements

- Linux (Wayland or X11) with OpenGL support
- Rust 1.95 or newer

## Build and run

```bash
cargo build            # debug build of the epsilon binary
cargo run              # launch the desktop app
cargo test --workspace # engine unit tests
cargo build --release  # optimized binary in target/release/epsilon
```

## Install the desktop launcher

```bash
cargo build --release
./packaging/install.sh   # user install under ~/.local
```

The script installs the binary, desktop entry, scalable icon and the
`.epsilon`/`.excalidraw` MIME types, then refreshes the desktop and MIME
indexes. Add `~/.local/bin` to `PATH` so the launcher's `Exec=epsilon` resolves.
`PREFIX` selects another install root and `EPSILON_BIN` another binary.

A file given on the command line opens at startup:

```bash
epsilon drawing.epsilon
epsilon scene.excalidraw
```

## Project layout

| Path | Purpose |
| --- | --- |
| `crates/epsilon-core` | Document model, geometry, canvas engine, history, I/O — no UI dependencies |
| `crates/epsilon-app` | Native eframe shell, input handling and rendering |
| `project/` | Project state, roadmap, technical decisions, changelog |
| `AGENTS.md` | Contribution and workflow rules |

## Design constraints

Native Rust only: no Electron, Tauri, WebView, browser engine, JS/Node/Python
runtime, cloud dependency, telemetry, or ads. The engine (`epsilon-core`) must
never depend on egui; documents are structured data, never screen pixels.

## License

GPL-3.0 (see `LICENSE`). Excalidraw is used as a documented reference only; no
Excalidraw source code is included. See `project/DECISIONS.md` (D-005).
