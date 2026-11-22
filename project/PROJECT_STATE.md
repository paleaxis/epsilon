# Project State

Last updated: 2026-11-22
Current stage: **Stage 1 — Native desktop shell**
Last completed task: **1.8 — Window/settings persistence** (done out of order at the owner's request; 1.4–1.7 remain open)
Next task: **1.4 — Menus and top toolbar**

## Status

The shell now persists the selected theme and active tool itself, and enables
eframe persistence so window geometry and resizable panel widths survive
restarts. The whiteboard itself is still not usable: the canvas is a placeholder
until Stage 2 and tools do not draw until Stage 3.

Repository layout:

```
Cargo.toml                  workspace (edition 2024, rust 1.95+)
AGENTS.md                   permanent rules
README.md                   build/run overview
project/                    STATE, ROADMAP, DECISIONS, CHANGELOG
crates/epsilon-core/        UI-independent engine (no egui dependency)
  src/geometry.rs           Point, Rect + 11 unit tests
crates/epsilon-app/         eframe desktop shell
  src/main.rs               native entry point (persist_window enabled)
  src/layout.rs             shell size budgets + compile-time assertions
  src/theme.rs              semantic palettes, Visuals setup, (de)serializable Theme
  src/tools.rs              Tool enum, shortcuts, painted icons, tool buttons
  src/app.rs                EpsilonApp, menus, tool state, persisted settings
```

Persistence design:

- eframe's `persistence` feature is enabled; settings go to eframe's store
  (`$XDG_DATA_HOME/epsilon/app.ron`, default `~/.local/share/epsilon/app.ron`).
- App settings (`epsilon.settings`): `theme` and `tool`, RON-serialized through
  `eframe::set_value` and restored in `EpsilonApp::from_storage`. Missing or
  corrupt values fall back to dark theme + Select tool (tested).
- Window geometry: eframe stores its `window` key (`persist_window: true`) and
  restores position/size.
- Panel widths: egui stores the resizable properties panel's `PanelState` in
  egui memory, which eframe persists under the `egui` key
  (`persist_egui_memory` is asserted true by a test).
- The properties panel now has a fixed 200–400px resize range. The previous
  available-width clamp was removed because transient narrow frames at startup
  could clamp and overwrite a persisted width; the compile-time layout budget
  still guarantees a 240px canvas at the 720px minimum window.

## Verified evidence (task 1.8)

A native run against `XDG_DATA_HOME=/tmp/epsilon-persist-test` produced
`/tmp/epsilon-persist-test/epsilon/app.ron` containing:

- `"epsilon.settings": "(theme:light,tool:Text)"` after seeding those values and
  restarting: the app loaded them, applied `theme_preference:Light`, and saved
  the same values on exit (proves theme/tool restore).
- `"window": "...inner_size_points:(x:...,y:...)"` (window geometry stored).
- `"egui"` memory containing the menu, toolbar, rail, properties panel and
  status-bar `outer_rect`s (panel sizes stored).

Caveats: the session runs a tiling compositor (niri), which forces window sizes;
window-size restoration and panel-width restoration were not independently
re-measured after restart, so those rely on eframe/egui behavior rather than a
reproduced measurement. No compositor window-close commands are used for
verification anymore.

## Environment

- Linux (Arch/CachyOS), niri Wayland session; Rust 1.100.0 (system toolchain,
  no rustup), cargo 1.100.0.
- egui/eframe **0.37.0**, glow backend, `persistence` feature; requires Rust ≥ 1.95.
- eframe 0.37 API: `App::ui(&mut Ui, &mut Frame)`; `Panel::top/left/right/bottom`;
  `Window::show(&Context)`; `Context::style_of(egui::Theme)`;
  `Context::egui_wants_keyboard_input()`; `cc.storage` is a field.
- Remote: `origin git@github.com:paleaxis/epsilon.git`, branch `main`.

## Commands and results (final state for this task)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | **27 passed, 0 failed** (16 app, 11 core) |
| `cargo build --workspace` | success, no warnings |
| `timeout 8 ./target/debug/epsilon` | exit 124 (alive, no stderr) |

App tests cover: persistence round-trip through a fake eframe `Storage`,
missing/corrupt settings fallback, `persist_egui_memory` enabled, tool
shortcuts, theme application/contrast, and headless shell layout at 720×480 and
1920×1080 in both themes.

## Features implemented

- Persisted theme and active tool across restarts; window geometry and panel
  widths delegated to eframe persistence.
- Tool palette with shortcuts, painted icons and accessible names (UI state only).
- Dark/light themes with a central palette and tested contrast.
- Six-region shell with resize-safe layout budgets.
- Functional About/Quit/theme menus; properties empty state.
- UI-independent core geometry + documentation baseline.

## Known issues / limitations

- Menus/toolbar (1.4), properties panel (1.5), live status readouts (1.6) and
  shortcut registry/help (1.7) remain open; 1.8 was completed early by request.
- Canvas and drawing (Stages 2–3) are not implemented.
- Window-size/panel-width restoration relies on eframe/egui and was not
  independently reproduced (tiling compositor); settings fallback is tested.
- No CI; local verification only. No screenshot review has been performed.
- Project license: GPL-3.0 (`LICENSE`); manifests declare `GPL-3.0-only`.

## Resume instructions

1. Read `AGENTS.md`, `project/ROADMAP.md`, this file, `project/DECISIONS.md`.
2. `git status` must be clean; run `cargo test --workspace` for the baseline.
3. Implement only task **1.4** (functional File/Edit/View/Help menus and top
   toolbar; no dead entries).
4. Verify format, clippy, tests, build, and `timeout 8 ./target/debug/epsilon`.
5. Update this file, `ROADMAP.md`, `CHANGELOG.md`, then commit and push.

Never close windows through the compositor during verification; only terminate
PIDs started for the test.
