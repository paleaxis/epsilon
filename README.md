# Epsilon

A fast, lightweight, offline-first native desktop whiteboard written entirely in
Rust (egui + eframe). Inspired by Excalidraw; not a wrapper and not a web app.

**Status: pre-release.** Drawing, editing, history, save/open, recovery and
PNG/SVG/Excalidraw export work natively. See `project/ROADMAP.md` for the
current task and progress.

## Requirements

Running an installed or packaged build needs:

- Linux (x86_64 is the tested platform)
- a Wayland or X11 session with OpenGL 3.3 or newer (Mesa or a vendor driver)
- an XDG Desktop Portal implementation for the native Open/Save/Export dialogs
  (installed by default on GNOME, KDE and most desktops)

Building from source additionally needs Rust 1.95 or newer. See
[Platform and runtime notes](#platform-and-runtime-notes) for details.

## Build and run

```bash
cargo build                # debug build of the epsilon binary
cargo test --workspace     # engine and app tests
cargo run                  # launch the desktop app
cargo build --release --locked  # optimized binary in target/release/epsilon
```

## Install the desktop launcher

From a source checkout:

```bash
cargo build --release --locked
./packaging/install.sh   # user install under ~/.local
```

The script installs the binary, desktop entry, scalable icon and the
`.epsilon`/`.excalidraw` MIME types, then refreshes the desktop and MIME
indexes. Add `~/.local/bin` to `PATH` so the launcher's `Exec=epsilon` resolves.
`PREFIX` selects another install root and `EPSILON_BIN` another binary. Inside
the release tarball the same script installs the binary sitting next to it.

## Release tarball

```bash
./packaging/release.sh
```

This runs `cargo build --release --locked`, then writes
`dist/epsilon-<version>-linux-<arch>.tar.gz` and a matching `.sha256` file. The
archive unpacks to a single directory:

| Path | Purpose |
| --- | --- |
| `epsilon` | release binary |
| `epsilon.desktop` | application launcher |
| `epsilon.svg` | scalable launcher icon |
| `epsilon-mime.xml` | `.epsilon` and `.excalidraw` MIME definitions |
| `install.sh` | installs this directory's binary and assets into `$PREFIX` |
| `LICENSE` | GPL-3.0 text |
| `README.md` | this file |

Install a tarball with:

```bash
tar -xzf epsilon-0.1.0-linux-x86_64.tar.gz
cd epsilon-0.1.0-linux-x86_64
./install.sh
```

`dist/` is build output and is not committed.

## Platform and runtime notes

- The binary links against `libc`, `libm` and `libgcc` only. OpenGL and the
  Wayland/X11 client libraries are loaded at runtime by `glow`/`winit`, so this
  is **not** a fully static build; the desktop session must provide an OpenGL
  driver and either a Wayland or X11 connection.
- Wayland is preferred; X11 works as a fallback. Wayland sessions need
  `XDG_RUNTIME_DIR` (normally set by the login manager).
- File dialogs use the XDG Desktop Portal. Without a running portal, Open, Save
  and Export report an error toast and the document is left untouched.
- Copy as Image prefers the Wayland data-control protocol and falls back to
  X11; the image is owned by the running process, so it may disappear from the
  clipboard after Epsilon exits.
- No system font packages are required: the UI uses egui's embedded fonts and
  PNG export reuses those font bytes.
- Settings, logs and autosave recovery live under `$XDG_DATA_HOME/epsilon`
  (default `~/.local/share/epsilon`). No network access or telemetry.

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
