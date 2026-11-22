# Changelog

Notable changes, newest first. Version sections are added at release time.

## [Unreleased]

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
