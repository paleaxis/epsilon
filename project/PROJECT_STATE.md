# Project State

Last updated: 2026-11-22
Current stage: **Stage 1 — Native desktop shell**
Last completed task: **1.5 — Right properties panel shell**
Next task: **1.6 — Status bar**

## Status

The shell has working File/View/Help menus and a toolbar with app identity,
theme toggle and About. Theme/tool settings and window/panel state persist from
task 1.8 (completed early). The whiteboard is still not usable: the canvas is a
placeholder until Stage 2 and tools do not draw until Stage 3.

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
  src/theme.rs              semantic palettes, Visuals setup, serializable Theme
  src/tools.rs              Tool enum, shortcuts, painted icons, tool buttons
  src/app.rs                EpsilonApp, menus, toolbar, tool state, settings
```

Working UI actions:

| Where | Action |
| --- | --- |
| File menu | Quit (sends `ViewportCommand::Close`) |
| View menu | "Dark mode" checkbox, applied immediately |
| Help menu | About Epsilon (dialog) |
| Top toolbar | Wordmark, document title, theme checkbox, About button, version |

Only working actions are listed; the Edit menu is intentionally absent until
undo/clipboard stage 4 exists. Menus and toolbar share the same action helpers
(`toggle_theme`, `open_about`, `request_quit`). Standard egui widgets provide
mouse and keyboard focus/activation behavior.

Other shell behavior:

- Tool palette: Select (V), Hand (H), Rectangle (R), Ellipse (O), Diamond (D),
  Line (L), Arrow (A), Freehand (P), Text (T), painted icons, tooltips,
  selected state; shortcuts defer to focused text input.
- Theme palette in `theme.rs`, default dark, explicit `ThemePreference`.
- Six regions: menu bar, top toolbar, left rail, canvas placeholder, collapsible
  properties panel (200–400px, "No selection" empty state), status bar.
- Persistence: eframe `persistence` enabled; `epsilon.settings` holds theme/tool;
  window geometry and egui panel memory persist through eframe.
- Layout budgets are compile-time checked (canvas ≥ 240px at 720×480).

## Environment

- Linux (Arch/CachyOS), niri Wayland session; Rust 1.100.0 (system toolchain,
  no rustup), cargo 1.100.0.
- egui/eframe **0.37.0**, glow backend, persistence; requires Rust ≥ 1.95.
- eframe 0.37 API: `App::ui(&mut Ui, &mut Frame)`; `Panel::top/left/right/bottom`;
  `Window::show(&Context)`; `Context::style_of(egui::Theme)`;
  `Context::egui_wants_keyboard_input()`; `cc.storage` is a field.
- Remote: `origin git@github.com/paleaxis/epsilon.git`, branch `main`.

## Verified commands and results (task 1.4)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | **32 passed, 0 failed** (21 app, 11 core) |
| `cargo build --workspace` | success, no warnings |
| `timeout 8 ./target/debug/epsilon` | exit 124 (alive, no stderr) |

New tests cover About opening, Quit sending `ViewportCommand::Close`, and theme
toggling applying immediately. Existing tests cover persistence, layout, tool
shortcuts, contrast and both-theme shell rendering.

Native launch was verified by process liveness. Menu clicks and keyboard
navigation were not automated or visually reviewed; only the action functions
behind them are tested.

## Features implemented

- Working menus (File/Quit, View/theme, Help/About) and toolbar actions.
- Collapsible/resizable properties panel with section header and empty state.
- Persisted theme/tool; eframe-persisted window geometry and panel widths.
- Tool palette with shortcuts, painted icons and accessible names.
- Dark/light themes with central palette and contrast tests.
- Six-region shell with resize-safe layout budgets.
- UI-independent core geometry; documentation baseline.

## Known issues / limitations

- Edit menu and formatting actions are not implemented (Stage 4); File has only
  Quit until document I/O (Stage 5).
- Status bar readouts are static (1.6); shortcut registry/help overlay absent
  (1.7).
- Canvas and drawing (Stages 2–3) are not implemented.
- No CI; local verification only. No visual/screenshot review performed.
- Window/panel restore relies on eframe/egui and was not re-measured under niri.
- Project license: GPL-3.0 (`LICENSE`); manifests declare `GPL-3.0-only`.

## Resume instructions

1. Read `AGENTS.md`, `project/ROADMAP.md`, this file, `project/DECISIONS.md`.
2. `git status` must be clean; run `cargo test --workspace` for the baseline.
3. Implement only task **1.6** (status bar: active tool, pointer/zoom
   placeholders, app version).
4. Verify format, clippy, tests, build, and `timeout 8 ./target/debug/epsilon`.
5. Update this file, `ROADMAP.md`, `CHANGELOG.md`, then commit and push.

Never close windows through the compositor during verification; only terminate
PIDs started for the test.
