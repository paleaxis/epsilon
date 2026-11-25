# Project State

Last updated: 2026-11-25
Current stage: **Stage 4 — Editing experience**
Last completed task: **4.2 — Undo/redo integration**
Next task: **4.3 — Clipboard: copy, cut, paste**

## Status

The shell has working File/View/Help menus and a toolbar with app identity,
theme toggle and About. Theme/tool settings and window/panel state persist from
task 1.8 (completed early). The canvas pans, zooms, shows an adaptive grid, and
the Rectangle, Ellipse, Diamond, Line, Arrow and Freehand tools create
persistent in-memory shapes with live previews, and the Text tool places and
edits multiline text in the canvas. The app now routes shape, freehand and text
creation, move/resize/rotation drags and properties/style edits through the
core's bounded, transaction-based undo/redo history; Ctrl+Z, Ctrl+Shift+Z,
Ctrl+Y, the Edit menu and toolbar buttons all drive it.

Repository layout:

```
Cargo.toml                  workspace (edition 2024, rust 1.95+)
AGENTS.md                   permanent rules
README.md                   build/run overview
project/                    STATE, ROADMAP, DECISIONS, CHANGELOG
crates/epsilon-core/        UI-independent engine (no egui dependency)
  src/geometry.rs           Point, Rect, shapes, rough strokes + 29 tests
  src/document.rs           schema-versioned document, shapes, text, styles, paint order
  src/history.rs            reversible commands, transactions, bounded undo/redo + 9 tests
crates/epsilon-app/         eframe desktop shell
  src/main.rs               native entry point (persist_window enabled)
  src/layout.rs             shell size budgets + compile-time assertions
  src/theme.rs              semantic palettes, Visuals setup, serializable Theme
  src/tools.rs              Tool enum, shortcuts, painted icons, tool buttons
  src/app.rs                EpsilonApp, menus, toolbar, tool state, history wiring, settings
```

Working UI actions:

| Where | Action |
| --- | --- |
| File menu | Quit (sends `ViewportCommand::Close`) |
| Edit menu | Undo (Ctrl+Z) and Redo (Ctrl+Shift+Z / Ctrl+Y), enabled by history depth |
| View menu | Dark mode, grid and clean-rendering toggles, applied immediately |
| Help menu | About Epsilon (dialog) |
| Top toolbar | Wordmark, document title, undo/redo buttons, zoom buttons, theme checkbox, About button, version |
| Canvas | Shape tools draw; Select picks/toggles/marquees, moves, resizes and rotates; text overlays edit in place; middle/space/hand drag pans; wheel zooms |
| Properties | Live transform/style editor with mixed values; panel hide control |
| Status bar | Tool, pointer, zoom, grid, undo/redo depths, history errors, version |

Only working actions are listed; Edit currently exposes undo and redo, and
clipboard commands arrive in task 4.3. Menus and toolbar share the same action
helpers (`toggle_theme`, `open_about`, `request_quit`, `undo`, `redo`). Standard
egui widgets provide mouse and keyboard focus/activation behavior.

Other shell behavior:

- Tool palette: Select (V), Hand (H), Rectangle (R), Ellipse (O), Diamond (D),
  Line (L), Arrow (A), Freehand (P), Text (T), painted icons, tooltips,
  selected state; shortcuts defer to focused text input.
- Undo/redo: Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y; one history entry per finished
  creation, drag or properties edit. Undo/redo stays available when a property
  field has focus but is blocked while the canvas text editor is open.
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

## Verified commands and results (task 4.2)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | **129 passed, 0 failed** (62 app, 67 core) |
| `cargo build --workspace` | success, no warnings |
| `timeout 8 ./target/debug/epsilon` | exit 124 (alive, no stderr) |

Task 4.2 tests cover Ctrl+Z/Ctrl+Shift+Z/Ctrl+Y dispatch, undo/redo of every
drawing tool's creation, text creation recorded on editor commit, undo/redo of
editing existing text, one-step undo/redo across a mixed sequence (create, move,
resize, rotate, property position, style) with exact document equality,
redo-branch discard after a new edit, and history blocked while the text editor
is open. The suite also retains
the core history tests and all earlier shell, persistence, viewport, grid,
document, styling, rough rendering and shape coverage.

Native launch was verified by process liveness. The new Edit menu and toolbar
buttons render in the headless shell tests, but interactive menu clicks,
shortcut presses and visual enabled-state changes were not manually inspected
in a live window.

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
- Rectangle, ellipse, diamond, line, arrow, freehand and text tools with live
  previews and world-space geometry. Linear tools store endpoints and support
  Shift-constrained 15-degree angles; freehand strokes are smoothed and
  simplified; text supports wrapping, alignment and in-place editing.
- Rotation-aware hit testing, topmost click selection, additive toggling and
  marquee selection with screen-space outlines.
- Move and eight-handle resize with multi-element transforms, Shift aspect lock,
  Alt-from-center and minimum size enforcement.
- Rotation handle with Shift 15-degree snapping, collective-center multi-element
  rotation and rotated polygon rendering.
- Element styling for colors, width, opacity, stroke/fill patterns and
  rectangle roundness, with selection-wide properties controls.
- Deterministic seeded hand-drawn strokes with a persisted clean-rendering
  toggle.
- Live transform and style binding in the properties panel, including mixed
  values for multi-selection.
- UI-independent command history with insert/remove/replace commands, grouped
  transactions, bounded undo stack, redo branching and drag coalescing.
- Undo/redo wired through every Stage 3 edit, with shortcuts, Edit menu items,
  history-aware toolbar buttons and status-bar depth/error feedback.
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

- Edit currently contains only Undo/Redo; clipboard, delete, z-order and other
  editing commands arrive in tasks 4.3–4.5. File has only Quit until document
  I/O (Stage 5).
- Shell polish and manual verification pass remains (1.9).
- Undo history is session-only and is not saved with settings; document
  persistence arrives in Stage 5. While the canvas text editor is open, its
  own TextEdit undo applies instead of document history.
- No CI; local verification only. No visual/screenshot review performed.
- Window/panel restore relies on eframe/egui and was not re-measured under niri.
- Project license: GPL-3.0 (`LICENSE`); manifests declare `GPL-3.0-only`.

## Resume instructions

1. Read `AGENTS.md`, `project/ROADMAP.md`, this file, `project/DECISIONS.md`.
2. `git status` must be clean; run `cargo test --workspace` for the baseline.
3. Start with the first incomplete task, 4.3 — Clipboard: copy, cut, paste.
4. Verify format, clippy, tests, build, and `timeout 8 ./target/debug/epsilon`.
5. Update this file, `ROADMAP.md`, `CHANGELOG.md`, then commit and push.

Never close windows through the compositor during verification; only terminate
PIDs started for the test.
