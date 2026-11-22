# Epsilon Roadmap

Legend: `[ ]` not started · `[~]` in progress · `[x]` completed · `[!]` blocked

Rules: one task per agent run. A task is complete only when its acceptance
criteria are verified and the context files are updated. Do not start a stage
before the previous stage's tasks are complete unless the roadmap says so.

---

## Stage 0 — Research and project foundation

### [x] 0.1 — Repository foundation, architecture and scaffold

- [x] Inspect repository, toolchain and environment.
- [x] Research Excalidraw's document format, MIT license and attribution terms.
- [x] Choose dependency set and record rationale in `DECISIONS.md`.
- [x] Create Cargo workspace: UI-independent `epsilon-core`, eframe app
      `epsilon-app` producing the `epsilon` binary.
- [x] Implement and test foundational geometry (`Point`, `Rect`).
- [x] Create `AGENTS.md` and all `project/` context files.
- [x] Verify `cargo fmt`, `cargo test --workspace`, `cargo build --workspace`,
      and native launch.

Acceptance: workspace builds cleanly; geometry tests pass; app launches a native
window; documentation allows a fresh session to resume.

---

## Stage 1 — Native desktop shell

### [x] 1.1 — Application shell layout

Build the region skeleton: menu bar, top toolbar, left tool panel, central canvas
region, right properties panel, status bar. Regions resize sensibly at 720×480
and 1920×1080.

Acceptance: launches natively; all six regions visible; window resize does not
panic or clip controls; `cargo test --workspace` and fmt pass.

Completed 2026-11-22: six regions implemented with compile-time size budgets,
headless layout tests at both sizes, native launch verified. Tool content,
menus, themes and live readouts remain in 1.2–1.6.

### [x] 1.2 — Theme system (dark / light)

Central palette/`egui::Visuals` setup; View menu toggle; all regions follow the
theme with legible contrast.

Acceptance: switching themes updates the whole shell immediately; no hard-coded
colors remain in shell widgets; manual launch check.

Completed 2026-11-22: semantic dark/light palettes in `theme.rs` applied via an
explicit `ThemePreference`; View → Dark mode toggle; contrast tests (7:1 primary,
4.5:1 weak); headless shell layout verified in both themes; native launch alive.

### [x] 1.3 — Tool model and left tool panel

`Tool` enum (select, hand, rectangle, ellipse, diamond, line, arrow, freehand,
text), active-tool state, icon/label buttons with tooltips and shortcut hints,
single-key shortcuts (V, H, R, O, D, L, A, P, T).

Acceptance: click or key selects exactly one tool; panel reflects state; unit
test for shortcut→tool mapping; no fake tool behavior beyond state selection.

Completed 2026-11-22: nine tools with painted vector icons, tooltips, selected
state and accessible names; plain-key shortcuts handled unless a widget owns the
keyboard; shortcut mapping and uniqueness tested. Drawing remains Stage 3.

### [x] 1.4 — Menus and top toolbar

Functional menu bar (File/Edit/View/Help) containing only actions that work at
this stage (theme toggle, About, Quit; File/Edit entries added when their stages
land). Top toolbar hosts app identity and the theme/About actions.

Acceptance: every visible menu item performs its action; no dead/placeholder
entries; keyboard navigation works.

Completed 2026-11-22: File → Quit, View → theme, Help → About; toolbar identity
plus theme/About controls; actions share helpers and are tested. The Edit menu is
intentionally omitted until Stage 4 provides working actions (no dead entries).
Keyboard behavior comes from standard egui widgets; it was not automated.

### [x] 1.5 — Right properties panel shell

Collapsible/resizable panel with section headers and a real empty state
("No selection") until Stage 3 binds element properties to it.

Acceptance: panel width persists in-session; layout unaffected at minimum window
size; empty state shown when no selection.

Completed 2026-11-22: properties panel uses `Panel::show_collapsible` with a
"Hide" control and a "Selection" collapsing section; fixed 200–400px resize
range retained; empty state shown when there is no selection; collapsed/expanded
paths render at the 720×480 minimum without panicking.

### [x] 1.6 — Status bar

Show active tool, placeholder-free readouts for pointer position and zoom
(`—` until the canvas exists), and app version.

Acceptance: active tool updates with selection; no crashes when resizing.

Completed 2026-11-22: status bar now shows the active tool name, `Pointer: —`,
`Zoom: —` and the app version. It follows the active tool because it reads
`EpsilonApp::tool` during layout; native launch and resize checks pass.

### [ ] 1.7 — Shortcut registry and help overlay

Central action/shortcut table used by panels and menus; help overlay (F1/?) lists
active shortcuts; duplicate bindings are rejected or tested.

Acceptance: unit test detects duplicate bindings; overlay opens/closes via
keyboard; shortcuts listed match implemented actions.

### [x] 1.8 — Window/settings persistence

Persist window size, theme, active panel widths and last tool across restarts
using eframe persistence (adds `persistence` feature + serde).

Acceptance: restart restores values; corrupt/missing storage falls back to
defaults without error.

Completed 2026-11-22 (out of order at the owner's request): eframe persistence
enabled; theme + tool saved/restored (round-trip, corrupt/missing fallback and
native seed-restart verified); window geometry and panel widths stored by
eframe/egui (verified in `app.ron`, restore not re-measured under the tiling
compositor).

### [ ] 1.9 — Shell polish and manual verification pass

Spacing, typography scale, separators, min sizes, HiDPI check, keyboard focus
behavior. Record a manual verification checklist result in `PROJECT_STATE.md`.

Acceptance: checklist executed at 100% and 150% scale; all items pass or are
documented as known issues.

---

## Stage 2 — Canvas engine

### [ ] 2.1 — Viewport model and transforms (core)

`Viewport { offset, zoom }` with world↔screen conversion, cursor-anchored zoom,
zoom clamping (0.1–30), fit-bounds. No egui types.

Acceptance: unit tests for round-trip conversion, anchor invariance under zoom,
clamping, and fit-to-rect.

### [ ] 2.2 — Canvas widget and coordinate pipeline

Central canvas widget allocating its rect, painting the scene background, and
mapping egui pointer positions through the viewport. Draw a world-origin marker
to prove alignment.

Acceptance: marker stays under the same world point while panning/zooming;
status bar coordinates match viewport math.

### [ ] 2.3 — Panning

Middle-button drag, space+drag, and hand tool drag; correct pointer capture and
cursor feedback; no pan while typing in text fields.

Acceptance: pan is 1:1 with pointer motion at any zoom; releasing outside the
window ends the drag cleanly.

### [ ] 2.4 — Zoom

Ctrl+wheel and pinch zoom anchored at the cursor, +/- buttons and shortcuts,
`0`/`Shift+0` reset/fit-to-content; wheel scrolls vertically; Shift+wheel
horizontally.

Acceptance: point under cursor is invariant across zoom steps (manual + unit
test); zoom limits enforced; UI reflects zoom percentage.

### [ ] 2.5 — Optional grid

Adaptive dot/line grid with configurable size, toggle and snapping-ready math;
grid stays crisp and seamless at all zoom levels.

Acceptance: toggle works from View menu; spacing adapts without visible drift;
toggle state in status/properties; unit tests for grid-step selection.

### [ ] 2.6 — Pointer input state machine

Explicit idle/panning/dragging state in the app layer; hover tracking; correct
handling of captures, modifiers, and focus; status bar live coordinates.

Acceptance: state machine unit-tested; input does not leak into panel widgets;
coordinates accurate at extreme zoom/pan.

---

## Stage 3 — Drawing engine

### [ ] 3.1 — Document model and element schema (core)

`Document` with schema version, canvas settings, ordered `Element`s; element base
fields (id, kind, x/y/width/height, angle, style, z-order, seed, version);
`ElementId` generation. Add `serde`/`serde_json` here.

Acceptance: round-trip serialization tests; element ordering semantics tested;
no egui dependency in core.

### [ ] 3.2 — Rendering pipeline and rectangle tool

Screen-space tessellation adapter in the app; drag-to-create rectangle; minimum
size handling; live preview while dragging.

Acceptance: rectangle persists in document, renders correctly at 10%–1000% zoom;
created bounds match drag in world space (screenshot-free numeric test where
possible).

### [ ] 3.3 — Ellipse tool

Drag-to-create ellipse with correct bounds and stroke/fill.

Acceptance: rendering matches bounds at all zooms; empty/zero-size drags discard.

### [ ] 3.4 — Diamond tool

Diamond rendered as polygon inscribed in element bounds.

Acceptance: geometry unit-tested; rendering consistent with bounds.

### [ ] 3.5 — Line tool

Two-point line with shift-constrain to 15° increments and multi-click polyline
only if scoped to a single segment for this task.

Acceptance: endpoints stored in world space, rotate/resize data model sound,
constraint math tested.

### [ ] 3.6 — Arrow tool

Line with arrowhead geometry; head avoids overdraw at short lengths; shift
constraint.

Acceptance: arrowhead polygon tests; rendering verified at multiple zooms.

### [ ] 3.7 — Freehand tool

Pointer-sampled polyline with pressure-agnostic smoothing and point reduction;
bounded memory for long strokes.

Acceptance: simplification unit tests; captured points survive pan/zoom during
drawing; stroke renders smoothly.

### [ ] 3.8 — Text tool

Click/drag to place text; multi-line editing overlay; font size, alignment,
color; empty text is discarded on commit.

Acceptance: text persists, edits commit on Esc/click-out, wrap width respected;
manual editing check.

### [ ] 3.9 — Selection and hit testing

Click-to-select topmost element, stroke/fill hit tolerance, rotated-element hit
tests, shift-click toggle, marquee selection.

Acceptance: hit-test unit tests for each shape incl. rotation; marquee selects
intersecting elements.

### [ ] 3.10 — Move and resize

Dragging selected elements, 8-handle resize (with aspect/shift constraint),
multi-element resize, Alt-from-center; handles rendered in screen space.

Acceptance: transform math unit-tested; no drift after repeated resizes; minimum
size enforced.

### [ ] 3.11 — Rotation

Rotation handle, angle-aware bounds/hit testing, shift snapping to 15°,
multi-element rotation around collective center.

Acceptance: rotation tests; handles remain usable for rotated selections.

### [ ] 3.12 — Styling system

Stroke/fill colors, stroke width, opacity, stroke style (solid/dashed/dotted),
fill style (solid/hatch/cross-hatch), roundness.

Acceptance: styles in document model with round-trip tests; rendering verified;
style changes apply to selection.

### [ ] 3.13 — Hand-drawn rendering

Deterministic rough-stroke rendering driven by element seed, with a clean
(architect-style) toggle.

Acceptance: same seed renders identically across frames/exports (test hashes or
point comparison); performance within budget on 500 elements.

### [ ] 3.14 — Properties panel binding

Right panel edits selected element properties (position, size, angle, style),
shows mixed values for multi-selection, commits on change.

Acceptance: edits are undoable once Stage 4 lands and update canvas live; panel
state consistent after selection changes.

---

## Stage 4 — Editing experience

### [ ] 4.1 — Command/history system (core)

Transaction-based history: commands with apply/revert, coalescing for drags,
bounded memory, no full-document clones.

Acceptance: unit tests for undo/redo ordering, branching after undo, and memory
bound; document equality checks after undo/redo cycles.

### [ ] 4.2 — Undo/redo integration

Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y, history-aware toolbar buttons, status feedback.

Acceptance: every Stage 3 edit is undoable; button enabled state matches history;
manual check across mixed operations.

### [ ] 4.3 — Clipboard: copy, cut, paste

Copy selection to internal clipboard JSON, paste with offset and fresh IDs,
system clipboard text interop where safe.

Acceptance: IDs regenerated (no collisions), z-order preserved, paste offset
tested; cut/copy/paste via shortcuts and menu.

### [ ] 4.4 — Delete, duplicate, select all, escape

Delete/Backspace, Ctrl+D duplicate, Ctrl+A select all, Esc clears selection and
returns to select tool.

Acceptance: shortcuts tested; duplicate offsets by grid step; no stale selection.

### [ ] 4.5 — Z-order commands

Bring to front/send to back, forward/backward, with menu/shortcut access.

Acceptance: ordering tests; selection preserved across reorder.

### [ ] 4.6 — Context menus

Right-click menu with applicable element/selection/canvas actions.

Acceptance: only valid actions shown; menu actions match shortcuts; no menu in
text-editing mode.

### [ ] 4.7 — Grouping

Group/ungroup with `groupIds`, grouped selection semantics, nested groups.

Acceptance: group operations undoable; hit test selects whole group; ungroup
preserves element order.

### [ ] 4.8 — Alignment

Align left/center/right/top/middle/bottom for 2+ elements.

Acceptance: pure alignment functions unit-tested; bounds correct for rotated
elements.

### [ ] 4.9 — Distribution

Distribute horizontally/vertically with equal gaps for 3+ elements.

Acceptance: distribution tests incl. already-even and overlapping cases.

---

## Stage 5 — Documents and persistence

### [ ] 5.1 — Native document format v1

Versioned JSON format (`.epsilon`): schema version, document, app state, unknown
fields preserved in `extras` maps.

Acceptance: round-trip tests incl. unknown-field preservation; clear error for
unsupported future versions.

### [ ] 5.2 — Save and open

Native dialogs (rfd), dirty tracking, atomic writes (temp file + rename), error
surfacing without data loss.

Acceptance: save/open round-trip on disk; read-only path and malformed file
produce user-visible errors; no partial overwrite on failure.

### [ ] 5.3 — New, Save As, close guard

New document, Save As, unsaved-changes prompt on close/new/open.

Acceptance: cancel paths preserve data; window close respects dirty state; manual
checks.

### [ ] 5.4 — Autosave and crash recovery

Periodic autosave to app data dir, crash recovery offer on launch, cleanup policy.

Acceptance: simulated kill restores scene; recovery can be discarded; tests for
file naming/retention.

### [ ] 5.5 — Recent documents

Recent file list in File menu and welcome state, persisted settings, missing-file
handling.

Acceptance: list deduplicates, caps length, handles missing files gracefully.

### [ ] 5.6 — Excalidraw import

Map Excalidraw elements/appState subset into the document model; preserve
unsupported fields for re-export; reject invalid JSON with clear messages.

Acceptance: fixture files import correctly (unit tests); unknown element types
skipped with warning, not crash; imported scene renders.

### [ ] 5.7 — Excalidraw export

Export scene as `.excalidraw` JSON with required fields and compatible element
properties.

Acceptance: exported file opens in Excalidraw (manual check); schema-required
fields covered by tests.

### [ ] 5.8 — Compatibility test suite

Fixture-based import/export/round-trip tests, malformed inputs, version handling.

Acceptance: suite passes in CI-local `cargo test`; fixtures stored under
`crates/epsilon-core/tests/fixtures`.

---

## Stage 6 — Export

### [ ] 6.1 — Raster export backend decision and spike

Evaluate offscreen egui/glow capture vs CPU rasterizer (e.g. tiny-skia); record
decision, dependency cost, text/rough-stroke fidelity.

Acceptance: decision recorded in `DECISIONS.md`; smallest viable spike renders one
rectangle + text to PNG.

### [ ] 6.2 — PNG export pipeline

World-bounds cropping, scale factor (1x/2x/4x/custom), background/transparent
option, deterministic output.

Acceptance: PNG dimensions and content bounds tested; export ignores UI panels;
large scenes bounded.

### [ ] 6.3 — SVG export

Vector output for all element types; text as text or outlined paths (decided in
6.1); stable element ordering.

Acceptance: valid SVG; visual parity check against canvas; geometry tests.

### [ ] 6.4 — Export UI

Export dialog: selection vs entire canvas, format, scale, filename; overwrite
handling.

Acceptance: exporting a selection omits other elements (bbox test); cancel is
safe; errors surfaced.

### [ ] 6.5 — Clipboard image export

Copy selection/canvas as PNG to system clipboard.

Acceptance: pasting into an external app works (manual); failure path reported.

---

## Stage 7 — Desktop readiness

### [ ] 7.1 — Error handling and logging pass

Unified user-facing error surface (non-blocking toasts/banner), persistent local
log file with rotation, no panic on recoverable errors.

Acceptance: forced failure scenarios show actionable messages; logs contain
context; no telemetry.

### [ ] 7.2 — Icon, desktop entry, file associations

Embedded window icon, `.desktop` launcher, `.epsilon`/`.excalidraw` associations,
CLI argument to open a file.

Acceptance: installed launcher starts app; opening a file from the file manager
loads it; icon embedded (no external asset at runtime).

### [ ] 7.3 — Performance profiling

Profile startup, idle CPU, pan/zoom frame time, 1k/5k-element scenes; fix
measured bottlenecks.

Acceptance: profiling numbers recorded before/after in `PROJECT_STATE.md`; no
optimization claimed without measurement.

### [ ] 7.4 — Release build and packaging docs

Release profile validation, binary size report, tarball layout, build-from-source
instructions, known platform requirements (GL, Wayland/X11).

Acceptance: `cargo build --release` verified; packaged binary runs on a clean
user profile; docs accurate.

### [ ] 7.5 — Final usability verification and v0.1.0

End-to-end whiteboard checklist (draw/edit/save/open/export), keyboard-only pass,
error recovery; tag v0.1.0 when all items pass.

Acceptance: checklist attached to release notes; no open blocker tasks.
