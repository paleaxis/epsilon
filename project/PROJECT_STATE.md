# Project State

Last updated: 2026-11-26
Current stage: **Stage 6 — Export**
Last completed task: **6.4 — Export UI**
Next task: **6.5 — Clipboard image export**

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
altering the current document. New (Ctrl+N), Save As (Ctrl+Shift+S), and a
Save/Discard/Cancel guard before New, Open or window close protect unsaved
work. Dirty documents are autosaved every 30 seconds into a rotating recovery
directory under the app data folder; launch offers the newest valid autosave
for recovery, and normal save/open/new/discard removes recovery files. Recent
documents are recorded on save/open, persisted with app settings, listed in the
File menu and welcome panel, and missing entries are removed on open attempt.
Excalidraw scenes can be imported from File → Import Excalidraw and exported
from File → Export Excalidraw; unsupported element types are skipped with a
status warning, and unknown element/envelope fields are preserved for re-export.
A fixture-based compatibility suite covers supported scenes, unknown fields and
types, future Excalidraw versions, malformed JSON and native future-version
rejection. PNG export is implemented: core computes crop bounds, 1x/2x/4x/custom
scales, background/transparent choice and size limits, and the app rasterizes
every element kind deterministically with `tiny-skia` paths and `fontdue`
glyphs. File → Export PNG… writes the cropped image atomically. SVG export
writes the same cropped scene as world-unit vector primitives with text as
`<text>`, rough/dashed strokes, hatch fills, arrowheads and paint order intact.
File → Export… opens a unified dialog for selection or entire canvas, PNG/SVG,
1×/2×/4×/custom scale, document or transparent background and a filename, with
an explicit overwrite confirmation. Selection exports are cropped to the
selected elements only. Clipboard image export remains task 6.5.

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
  src/recovery.rs           rotating autosaves, naming/retention/fallback + 5 tests
  src/recent.rs             bounded recent paths, dedup/prune/serde + 3 tests
  src/excalidraw.rs         Excalidraw import/export, extras preservation + 6 tests
  src/export.rs             export crop bounds, scopes, scales, limits + 9 tests
  tests/excalidraw_compat.rs fixture compatibility suite + 5 tests
  tests/fixtures/           scene, future-version and malformed Excalidraw files
  src/history.rs            reversible commands, transactions, bounded undo/redo + 9 tests
  src/clipboard.rs          clipboard JSON payload, paste preparation, group-ID refresh + 7 tests
crates/epsilon-app/         eframe desktop shell
  src/main.rs               native entry point (persist_window enabled)
  src/layout.rs             shell size budgets + compile-time assertions
  src/theme.rs              semantic palettes, Visuals setup, serializable Theme
  src/tools.rs              Tool enum, shortcuts, painted icons, tool buttons
  src/export.rs             deterministic PNG rasterizer + 6 tests
  src/svg.rs                deterministic SVG exporter + 4 tests
  src/app.rs                EpsilonApp, menus, toolbar, tool state, history wiring, settings
```

Working UI actions:

| Where | Action |
| --- | --- |
| File menu | New/Open/Save/Save As, Excalidraw import/export, Export…, Export PNG/SVG, Open Recent, Quit (sends `ViewportCommand::Close`) |
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
- Export dialog: File → Export… chooses selection (disabled with no selection)
  or entire canvas, PNG/SVG, 1×/2×/4×/custom scale, document/transparent
  background and filename. The extension follows the chosen format; exporting
  to an existing file opens an Overwrite/Cancel prompt and leaves the file
  untouched until confirmed. Invalid custom scales and empty selections are
  reported in the status bar, and Cancel closes without writing (D-055).
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

## Verified commands and results (task 6.4)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | **242 passed, 0 failed** (124 app, 113 core, 5 compat) |
| `cargo build --workspace` | success, no warnings |
| `timeout 8 ./target/debug/epsilon` | exit 124 (alive, no stderr) |

Task 6.4 adds the File → Export… dialog and selection-scoped exports. New app
tests check selection PNG dimensions (54×64 for one rectangle vs 144×64 for the
whole scene), selection SVG viewBox (98,18,54,64 for the ellipse), 2× scale
(286×126), invalid custom scale, empty selection, alias/scope helpers, cancel
safety and overwrite confirmation, plus a headless render of the dialog; core
also pins `ExportScope`'s default. The dialog UI was rendered only in headless
or unit paths; live menu clicks, the overwrite prompt and native file dialogs
were not manually inspected. The SVG visual comparison and Excalidraw real-app
checks remain pending manual items. Images/`files`, frames, bindings and
arrowhead variants are not fully mapped.

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
- File → New/Open/Save/Save As with native rfd dialogs and Ctrl+N/O/S/Shift+S,
  atomic same-directory temp write plus rename, dirty snapshot tracking,
  title/status file indicators and user-visible failures without data loss.
- Save/Discard/Cancel guard before New, Open and window close; saving resolves
  the guard only after the write succeeds, and a cancelled Save As leaves the
  prompt and document intact.
- 30-second autosave of dirty documents to a rotating recovery directory (five
  retained), startup Recover/Discard offer using the newest valid file, source
  path restoration, and cleanup on normal save/open/new/discard/quit.
- Persisted recent documents (max 8) shown in File → Open Recent and the welcome
  panel, with deduplication, clear-all and stale-entry removal on open.
- Excalidraw import for rectangles, ellipses, diamonds, lines, arrows, freehand
  and text, including styles, groups, points, zoom/grid/background, with
  warnings for unsupported types.
- Excalidraw export with required envelope/app-state fields, relative line
  points, preserved custom/unknown fields and atomic writes; import/export menu
  entries using native dialogs.
- Fixture-based Excalidraw/native compatibility suite covering supported
  scenes, unknown fields/types, future versions and malformed inputs.
- PNG export pipeline: core crop bounds/scales/limits and a deterministic
  `tiny-skia`/`fontdue` rasterizer wired to File → Export PNG.
- SVG export for every element kind with the same cropped viewBox, rotation,
  rough/dash/dot strokes, hatch fills, arrowheads, multi-line text and stable
  paint order, wired to File → Export SVG.
- Unified export dialog with selection/entire-canvas scope, PNG/SVG, scale,
  background and filename options, normalized extensions, overwrite
  confirmation and surfaced validation errors.
- Selection-only PNG and SVG exports cropped to the selected world bounds.
- PNG export with scene-bounds cropping, 1x/2x/4x/custom scale, optional
transparent background, deterministic element rasterization (rough strokes,
dash/dot strokes, hatch fills, arrowheads, wrapped text), export size limits and
atomic writes from File → Export PNG.
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

## Manual acceptance checklist — Excalidraw export (task 5.7)

Automated tests cover the export schema, required fields, relative line points,
preserved custom data and re-import. The real-application check cannot run here
(no browser/JS automation is permitted), so it remains an explicit manual gate:

- [ ] Launch `./target/debug/epsilon` and draw a rectangle with a fill, an
  ellipse, a line, an arrow, a freehand stroke and a text element.
- [ ] Give at least one element a non-default stroke/fill color, width, opacity,
  dashed/dotted stroke or hatch fill, and group two elements.
- [ ] File → Export Excalidraw…, save `/tmp/epsilon-export-check.excalidraw`.
- [ ] Open <https://excalidraw.com> (or Excalidraw desktop), choose Open, and
  select that file.
- [ ] Confirm all six element kinds appear at the same positions, with colors,
  fills, stroke styles, opacity, text and group selection intact; no parse or
  schema warning appears.
- [ ] File → Import Excalidraw… in a fresh Epsilon window, select the same file,
  and confirm the scene renders and the status bar reports no skipped supported
  types.

Status: **pending** — not performed in this environment.

## Manual acceptance checklist — PNG export (task 6.2)

Automated tests cover dimensions, cropping, transparency, text pixels, repeat
render determinism, size limits and the atomic write. The file dialog and visual
result were not inspected:

- [ ] Launch `./target/debug/epsilon` and draw shapes with fill, rough strokes,
  dashed/dotted strokes, hatch fill, an arrow and wrapped multi-line text.
- [ ] File → Export PNG…, save `/tmp/epsilon-export-check.png`.
- [ ] Open the PNG and confirm the image is cropped to the drawing (no canvas
  padding or UI panels), strokes are not clipped, and text matches the canvas.
- [ ] Toggle View → Clean rendering and export again; rough jitter should be
  absent in the second image.
- [ ] Export an empty document and confirm the status bar reports that there is
  nothing to export instead of writing a file.

Status: **pending** — not performed in this environment. Rotated text and full
egui text shaping are known fidelity gaps (D-053).

## Manual acceptance checklist — SVG export (task 6.3)

Automated tests cover the cropped viewBox and dimensions, every element kind,
paint order, determinism and an on-disk export. The rendered result was not
compared with the canvas:

- [ ] Launch `./target/debug/epsilon` and draw shapes with fill, rough strokes,
  dashed/dotted strokes, hatch fill, an arrow and rotated text.
- [ ] File → Export SVG…, save `/tmp/epsilon-export-check.svg`.
- [ ] Open the SVG in a browser or vector editor and confirm the crop matches
  the drawing, paint order and colors match, and strokes are not clipped.
- [ ] Confirm multi-line text appears with the expected alignment and fallback
  font; compare its wrapping with the canvas and note differences.
- [ ] Export an empty document and confirm the status bar reports that there is
  nothing to export instead of writing a file.

Status: **pending** — not performed in this environment.

## Manual acceptance checklist — export dialog (task 6.4)

Automated tests cover selection cropping, scale, invalid input, cancel safety
and overwrite confirmation without the live window:

- [ ] Launch `./target/debug/epsilon`, draw two separated shapes and select one.
- [ ] File → Export…, choose Selection + PNG + 2×, keep the document background,
  and export to a new file; confirm the image is cropped to the selected shape
  at twice its size and does not include the other shape.
- [ ] Repeat with SVG and confirm the viewBox covers only the selection.
- [ ] Export again to the same filename and confirm the Overwrite/Cancel prompt
  appears; Cancel must leave the old file unchanged.
- [ ] Clear the selection and confirm the Selection radio is disabled; choose a
  custom scale of `0`, `-1` or `abc` and confirm an error is shown.
- [ ] Cancel the dialog and confirm no file is written.

Status: **pending** — not performed in this environment.

## Known issues / limitations

- Edit contains Undo/Redo, Cut/Copy/Paste, Select All, Duplicate, Delete, Group,
  Ungroup, Order and the Arrange alignment/distribution submenu. File has
  New/Open/Open Recent/Save/Save As, Excalidraw Import/Export and Quit with the
  unsaved-changes guard. Excalidraw images/frames/bindings are not mapped yet.
- Stage 5 is complete; Stage 6 now has PNG and SVG pipelines plus the unified
  export dialog. Clipboard image export remains. Manual acceptance remains for
  opening an Excalidraw export in the real app, visually checking PNG/SVG
  exports, and clicking the export dialog, overwrite prompt and native dialogs.
- PNG text uses `fontdue` per-glyph advances and greedy word wrapping, not
  egui's shaping/kerning; rotated text is exported unrotated. Rough strokes are
  deterministic but not pixel-identical to the canvas anti-aliasing.
- Direct File → Export PNG…/Export SVG… still export the whole document at 1×
  with the document background; use File → Export… for selection, scale and
  background options. The dialog's typed filename keeps its old extension when
  the format changes, although the written file gets the correct one.
- SVG text is emitted as `<text>` with explicit-newline only wrapping and a
  generic sans-serif fallback, so its appearance depends on the viewer's font
  and is not pixel-identical to the canvas (D-054).
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
3. Start with the first incomplete task, 6.5 — Clipboard image export.
4. Verify format, clippy, tests, build, and `timeout 8 ./target/debug/epsilon`.
5. Update this file, `ROADMAP.md`, `CHANGELOG.md`, then commit and push.

Never close windows through the compositor during verification; only terminate
PIDs started for the test.
