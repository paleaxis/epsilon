# Project State

Last updated: 2026-11-23
Current stage: **Stage 3 — Drawing engine**
Last completed task: **3.4 — Diamond tool**
Next task: **3.5 — Line tool**

## Status

The shell has working File/View/Help menus and a toolbar with app identity,
theme toggle and About. Theme/tool settings and window/panel state persist from
task 1.8 (completed early). The canvas pans, zooms, shows an adaptive grid, and
the Rectangle, Ellipse and Diamond tools create persistent in-memory shapes
with live previews.

Repository layout:

```
Cargo.toml                  workspace (edition 2024, rust 1.95+)
AGENTS.md                   permanent rules
README.md                   build/run overview
project/                    STATE, ROADMAP, DECISIONS, CHANGELOG
crates/epsilon-core/        UI-independent engine (no egui dependency)
  src/geometry.rs           Point, Rect, diamond vertices + 12 unit tests
  src/document.rs           schema-versioned document, elements, IDs, paint order
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
| Canvas | Rectangle/Ellipse/Diamond drag creates shapes; middle/space/hand drag pans; wheel zooms |
| Properties | Show-grid toggle and panel hide control |

Only working actions are listed; the Edit menu is intentionally absent until
undo/clipboard stage 4 exists. Menus and toolbar share the same action helpers
(`toggle_theme`, `open_about`, `request_quit`). Standard egui widgets provide
mouse and keyboard focus/activation behavior.

Other shell behavior:

- Tool palette: Select (V), Hand (H), Rectangle (R), Ellipse (O), Diamond (D),
  Line (L), Arrow (A), Freehand (P), Text (T), painted icons, tooltips,
  selected state; shortcuts defer to focused text input.
- Theme palette in `theme.rs`, default dark, explicit `ThemePreference`.
- Six regions: menu bar, top toolbar, left rail, pannable/zoomable canvas, collapsible
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

## Verified commands and results (task 3.4)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | **70 passed, 0 failed** (36 app, 34 core) |
| `cargo build --workspace` | success, no warnings |
| `timeout 8 ./target/debug/epsilon` | exit 124 (alive, no stderr) |

Task 3.4 tests cover the core diamond vertices and an end-to-end drag plus a
zoomed screen-projection check that the polygon touches the element bounds.
The suite also retains shell, persistence, viewport, grid, document and shape
coverage from earlier tasks.

Native launch was verified by process liveness. Menu clicks and keyboard
navigation were not automated or visually reviewed; only the action functions
behind them are tested.

## Features implemented

- Working menus (File/Quit, View/theme, Help/About) and toolbar actions.
- Collapsible/resizable properties panel with section header and empty state.
- Status bar reports active tool, live pointer coordinates, zoom and grid state.
- Central shortcut registry and F1/? help overlay.
- Stage 1 shell polish (spacing, separators, compact status text).
- Core viewport model with world/screen transforms and zoom clamping.
- Canvas widget with background, pointer-to-world mapping and origin marker.
- Canvas panning with middle/space/hand-tool drags.
- Cursor-anchored zoom, wheel scrolling and zoom shortcuts/buttons.
- Optional adaptive grid with toggle and snapping helpers.
- Explicit idle/panning/dragging pointer state machine with hover tracking.
- Core document schema with canvas settings, element base fields, stable IDs
  and documented paint-order semantics.
- App-held document rendering with paint order and screen-space projection.
- Rectangle, ellipse and diamond tools with live preview, minimum-size discard
  and world-space bounds.
- Persisted theme/tool; eframe-persisted window geometry and panel widths.
- Tool palette with shortcuts, painted icons and accessible names.
- Dark/light themes with central palette and contrast tests.
- Six-region shell with resize-safe layout budgets.
- UI-independent core geometry; documentation baseline.

## Manual verification checklist (task 1.9)

Automated checks run for this task:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test --workspace` | 36 passed, 0 failed |
| `cargo build --workspace` | success, no warnings |
| `timeout 8 ./target/debug/epsilon` | exit 124 (alive, no stderr) |
| Headless shell at 720×480, 1920×1080 | pass |
| Headless shell at 150% pixels-per-point | pass (no panic) |

Manual visual checks not performed in this environment (no screenshot/browser
automation allowed). Items requiring a human reviewer:

- [ ] HiDPI rendering at 150% scale looks correct (text/icons/spacing).
- [ ] Keyboard focus traversal reaches every interactive control.
- [ ] Tool rail, menus, toolbar and status bar spacing looks balanced.
- [ ] Collapsed properties panel leaves a usable grab handle.

## Known issues / limitations

- Edit menu and formatting actions are not implemented (Stage 4); File has only
  Quit until document I/O (Stage 5).
- Shell polish and manual verification pass remains (1.9).
- Only rectangles, ellipses and diamonds render; line, arrow, freehand and
  text tools are still pending (Stage 3.5+).
- No CI; local verification only. No visual/screenshot review performed.
- Window/panel restore relies on eframe/egui and was not re-measured under niri.
- Project license: GPL-3.0 (`LICENSE`); manifests declare `GPL-3.0-only`.

## Resume instructions

1. Read `AGENTS.md`, `project/ROADMAP.md`, this file, `project/DECISIONS.md`.
2. `git status` must be clean; run `cargo test --workspace` for the baseline.
3. Implement only task **3.5** (line tool).
4. Verify format, clippy, tests, build, and `timeout 8 ./target/debug/epsilon`.
5. Update this file, `ROADMAP.md`, `CHANGELOG.md`, then commit and push.

Never close windows through the compositor during verification; only terminate
PIDs started for the test.
