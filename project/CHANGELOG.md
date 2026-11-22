# Changelog

Notable changes, newest first. Version sections are added at release time.

## [Unreleased]

### Added — 2026-11-23 (task 3.13)

- Deterministic seeded hand-drawn stroke rendering with a persisted clean
  architect-style toggle.

### Added — 2026-11-23 (task 3.12)

- Full element styling: colors, width, opacity, dashed/dotted strokes,
  solid/hatch/cross-hatch fills and rectangle roundness.
- Selection style controls with multi-element application.

### Added — 2026-11-23 (task 3.11)

- Rotation handle with 15-degree Shift snapping and collective multi-element
  rotation.
- Rotated polygon rendering and angle-aware selection bounds.

### Added — 2026-11-23 (task 3.10)

- Move and eight-handle resize for selections, including multi-element
  transforms, Shift aspect lock, Alt-from-center and minimum size.
- Screen-space selection handles and drift-free transform snapshots.

### Added — 2026-11-23 (task 3.9)

- Selection tool with topmost hit testing, shift-toggle, marquee selection and
  selection outlines.
- Rotation-aware element hit tests and marquee bounds in core.

### Added — 2026-11-23 (task 3.8)

- Text tool with click/drag placement, multiline in-canvas editing, wrapped
  measurement and Esc/click-out commit.
- Text content, font size and alignment in the document model.

### Added — 2026-11-23 (task 3.7)

- Freehand tool with world-space sampling, smoothing, simplification and a
  bounded point budget.

### Added — 2026-11-23 (task 3.6)

- Arrow tool with zoom-independent arrowheads and the shared Shift angle
  constraint.
- Core arrowhead geometry with short-shaft clamping.

### Added — 2026-11-23 (task 3.5)

- Line tool with live preview, world-space endpoints and Shift-constrained
  15-degree angles.
- Element `points` storage for linear geometry, with line bounds derived from
  the endpoints.

### Added — 2026-11-23 (task 3.4)

- Diamond tool with drag preview and document rendering.
- Core `Rect::diamond_points` geometry for the inscribed diamond polygon.

### Added — 2026-11-23 (task 3.3)

- Ellipse tool with drag preview and document rendering.

### Added — 2026-11-23 (task 3.2)

- Screen-space element rendering with paint-order support.
- Rectangle tool with live drag preview, normalized world bounds and minimum-
  size discard.

### Added — 2026-11-22 (task 3.1)

- Core document model with schema version, canvas settings and element base
  fields, serialized with serde.
- Stable element IDs, paint-order semantics and JSON round-trip tests.

### Added — 2026-11-22 (task 2.6)

- Explicit idle/panning/dragging pointer state machine for the canvas.
- Hover tracking and canvas-only pan starts; world coordinates remain `f64`.

### Added — 2026-11-22 (task 2.5)

- Adaptive canvas grid with View-menu and properties-panel toggles.
- Grid state is shown in the status bar; snapping helpers live in core.

### Added — 2026-11-22 (task 2.4)

- Ctrl+wheel and pinch zoom anchored at the cursor, plus +/- and reset buttons.
- Plus/Minus/0 shortcuts zoom and reset; wheel scrolls, Shift+wheel scrolls
  horizontally.
- Status bar now reports the live zoom percentage.

### Added — 2026-11-22 (task 2.3)

- Canvas panning via middle-button drag, space+drag and hand-tool drag.
- Viewport offset updates 1:1 with screen-space pointer motion; cursor feedback
  for pan modes.

### Added — 2026-11-22 (task 2.2)

- Canvas widget allocates its rect, paints the scene background, tracks the
  pointer in world coordinates, and draws a world-origin marker through the
  viewport.
- Status bar now shows live world pointer coordinates and zoom percentage.

### Added — 2026-11-22 (task 2.1)

- `epsilon-core::viewport::Viewport` with world↔screen transforms,
  cursor-anchored zoom, zoom clamping (0.1–30) and fit-to-rect.

### Changed — 2026-11-22 (task 1.9)

- Tidied tool-rail spacing and separators and compacted status-bar typography.
- Added headless layout coverage at 150% pixels-per-point.

### Added — 2026-11-22 (task 1.7)

- Central `KEYBINDINGS` registry for implemented tool and help shortcuts.
- Help overlay (F1 or `?`) listing all registered shortcuts; Escape closes it.
- Tests for duplicate bindings, tool coverage, overlay toggling and closing.

### Added — 2026-11-22 (task 1.6)

- Status bar now reports the active tool plus `Pointer: —` and `Zoom: —`
  placeholders (replacing the generic "Ready" label) and the app version.

### Added — 2026-11-22 (task 1.5)

- Right properties panel is now collapsible through `Panel::show_collapsible`,
  with a "Hide" control and drag-to-collapse/reopen behavior.
- Panel body uses a "Selection" collapsing section; the real empty state
  ("No selection") is shown until Stage 3 binds element properties.
- Fixed 200–400px resize range and persisted egui panel width are retained.

### Added — 2026-11-22 (task 1.4)

- Menu bar rebuilt around working actions only: File → Quit, View → Dark mode,
  Help → About Epsilon.
- Top toolbar now hosts the app identity, theme toggle and About button; the
  document title remains visible.
- Shared action helpers for theme toggling, About and Quit, covered by tests
  (including Quit emitting `ViewportCommand::Close`).

### Added — 2026-11-22 (task 1.8, done out of order)

- eframe persistence enabled; theme and active tool are saved to the app storage
  and restored on startup, with defaults on missing/corrupt data.
- Window geometry persisted through eframe's window settings; resizable panel
  widths persist through egui memory.
- Properties panel uses a fixed 200–400px range so transient startup frames can
  no longer clamp a saved width.

### Added — 2026-11-22 (task 1.3)

- Tool model with nine tools: Select, Hand, Rectangle, Ellipse, Diamond, Line,
  Arrow, Freehand and Text.
- Left tool rail with 20×20 vector icons painted from theme colors, selected and
  hover states, tooltips with shortcut hints, and accessible button info.
- Single-key tool shortcuts (V, H, R, O, D, L, A, P, T) that ignore modified
  keys and defer to focused text input; shortcut mapping and uniqueness tested.

### Added — 2026-11-22 (task 1.2)

- Dark and light themes driven by a central semantic palette in `theme.rs`.
- View → "Dark mode" toggle that applies the new theme immediately; all shell
  regions follow the active `egui::Visuals`.
- Explicit theme preference, so the OS theme cannot override the in-app choice;
  default theme is dark.
- WCAG contrast unit tests (primary text ≥ 7:1, weak text ≥ 4.5:1 against panel
  and canvas) and theme application/headless shell tests for both themes.

### Added — 2026-11-22 (task 1.1)

- Six-region native shell: menu bar, top toolbar, left tool rail, canvas region,
  right properties panel, status bar.
- Functional "Epsilon" menu with an About dialog and Quit.
- Resizable properties panel with a "No selection" empty state and a width clamp
  that keeps the canvas at least 240px wide in undersized windows.
- Shell layout constants with compile-time budget assertions and headless layout
  tests at 720×480 and 1920×1080.

### Added — 2026-11-22 (task 0.1)

- Cargo workspace with an egui-independent `epsilon-core` crate and an
  `epsilon-app` crate producing the native `epsilon` binary.
- `epsilon-core::geometry`: `Point` and `Rect` primitives with 11 unit tests.
- Native application bootstrap using eframe 0.37 (glow backend, Wayland/X11),
  1280×800 window with a 720×480 minimum.
- Project documentation: `AGENTS.md`, `README.md`, `project/PROJECT_STATE.md`,
  `project/ROADMAP.md`, `project/DECISIONS.md`, this changelog.
- Tooling baseline: rustfmt check, workspace tests, clippy with `-D warnings`,
  and a native launch smoke check.

### Notes

- No whiteboard features exist yet; the window shows placeholder content until
  Stage 1 layout work.
- Manifests declare the repository's existing GPL-3.0 license (`GPL-3.0-only`).
