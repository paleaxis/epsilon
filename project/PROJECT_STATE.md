# Project State

Last updated: 2026-11-27
Current stage: **Stage 7 — Desktop readiness** (7.1–7.4 complete; 7.5 in progress)
Last completed task: **7.4 — Release build and packaging docs**
Current task: **7.5 — Final usability verification and v0.1.0 (automated pass done, not tagged)**

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
selected elements only. File → Copy as Image places the selection (or the whole
canvas when nothing is selected) on the system clipboard as a PNG through
`arboard`, using the Wayland data-control protocol when available and X11
otherwise. Every failure now lands on one non-blocking toast surface (errors
and warnings, dismissible and auto-expiring) and in a rotating local log file,
while the status bar keeps only the last successful result. Stage 6 is
complete. Stage 7 now has desktop integration: the window icon is rasterized in
code, `packaging/` provides a desktop entry, MIME types for `.epsilon` and
`.excalidraw`, a scalable launcher icon and a user install script, and a file
path passed on the command line opens the document or imports the scene at
startup. Task 7.3 profiled startup, idle CPU and pan/zoom frames with 1k/5k
scenes in release mode: startup fell from ~250 ms to ~77 ms after the icon
rasterizer stopped supersampling uniform areas and pruned stroke segments, and
off-screen culling cut a 5,000-element frame from ~3.5 ms to ~0.8 ms. Task 7.4
adds a reproducible release tarball (`packaging/release.sh`), a distribution-
aware `install.sh`, documented runtime requirements and a binary-size report;
the packaged binary was launched from a fresh `HOME` and installed from the
tarball into a temporary prefix. Task 7.5's automated end-to-end pass also
succeeds (270 tests, including an 11-test focused run for draw/edit/save/open/
export/error recovery), but `v0.1.0` is deliberately not tagged while the live
GUI and cross-session checks remain manual.

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
  src/main.rs               native entry point (persist_window, icon, CLI file argument)
  src/icon.rs               procedurally rasterized window icon (uniform-area fast paths) + 4 tests
  src/layout.rs             shell size budgets + compile-time assertions
  src/theme.rs              semantic palettes, Visuals setup, serializable Theme
  src/tools.rs              Tool enum, shortcuts, painted icons, tool buttons
  src/export.rs             deterministic PNG/RGBA rasterizer + 7 tests
  src/clipboard_image.rs    system clipboard image copy + 2 tests
  src/logging.rs            rotating plain-text local log + panic hook + 4 tests
  src/notices.rs            non-blocking error/warning toast surface + 5 tests
  src/svg.rs                deterministic SVG exporter + 4 tests
  src/app.rs                EpsilonApp, menus, toolbar, tool state, history wiring, settings
packaging/
  epsilon.desktop           Linux launcher with .epsilon/.excalidraw associations
  epsilon-mime.xml          shared-mime-info definitions for both extensions
  epsilon.svg               scalable launcher icon (matches src/icon.rs)
  install.sh                user-level install into $PREFIX, from a checkout or tarball
  release.sh                release build → dist/epsilon-<version>-linux-<arch>.tar.gz + .sha256
```

Working UI actions:

| Where | Action |
| --- | --- |
| File menu | New/Open/Save/Save As, Excalidraw import/export, Export…, Export PNG/SVG, Copy as Image, Open Recent, Quit (sends `ViewportCommand::Close`) |
| Edit menu | Undo/Redo (history-aware), Cut/Copy/Paste, Select All, Duplicate, Delete and Order (front/forward/backward/back) |
| View menu | Dark mode, grid and clean-rendering toggles, applied immediately |
| Help menu | About Epsilon (dialog) |
| Top toolbar | Wordmark, document title, undo/redo buttons, zoom buttons, theme checkbox, About button, version |
| Canvas | Shape tools draw; Select picks/toggles/marquees, moves, resizes and rotates; text overlays edit in place; middle/space/hand drag pans; wheel zooms; right-click targets and opens a context menu |
| Properties | Live transform/style editor with mixed values; panel hide control |
| Status bar | Tool, pointer, zoom, grid, undo/redo depths, last successful file/clipboard result, version |

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
- Clipboard image: File → Copy as Image renders the selection, or the whole
  canvas when nothing is selected, through the cropped PNG pipeline at 1× with
  the document background and writes `image/png` to the system clipboard via
  `arboard` (Wayland data-control preferred, X11 fallback). Empty scenes,
  oversized images and unavailable clipboard backends are reported as toasts;
  nothing is written to disk. Clipboard ownership lasts while the app runs.
- Errors and warnings: every file, clipboard and history failure is routed
  through one reporting helper that logs the event and pushes a toast in the
  top-right corner (errors 12 s, warnings 8 s, dismissible, at most four,
  duplicates refreshed). File failures name the full path. The status bar shows
  only the last successful file/clipboard message. No modal error dialogs are
  used for recoverable failures.
- Local log: `main` installs a panic hook and a rotating plain-text log at
  `<app data>/logs/epsilon.log` (1 MiB cap, three numbered siblings). Records
  include a UTC timestamp, level, scope and message; the session start and each
  written/read document or export path are logged. No telemetry and no network
  access; writing failures are ignored so the editor keeps working.
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

## Verified commands and results (task 7.2)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | **269 passed, 0 failed** (151 app, 113 core, 5 compat) |
| `cargo build --workspace` | success, no warnings |
| `timeout 8 ./target/debug/epsilon` | exit 124 (alive, no stderr) |
| `PREFIX=<tmp> EPSILON_BIN=./target/debug/epsilon packaging/install.sh` | installed binary, desktop entry, icon and MIME files |
| `desktop-file-validate <tmp>/share/applications/epsilon.desktop` | exit 0, no diagnostics |
| `grep application/(x-epsilon\|vnd.excalidraw+json) <tmp>/share/applications/mimeinfo.cache` | both types map to `epsilon.desktop` |
| `PATH=<tmp>/bin:$PATH timeout 8 epsilon crates/epsilon-core/tests/fixtures/scene.excalidraw` | exit 124 (alive); log recorded `import: read …scene.excalidraw`, 4 elements imported, 1 skipped |
| `timeout 8 ./target/debug/epsilon /tmp/does-not-exist.epsilon` | exit 124 (alive); log recorded the open failure with the full path |

Task 7.2 adds `icon.rs`, which rasterizes a 256×256 RGBA icon at startup
(4×4 supersampling, rounded board and accent mark) for
`ViewportBuilder::with_icon`, so no image file is read at runtime. Four tests
cover dimensions, transparent corners/opaque surface, the accent mark and
determinism. `main` takes the first positional argument as a startup document;
three tests cover positional/option/empty parsing. `EpsilonApp` gained
`with_startup_file`, which imports `.excalidraw` paths (case-insensitive) and
natively opens every other path, suppressing the welcome/recovery prompts.
App tests cover path routing, native load, Excalidraw import and a missing-file
failure that leaves a usable empty document. The install was exercised only in
a temporary prefix, not in the user session. Previous verification: 7.1's 258
tests and log/toast checks, 6.x export pipelines (see their checklists below).

## Performance profiling (task 7.3)

Measurements were taken on the development machine (4 available cores, Rust
1.100, `--release`, thin LTO) with the ignored harness
`cargo test -p epsilon-app --release -- --ignored --nocapture profile_frame_costs`.
Each figure is the mean of 60 frames after three warm-up frames; "ui" is
`Context::run_ui` (input, layout, painting) and "tess" is `Context::tessellate`
for the same frame. Scenes are 50-column grids of filled rectangles with
45×35 world-unit spacing: 1,000 elements span 2,250×700 and 5,000 span
2,250×3,500, while the canvas is about 980×580, so the 5,000-element scene is
more than 90% off screen unless culled. Pan/zoom frames move the viewport 4/2
world units and change zoom by 2% per frame.

| Measurement | Before | After |
| --- | --- | --- |
| 1,000 elements idle frame (ui + tess) | 1.30 ms | 0.91 ms |
| 1,000 elements pan/zoom frame | 1.39 ms | 0.83 ms |
| 5,000 elements idle frame | 3.49 ms | 0.79 ms |
| 5,000 elements pan/zoom frame | 3.35 ms | 1.18 ms |
| Window icon rasterization (one call) | 127.8 ms | 15.4 ms |
| Startup until the window appears | 196–261 ms | 75–102 ms |
| Idle CPU (5 s sample) | 0.00–0.60% | 0.2–3.2% (one outlier; median 0.6%) |
| Resident memory (idle) | ~71 MB | ~71 MB |

Startup was measured externally: the release binary is started, `niri msg
windows` is polled for its PID, and the elapsed time is reported (about 10 ms
polling granularity, so treat the absolute numbers as approximate). Idle CPU
comes from `/proc/<pid>/stat` deltas over five seconds. The app only repaints
on input, so idle CPU is effectively the compositor and event-loop cost; the
3.2% sample ran while the machine was otherwise busy.

Two measured bottlenecks were fixed:

- **Off-screen painting.** `paint_document` now skips elements whose rotated
  world bounds, projected to screen space and expanded by a conservative
  margin, do not intersect the canvas. The margin covers rough-stroke jitter,
  arrowhead width, stroke width and text that can re-wrap taller than its
  stored height, so nothing visible is clipped. Unit tests cover an on-screen
  rectangle, a far rectangle, a tall rectangle visible only after rotation, an
  arrow whose head alone overlaps the canvas, and text near/past the margin.
- **Icon rasterization.** The icon still uses 4×4 supersampling on edges, but
  convex-corner checks fill uniform background/board pixels directly, mark
  tests are gated by a bounding box, distances compare squares, and per-segment
  boxes skip most of the 48-segment stroke walk. Output is visually unchanged
  (preview inspected) and the design/tests are unchanged.

`paint_order()` remains 11.3 µs per call at 5,000 elements, about 1% of the
frame, so it was left alone. Per-element culling runs before the rough-stroke
and tessellation work; egui's own tessellation of the surviving shapes is now
the largest frame component at 5,000 elements (0.35–0.63 ms).

### Verified commands and results (task 7.3)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | **270 passed, 0 failed** (152 app, 113 core, 5 compat); 1 ignored profiling harness |
| `cargo build --workspace` | success, no warnings |
| `timeout 8 ./target/debug/epsilon` | exit 124 (alive, no stderr) |
| `cargo test -p epsilon-app --release -- --ignored --nocapture profile_frame_costs` | numbers in the table above |
| `cargo build --release -p epsilon-app` + timed launches | window appeared after 75, 102 and 77 ms over three runs |

## Release packaging (task 7.4)

**Release profile.** The workspace keeps `[profile.release]` as `lto = "thin"`,
`codegen-units = 1` and `strip = true`; it was validated with
`cargo build --release --locked --workspace`. Panic strategy stays at the
default unwind so the existing panic hook can log; switching to abort was not
needed for size and would remove that final report.

| Build | Size |
| --- | --- |
| `target/debug/epsilon` | 268,959,640 bytes (~256 MiB, unstripped, debug info) |
| `target/release/epsilon` | 11,233,392 bytes (~10.7 MiB, stripped) |
| `dist/epsilon-0.1.0-linux-x86_64.tar.gz` | 4,808,287 bytes (~4.6 MiB) |

**Tarball layout.** `packaging/release.sh` reads the workspace version, runs
`cargo build --release --locked -p epsilon-app`, stages
`epsilon-<version>-linux-<arch>/` and writes the archive plus a SHA-256 file
into `dist/` (gitignored). The directory contains:

```
epsilon-0.1.0-linux-x86_64/
  epsilon              release binary (stripped)
  epsilon.desktop      launcher
  epsilon.svg          scalable icon
  epsilon-mime.xml     shared-mime-info definitions
  install.sh           installs the adjacent binary and assets
  LICENSE
  README.md
```

`install.sh` now detects its layout: when `epsilon.desktop` sits next to it (the
tarball), it installs the adjacent `epsilon` binary and assets; in a checkout it
uses `packaging/` and `target/release/epsilon` as before. `EPSILON_BIN` still
overrides the binary and `PREFIX` the install root.

**Runtime requirements.** `ldd target/release/epsilon` lists only `libc`,
`libm` and `libgcc_s`; OpenGL and the Wayland/X11 client libraries are loaded
at runtime by `glow`/`winit`, so the build is not fully static and must not be
described as such. The app needs an OpenGL 3.3+ driver and a Wayland or X11
session (`XDG_RUNTIME_DIR` on Wayland). Open/Save/Export dialogs go through the
XDG Desktop Portal (`rfd`'s default `xdg-portal` backend); without a running
portal those commands report an error toast and leave the document untouched.
Copy as Image prefers Wayland data-control and falls back to X11. UI fonts are
embedded, so no system font packages are required. Settings, logs and recovery
files live under `$XDG_DATA_HOME/epsilon`.

### Verified commands and results (task 7.4)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | **270 passed, 0 failed** (152 app, 113 core, 5 compat); 1 ignored profiling harness |
| `cargo build --workspace` | success, no warnings |
| `cargo build --release --locked --workspace` | success |
| `timeout 8 ./target/debug/epsilon` | exit 124 (alive, no stderr) |
| `HOME=<fresh> timeout 8 ./target/release/epsilon` | exit 124; session record written under the fresh `$XDG_DATA_HOME/epsilon/logs` |
| `./packaging/release.sh` | exit 0; tarball and `.sha256` written |
| `sha256sum -c epsilon-0.1.0-linux-x86_64.tar.gz.sha256` | OK |
| `tar -tzf …` | expected seven entries; `epsilon` and `install.sh` are executable |
| tarball binary from `/tmp` with fresh `HOME` | exit 124; session record written (no CWD dependency) |
| `PREFIX=<tmp> <tarball>/install.sh` | installed adjacent binary, desktop entry, icon and MIME files |
| `desktop-file-validate <tmp>/share/applications/epsilon.desktop` | exit 0, no diagnostics |
| MIME cache check | both `application/x-epsilon` and `application/vnd.excalidraw+json` map to `epsilon.desktop` |
| installed `epsilon scene.excalidraw` with fresh `HOME` | exit 124; log records the read, 4 imported elements and 1 skipped |

**Manual checks still required.** The “clean profile” above shares this host's
kernel, GPU, Mesa stack, desktop portal and Wayland compositor; it has no prior
Epsilon settings or data. A different distribution/architecture, a machine
without a portal, and an X11-only session were not exercised. See the manual
checklist below.

## Final verification checkpoint (task 7.5, in progress)

The full suite passes (270 tests, 1 ignored harness), and these focused
automated end-to-end tests pass together:
`every_drawing_tool_creation_is_undoable`,
`mixed_stage_three_edits_undo_and_redo_exactly`,
`save_and_open_round_trip_updates_dirty_state`,
`png_export_writes_a_decodable_image`,
`excalidraw_export_writes_a_readable_scene`,
`open_errors_leave_the_current_document_untouched`,
`failed_save_reports_the_error_and_keeps_the_document_dirty`,
`autosave_is_offered_and_recovered_after_a_crash`,
`keyboard_shortcuts_select_tools`, `edit_shortcuts_are_registered` and
`file_shortcuts_are_registered` (11/11). Together with the export, group,
order, clipboard, recovery and packaging checks elsewhere in this file, the
draw → edit → save → open → export → recover flow is covered without a window.
`project/RELEASE_NOTES_v0.1.0.md` holds the checklist and marks every item as
automated or manual.

**v0.1.0 is not tagged.** The remaining items need a human in a live session:
visual draw/edit inspection, native Open/Save/Export dialogs and overwrite
prompt, keyboard-only walkthrough, HiDPI, PNG/SVG visual comparison, Excalidraw
round trip in a browser, clipboard image into an external editor, a portal-less
session, an X11-only session, and launching from a real desktop install. Do not
tag until those are ticked or accepted as limitations in this file.

## Features implemented

- Embedded window icon generated from code (no external image at runtime).
- Reproducible release pipeline: `packaging/release.sh` writes a versioned,
  architecture-named tarball with checksums and the full install set.
- `install.sh` works both from a source checkout and from an unpacked tarball.
- Off-screen element culling in the canvas paint path with conservative margins
  for rough strokes, arrowheads and text.
- Linux packaging: desktop launcher, scalable icon, shared-mime-info types for
  `.epsilon` and `.excalidraw`, and a user-level `install.sh`.
- Startup file argument: `.excalidraw` imports, `.epsilon` opens natively, and
  failures go through the toast/log surface.
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
- Clipboard image export from File → Copy as Image: selection or whole canvas
  as a cropped 1× PNG with the document background, straight-alpha RGBA and
  `arboard` transfer (Wayland data-control preferred, X11 fallback), with toast
  failures and full-path log context.
- Unified error surface: file, clipboard and history failures become
  dismissible, auto-expiring toasts; duplicates are refreshed, the stack is
  bounded and the status bar only shows successful results.
- Rotating local diagnostic log (`<app data>/logs/epsilon.log`, 1 MiB, three
  siblings) with UTC timestamps, session/file-path records and a panic hook;
  no telemetry or network access.
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

## Manual acceptance checklist — clipboard image (task 6.5)

Automated tests cover the RGBA conversion and empty-scene failure paths. The
system clipboard and an external paste were not exercised, because that would
overwrite the user's clipboard:

- [ ] Launch `./target/debug/epsilon`, draw two separated shapes and select
  one, then choose File → Copy as Image and paste into an image editor; confirm
  only the selected shape appears, cropped, at 1× with the document background.
- [ ] Clear the selection (Esc), repeat Copy as Image and confirm the external
  paste contains the whole canvas.
- [ ] On niri, repeat after unsetting `DISPLAY` to confirm the Wayland
  data-control path works without XWayland.
- [ ] Copy a very large scene that exceeds the export limits and confirm the
  status bar reports the failure and the clipboard is unchanged.
- [ ] With an empty document, confirm the menu item is disabled and no
  clipboard data is written.

Status: **pending** — not performed in this environment.

## Manual acceptance checklist — error surface and log (task 7.1)

Automated tests cover toast dedupe, capacity, expiry, headless rendering, the
save-failure wiring, log records, timestamp formatting and rotation. The live
window and an actual panic were not exercised:

- [ ] Launch, draw a shape, then attempt a save whose parent path is a file and
  confirm a top-right Error toast names the full path, the × button dismisses
  it, and the status bar keeps only successful results.
- [ ] Trigger five different failures and confirm at most four toasts show;
  repeat one failure while its toast is visible and confirm it refreshes rather
  than stacking.
- [ ] Leave an error toast alone and confirm it disappears after about 12 s, and
  a warning after about 8 s.
- [ ] Copy unrelated text in another application and press Ctrl+V in Epsilon and
  confirm the "Clipboard has no Epsilon elements" warning toast.
- [ ] Inspect `<app data>/logs/epsilon.log` and confirm session-start, read/write
  path and failure records, with UTC timestamps and full paths on errors.
- [ ] Exceed 1 MiB of log entries and confirm `epsilon.log.1` … `.3` rotate and
  the current file starts fresh.
- [ ] Panic in a debug build and confirm stderr still shows the panic and the
  log gains an `[ERROR] panic` record with location.

Status: **pending** — not performed in this environment.

## Manual acceptance checklist — desktop integration (task 7.2)

Automated checks cover icon pixels, CLI parsing, startup load/import/failure,
desktop-entry validation and MIME-cache mapping in a temporary prefix. The real
session, icon rendering and file-manager click were not exercised:

- [ ] Run `cargo build --release && ./packaging/install.sh`, then confirm
  `epsilon` is on `PATH` and launches from the desktop application menu with the
  Epsilon icon visible (not a generic placeholder).
- [ ] Confirm the window/task-switcher icon matches `packaging/epsilon.svg` and
  no external image is needed at runtime.
- [ ] In the file manager, double-click a `.epsilon` file and confirm the window
  opens that document with its saved viewport and a clean dirty state.
- [ ] Double-click a `.excalidraw` scene and confirm it is imported as an unsaved
  document; skipping unsupported elements is reported as a warning toast.
- [ ] From a terminal, run `epsilon` with no argument and confirm the welcome
  panel still appears; run `epsilon /missing/file.epsilon` and confirm the error
  toast while the app stays usable.
- [ ] Confirm `.epsilon`/`.excalidraw` no longer associate with another installed
  application (or that changing the default back is possible).

Status: **pending** — the install was validated in a temporary prefix only.

## Manual acceptance checklist — release tarball (task 7.4)

Automated checks cover the release build, archive contents, checksum, launch
from a fresh `HOME`, installation into a temporary prefix, desktop-entry
validation and MIME mapping. These remain for a human on other systems:

- [ ] On a machine without a Rust toolchain, unpack the tarball, run
  `./install.sh`, and launch `epsilon` from the application menu.
- [ ] Repeat on a second distribution and (if available) a non-x86_64 host
  built from source with `./packaging/release.sh`.
- [ ] On a session without an XDG Desktop Portal, confirm File → Open still
  shows an error toast instead of failing silently or crashing.
- [ ] In an X11-only session, confirm the window starts and Copy as Image falls
  back to X11 clipboard data.
- [ ] Confirm the archive's `LICENSE` and `README.md` match the installed
  version and that `sha256sum -c` passes after copying the files.

Status: **pending** — only this host with a clean `HOME` was used.

## Known issues / limitations

- Edit contains Undo/Redo, Cut/Copy/Paste, Select All, Duplicate, Delete, Group,
  Ungroup, Order and the Arrange alignment/distribution submenu. File has
  New/Open/Open Recent/Save/Save As, Excalidraw Import/Export and Quit with the
  unsaved-changes guard. Excalidraw images/frames/bindings are not mapped yet.
- Stages 5 and 6 are complete: PNG and SVG pipelines, the unified export
  dialog and clipboard image copy. Manual acceptance remains for opening an
  Excalidraw export in the real app, visually checking PNG/SVG exports,
  clicking the export dialog/overwrite prompt/native dialogs, and pasting a
  copied image into an external application.
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
- Clipboard images are owned by the running Epsilon process (no daemonization),
  so the image may disappear from the clipboard after the app exits. Copy as
  Image has no keyboard shortcut and always uses 1× with the document
  background; the export dialog offers the other scale/background choices.
  Text clipboard interop is unchanged and separate from image copy.
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
- Error toasts and the log were verified by unit tests, a headless render and a
  native launch that wrote a session record; a human has not seen a toast, used
  its dismiss button, or forced a panic. Toasts auto-dismiss by design, so a
  missed error is also available in `<app data>/logs/epsilon.log`. There is no
  in-app log viewer or configurable log level/retention beyond the constants in
  `logging.rs`.
- Desktop integration was verified in a temporary `PREFIX`: entry validation,
  MIME cache and launch with a file argument. The icon was not viewed, and no
  real file-manager double-click or application-menu launch was performed. A
  `*.excalidraw` association overrides any existing default on install.
- Performance numbers come from one machine and synthetic rectangle grids; the
  5,000-element scene assumes a large board with most elements off screen. The
  ignored harness is not run by `cargo test --workspace`, so regressions are not
  caught automatically. Startup is measured externally via compositor window
  polling (~10 ms granularity) and includes process launch, GL/window setup and
  the icon; idle CPU varies with system load.
- The release binary is not statically linked: `ldd` shows `libc`, `libm` and
  `libgcc_s`, while GL and windowing libraries are loaded at runtime. Package
  docs say so; do not claim a fully static build.
- Release tarballs are built for the host architecture (`uname -m`); only
  x86_64 Linux was tested. A clean profile here means a fresh `HOME` on this
  machine, not a fresh OS install, and a missing desktop portal was not tried.
- The release profile keeps `panic = "unwind"`; binary size was 10.7 MiB
  stripped. No further size tuning (for example `panic = "abort"` or removing
  unused font data) was attempted.
- No CI; local verification only. No visual/screenshot review performed.
- Window/panel restore relies on eframe/egui and was not re-measured under niri.
- Project license: GPL-3.0 (`LICENSE`); manifests declare `GPL-3.0-only`.

## Resume instructions

1. Read `AGENTS.md`, `project/ROADMAP.md`, this file, `project/DECISIONS.md`.
2. `git status` must be clean; run `cargo test --workspace` for the baseline.
3. Task 7.5 is in progress: run the manual items in
   `project/RELEASE_NOTES_v0.1.0.md`, tick or document them, then tag `v0.1.0`.
4. Verify format, clippy, tests, build, and `timeout 8 ./target/debug/epsilon`.
5. Update this file, `ROADMAP.md`, `CHANGELOG.md`, then commit and push.

Never close windows through the compositor during verification; only terminate
PIDs started for the test.
