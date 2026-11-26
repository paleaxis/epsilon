# Project State

Last updated: 2026-11-26
Current stage: **Stage 5 — Documents and persistence**
Last completed task: **5.2 — Save and open**
Next task: **5.3 — New, Save As, close guard**

## Status

The shell has working File/View/Help menus and a toolbar with app identity,
theme toggle and About. Theme/tool settings and window/panel state persist from
task 1.8 (completed early). The canvas pans, zooms, shows an adaptive grid, and
the Rectangle, Ellipse, Diamond, Line, Arrow and Freehand tools create
persistent in-memory shapes with live previews, and the Text tool places and
edits multiline text in the canvas. The app now routes shape, freehand and text
creation, move/resize/rotation drags and properties/style edits through the
core's bounded, transaction-based undo/redo history; Ctrl+Z, Ctrl+Shift+Z,
Ctrl+Y, the Edit menu and toolbar buttons all drive it. Cut, copy and paste are
wired through a portable JSON payload: copies get fresh IDs and paint-order
slots, repeated pastes cascade by 10 world units, and cut/copy/paste are
available from Ctrl+X/C/V and the Edit menu. Delete/Backspace remove the
selection, Ctrl+D duplicates it one grid step away, Ctrl+A selects every
element, and Esc clears the selection and returns to the Select tool. Bring to
front/send to back and one-step bring forward/send backward work from shortcuts
and the Edit → Order submenu, preserving the selection and recording one undo
entry. Right-clicking the canvas targets the element under the pointer and
opens a context menu containing only the commands valid for that target.
Grouping wraps the selection in a new outermost group, ungrouping peels one
nesting level, and clicks, marquees, shift-toggles and right-click targeting
select whole groups. Each group operation is one undo entry. Alignment moves
two or more selected elements onto a shared left/center/right or
top/middle/bottom edge using rotated world bounds, as one undo transaction.
Distribution equalises gaps for three or more elements horizontally or
vertically while keeping the outermost elements fixed. The native `.epsilon`
format exists in core as a versioned JSON envelope with document, viewport app
state and preserved extras maps. File → Open/Save (Ctrl+O/Ctrl+S) use native
dialogs and atomic temp-file-plus-rename writes; the title and status bar show
the file name and dirty state, and failed operations are reported without
altering the current document.

Repository layout:

```
Cargo.toml                  workspace (edition 2024, rust 1.95+)
AGENTS.md                   permanent rules
README.md                   build/run overview
project/                    STATE, ROADMAP, DECISIONS, CHANGELOG
crates/epsilon-core/        UI-independent engine (no egui dependency)
  src/geometry.rs           Point, Rect, shapes, rough strokes + 29 tests
  src/document.rs           schema-versioned document, shapes, text, styles, paint order, groups, arrange operations
  src/format.rs             native .epsilon v1 envelope, atomic file I/O, extras + 6 tests
  src/history.rs            reversible commands, transactions, bounded undo/redo + 9 tests
  src/clipboard.rs          clipboard JSON payload, paste preparation, group-ID refresh + 7 tests
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
| Edit menu | Undo/Redo (history-aware), Cut/Copy/Paste, Select All, Duplicate, Delete and Order (front/forward/backward/back) |
| View menu | Dark mode, grid and clean-rendering toggles, applied immediately |
| Help menu | About Epsilon (dialog) |
| Top toolbar | Wordmark, document title, undo/redo buttons, zoom buttons, theme checkbox, About button, version |
| Canvas | Shape tools draw; Select picks/toggles/marquees, moves, resizes and rotates; text overlays edit in place; middle/space/hand drag pans; wheel zooms; right-click targets and opens a context menu |
| Properties | Live transform/style editor with mixed values; panel hide control |
| Status bar | Tool, pointer, zoom, grid, undo/redo depths, history/clipboard feedback, version |

Only working actions are listed; Edit exposes undo, redo, cut, copy, paste,
select all, duplicate, delete and the four z-order moves; the canvas context
menu shows the applicable subset. Menus and toolbar share the same action helpers (`toggle_theme`,
`open_about`, `request_quit`, `undo`, `redo`, `cut_selection`,
`copy_selection`, `paste_from_menu`, `select_all`, `duplicate_selection`,
`delete_selection`, `move_selection_in_z_order`). Standard egui widgets provide
mouse and keyboard focus/activation behavior.

Other shell behavior:

- Tool palette: Select (V), Hand (H), Rectangle (R), Ellipse (O), Diamond (D),
  Line (L), Arrow (A), Freehand (P), Text (T), painted icons, tooltips,
  selected state; shortcuts defer to focused text input.
- Undo/redo: Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y; one history entry per finished
  creation, drag or properties edit. Undo/redo stays available when a property
  field has focus but is blocked while the canvas text editor is open.
- Selection commands: Delete or Backspace remove the selection as one undoable
  transaction; Ctrl+D duplicates it by one `DEFAULT_GRID_SIZE` (20 world units)
  with fresh IDs and selects the copies; Ctrl+A selects every element; Esc
  clears the selection and switches to the Select tool. Esc closes the help
  overlay first, and all four are ignored while a drag, property edit or canvas
  text editor is in progress.
- Context menu: right-clicking an element selects it (unless part of the
  current selection) and offers Cut/Copy/Paste, Duplicate, Delete and the four
  Order moves; right-clicking empty canvas clears the selection and offers only
  Paste/Select All when they apply. Empty action lists open no popup, and no
  context menu is attached while a canvas text editor is open. Labels take
  their shortcut text from the central registry.
- Z-order: Ctrl+Shift+] brings the selection to the front, Ctrl+] moves it
  forward one slot, Ctrl+[ moves it backward and Ctrl+Shift+[ sends it to the
  back; Edit → Order exposes the same four commands. Front/back keep the
  selection's relative order; forward/backward move a contiguous block as a
  group. Each effective move is one undo transaction, the selection is
  preserved, and moves that would change nothing record no history.
- Clipboard: Ctrl+C/Ctrl+X write a versioned `epsilon/clipboard` JSON payload to
  the system clipboard (eframe reports these as clipboard events, not key
  presses); Ctrl+V parses platform clipboard text and ignores non-Epsilon text.
  The Edit menu pastes the in-app payload because egui exposes no synchronous
  clipboard read. Paste assigns fresh IDs, preserves relative paint order above
  existing elements, offsets by 10 world units per paste, selects the copies and
  records one undoable insert transaction. Cut is copy plus one removal
  transaction. Clipboard actions are disabled while a canvas text editor is open.
  Status bar reports Copied/Cut/Pasted counts and unsupported payloads.
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

## Verified commands and results (task 5.2)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | **182 passed, 0 failed** (92 app, 90 core) |
| `cargo build --workspace` | success, no warnings |
| `timeout 8 ./target/debug/epsilon` | exit 124 (alive, no stderr) |

Task 5.2 adds core tests for on-disk save/load/overwrite, malformed files,
missing files, failed writes leaving existing content and no temp files behind.
App tests cover save/open round-trip, dirty-to-clean transitions, title/status
labels, malformed open leaving the document untouched, failed save remaining
dirty, extension handling and Ctrl+O/Ctrl+S registration. The suite also
retains task 5.1 format tests and all Stage 4 and earlier coverage.

Native launch was verified by process liveness. Path-level save/open was tested
to disk; the native dialogs themselves were not clicked in a live window, and
read-only filesystem behaviour was represented by an invalid parent path.

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
- Cut/copy/paste through a versioned clipboard JSON payload with fresh IDs,
  preserved paint order, cascading offsets, one-transaction paste/cut, system
  clipboard text interop and status-bar feedback.
- Delete/Backspace, Ctrl+D duplicate with a grid-step offset, Ctrl+A select all,
  and Esc clear-selection-return-to-Select, all exposed in the Edit menu.
- Bring to front/forward, send backward/to back through Ctrl+[ / Ctrl+] with
  Shift variants, one undo entry per move and a selection that survives reorder.
- Right-click canvas context menu with target-aware, shortcut-labelled commands
  and no menu while editing text.
- Group/ungroup through Ctrl+G / Ctrl+Shift+G, Edit and context menus, with
  nested outermost-first group IDs, whole-group selection, one undo entry per
  operation and fresh group IDs on paste/duplicate.
- Align left/center/right and top/middle/bottom from Edit → Arrange for two or
  more selected elements, using rotated world bounds and one undo entry.
- Distribute horizontally/vertically from Edit → Arrange for three or more
  selected elements with equal gaps, fixed outer elements and one undo entry.
- Core `.epsilon` v1 JSON envelope with schema version, document, viewport app
  state, and unknown fields preserved at envelope, document and app-state
  levels; future versions and foreign formats get explicit errors.
- File → Open/Save with native rfd dialogs, Ctrl+O/Ctrl+S, atomic same-directory
  temp write plus rename, dirty snapshot tracking, title/status file indicators
  and user-visible failures without data loss.
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

- Edit contains Undo/Redo, Cut/Copy/Paste, Select All, Duplicate, Delete, Group,
  Ungroup, Order and the Arrange alignment/distribution submenu. File has
  Open/Save and Quit; New, Save As and the unsaved-changes close guard arrive in
  task 5.3.
- Arrange commands have no keyboard shortcuts; they are available only from the
  Edit menu, and the menu itself was not clicked in a live window.
- Z-order moves rewrite paint slots densely (0..n), so elements whose slot
  actually changed are replaced; this normalises any non-dense imported
  z-orders the first time a move touches them.
- Menu Paste uses the last in-app copy; Ctrl+V reads platform clipboard text.
  There is no synchronous clipboard read, so a payload copied by another Epsilon
  window cannot be pasted from the menu. Paste silently ignores non-Epsilon text
  apart from status-bar feedback.
- Shell polish and manual verification pass remains (1.9).
- Undo history is session-only and is not saved with settings; only the
  serialization envelope exists, with save/open arriving in task 5.2. While the
  canvas text editor is open, its own TextEdit undo applies instead of document
  history.
- No CI; local verification only. No visual/screenshot review performed.
- Window/panel restore relies on eframe/egui and was not re-measured under niri.
- Project license: GPL-3.0 (`LICENSE`); manifests declare `GPL-3.0-only`.

## Resume instructions

1. Read `AGENTS.md`, `project/ROADMAP.md`, this file, `project/DECISIONS.md`.
2. `git status` must be clean; run `cargo test --workspace` for the baseline.
3. Start with the first incomplete task, 5.3 — New, Save As, close guard.
4. Verify format, clippy, tests, build, and `timeout 8 ./target/debug/epsilon`.
5. Update this file, `ROADMAP.md`, `CHANGELOG.md`, then commit and push.

Never close windows through the compositor during verification; only terminate
PIDs started for the test.
