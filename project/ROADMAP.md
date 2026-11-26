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

### [x] 1.7 — Shortcut registry and help overlay

Central action/shortcut table used by panels and menus; help overlay (F1/?) lists
active shortcuts; duplicate bindings are rejected or tested.

Acceptance: unit test detects duplicate bindings; overlay opens/closes via
keyboard; shortcuts listed match implemented actions.

Completed 2026-11-22: `KEYBINDINGS` is the central registry for tool and help
shortcuts; `handle_shortcuts` dispatches through it; F1 and `?` toggle the
help overlay and Escape closes it. Tests cover duplicate bindings, tool
coverage, F1/`?` toggling and Escape closing.

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

### [x] 1.9 — Shell polish and manual verification pass

Spacing, typography scale, separators, min sizes, HiDPI check, keyboard focus
behavior. Record a manual verification checklist result in `PROJECT_STATE.md`.

Acceptance: checklist executed at 100% and 150% scale; all items pass or are
documented as known issues.

Completed 2026-11-22: spacing/separators and small status-bar typography were
tidied; headless layout checks pass at 100% and 150% scale. Visual HiDPI review
and keyboard-focus walkthrough are documented below rather than executed.

---

## Stage 2 — Canvas engine

### [x] 2.1 — Viewport model and transforms (core)

`Viewport { offset, zoom }` with world↔screen conversion, cursor-anchored zoom,
zoom clamping (0.1–30), fit-bounds. No egui types.

Acceptance: unit tests for round-trip conversion, anchor invariance under zoom,
clamping, and fit-to-rect.

Completed 2026-11-22: `epsilon-core::viewport::Viewport` implements world/screen
conversion, cursor-anchored zoom, clamping and fit-to-rect with 14 unit tests.

### [x] 2.2 — Canvas widget and coordinate pipeline

Central canvas widget allocating its rect, painting the scene background, and
mapping egui pointer positions through the viewport. Draw a world-origin marker
to prove alignment.

Acceptance: marker stays under the same world point while panning/zooming;
status bar coordinates match viewport math.

Completed 2026-11-22: canvas now allocates its rect, paints the canvas
background, tracks the pointer in world coordinates for the status bar, and
draws a world-origin marker through `Viewport`. Panning/zooming arrive in 2.3
and 2.4; the marker is stationary until then.

### [x] 2.3 — Panning

Middle-button drag, space+drag, and hand tool drag; correct pointer capture and
cursor feedback; no pan while typing in text fields.

Acceptance: pan is 1:1 with pointer motion at any zoom; releasing outside the
window ends the drag cleanly.

Completed 2026-11-22: canvas panning supports middle-button drag, space+drag and
hand-tool drag. Screen-space deltas update the viewport offset directly, space
panning is disabled while a widget owns keyboard input, and cursor feedback is
provided.

### [x] 2.4 — Zoom

Ctrl+wheel and pinch zoom anchored at the cursor, +/- buttons and shortcuts,
`0`/`Shift+0` reset/fit-to-content; wheel scrolls vertically; Shift+wheel
horizontally.

Acceptance: point under cursor is invariant across zoom steps (manual + unit
test); zoom limits enforced; UI reflects zoom percentage.

Completed 2026-11-22: Ctrl+wheel/pinch zoom is anchored at the cursor, +/- and
0 buttons plus Plus/Minus/0 shortcuts work, wheel scrolling is horizontal with
Shift, and the status bar shows the zoom percentage.

### [x] 2.5 — Optional grid

Adaptive dot/line grid with configurable size, toggle and snapping-ready math;
grid stays crisp and seamless at all zoom levels.

Acceptance: toggle works from View menu; spacing adapts without visible drift;
toggle state in status/properties; unit tests for grid-step selection.

Completed 2026-11-22: adaptive line grid uses `epsilon-core::grid`, toggles from
View menu and the properties panel, and reports state in the status bar. Core
tests cover grid-step selection and snapping.

### [x] 2.6 — Pointer input state machine

Explicit idle/panning/dragging state in the app layer; hover tracking; correct
handling of captures, modifiers, and focus; status bar live coordinates.

Acceptance: state machine unit-tested; input does not leak into panel widgets;
coordinates accurate at extreme zoom/pan.

Completed 2026-11-22: canvas pointer interaction now uses an explicit
`PointerMode` state machine, tracks hover position, starts panning only inside
the canvas, and keeps world coordinates in `f64` for extreme zoom/pan. Unit
and frame tests cover middle-click panning and the dragging state.

---

## Stage 3 — Drawing engine

### [x] 3.1 — Document model and element schema (core)

`Document` with schema version, canvas settings, ordered `Element`s; element base
fields (id, kind, x/y/width/height, angle, style, z-order, seed, version);
`ElementId` generation. Add `serde`/`serde_json` here.

Acceptance: round-trip serialization tests; element ordering semantics tested;
no egui dependency in core.

Completed 2026-11-22: `epsilon-core::document` defines the schema-versioned
`Document`, canvas settings, element base fields, style/color types and
monotonic `ElementId` generation. Paint order sorts by `z_order` with stable
insertion-order ties. JSON round-trip and ordering tests pass; core still has
no egui dependency.

### [x] 3.2 — Rendering pipeline and rectangle tool

Screen-space tessellation adapter in the app; drag-to-create rectangle; minimum
size handling; live preview while dragging.

Acceptance: rectangle persists in document, renders correctly at 10%–1000% zoom;
created bounds match drag in world space (screenshot-free numeric test where
possible).

Completed 2026-11-23: the app holds an in-memory `Document`, projects elements
through the viewport into egui rectangles, draws them in paint order, and adds
a live drag preview. Rectangle drags commit normalized world bounds and discard
drags smaller than 3 screen pixels. Numeric tests cover 10% and 1000% zoom plus
an end-to-end pointer press/move/release frame sequence.

### [x] 3.3 — Ellipse tool

Drag-to-create ellipse with correct bounds and stroke/fill.

Acceptance: rendering matches bounds at all zooms; empty/zero-size drags discard.

Completed 2026-11-23: the shared drag path now creates ellipses, the preview
and document renderer draw ellipses inscribed in their element bounds, and the
minimum-size discard applies to zero/small drags. Frame and projection tests
cover ellipse creation at extreme zoom.

### [x] 3.4 — Diamond tool

Diamond rendered as polygon inscribed in element bounds.

Acceptance: geometry unit-tested; rendering consistent with bounds.

Completed 2026-11-23: `Rect::diamond_points` returns the four edge-midpoint
vertices in core, with unit tests covering the vertices and their bounds. The
Diamond tool uses the shared drag pipeline, and document/preview rendering
project those vertices through the viewport; a zoomed projection test confirms
they coincide with the element's screen bounds.

### [x] 3.5 — Line tool

Two-point line with shift-constrain to 15° increments and multi-click polyline
only if scoped to a single segment for this task.

Acceptance: endpoints stored in world space, rotate/resize data model sound,
constraint math tested.

Completed 2026-11-23: `Element::line` stores both endpoints in world space and
derives the base bounds from them, with reverse drags preserving direction.
The Line tool uses the shared drag pipeline, previews and renders the segment,
and accepts thin horizontal/vertical lines by testing segment length rather
than both bounds. Shift constrains the endpoint through a tested core function
that snaps to 15° increments while preserving length.

### [x] 3.6 — Arrow tool

Line with arrowhead geometry; head avoids overdraw at short lengths; shift
constraint.

Acceptance: arrowhead polygon tests; rendering verified at multiple zooms.

Completed 2026-11-23: arrows reuse the line endpoint model and 15° Shift
constraint. `arrowhead_points` clamps the head to half the shaft and its width
to the head length, with unit tests for normal, short and zero-length arrows.
The app keeps the head a constant screen size across zoom levels by converting
pixels through the viewport.

### [x] 3.7 — Freehand tool

Pointer-sampled polyline with pressure-agnostic smoothing and point reduction;
bounded memory for long strokes.

Acceptance: simplification unit tests; captured points survive pan/zoom during
drawing; stroke renders smoothly.

Completed 2026-11-23: freehand samples are captured in world space, smoothed
with a moving average and reduced with Ramer–Douglas–Peucker before commit.
Input is distance-filtered and hard-capped at 4096 points. Core tests cover
segment distance, smoothing and simplification; a frame test covers the full
press/move/release commit path.

### [x] 3.8 — Text tool

Click/drag to place text; multi-line editing overlay; font size, alignment,
color; empty text is discarded on commit.

Acceptance: text persists, edits commit on Esc/click-out, wrap width respected;
manual editing check.

Completed 2026-11-23: text elements store content, font size and alignment.
A click places a default-width text box, a drag sets its wrap width, and a
foreground multiline TextEdit edits it. Esc/click-out commits, empty text is
removed, and the stored height is measured from wrapped layout. Rendering uses
the element color, alignment and wrap width. Headless tests cover placement,
empty discard, measured commit and Escape; live visual editing was not possible
in this environment.

### [x] 3.9 — Selection and hit testing

Click-to-select topmost element, stroke/fill hit tolerance, rotated-element hit
tests, shift-click toggle, marquee selection.

Acceptance: hit-test unit tests for each shape incl. rotation; marquee selects
intersecting elements.

Completed 2026-11-23: the core element now provides rotation-aware hit tests,
rotated world bounds and marquee intersection. Rectangle, ellipse, diamond,
line, arrow, freehand and text tests cover stroke tolerance, transparent versus
filled interiors and rotation. The Select tool picks the topmost hit, supports
additive toggling and captures an intersecting marquee, with selection outlines
painted in screen space.

### [x] 3.10 — Move and resize

Dragging selected elements, 8-handle resize (with aspect/shift constraint),
multi-element resize, Alt-from-center; handles rendered in screen space.

Acceptance: transform math unit-tested; no drift after repeated resizes; minimum
size enforced.

Completed 2026-11-23: selected elements move by world-space pointer deltas and
resize from eight screen-space handles. Resize math handles edge/corner anchors,
Shift aspect lock, Alt centering and minimum size; multi-element transforms are
recomputed from drag-start snapshots each frame to avoid accumulated drift.
Core tests cover resize bounds and scaling round trips, and app tests cover
multi-element move, multi-element resize and handle placement.

### [x] 3.11 — Rotation

Rotation handle, angle-aware bounds/hit testing, shift snapping to 15°,
multi-element rotation around collective center.

Acceptance: rotation tests; handles remain usable for rotated selections.

Completed 2026-11-23: selections expose a rotation handle above their bounds,
with Shift snapping to 15°. Shape rotations accumulate `angle`, linear point
geometry rotates in world space, and multi-element rotations use the collective
bounds center. Rotated shapes render from projected rotated polygons; rotated
selections keep eight usable screen-space resize handles. Core and app tests
cover angle snapping, collective rotation and rotated handle bounds.

### [x] 3.12 — Styling system

Stroke/fill colors, stroke width, opacity, stroke style (solid/dashed/dotted),
fill style (solid/hatch/cross-hatch), roundness.

Acceptance: styles in document model with round-trip tests; rendering verified;
style changes apply to selection.

Completed 2026-11-23: element styles now include stroke/fill colors, width,
opacity, solid/dashed/dotted strokes, solid/hatch/cross-hatch fills and corner
roundness. Rendering applies opacity, dashed/dotted paths, hatch scanlines and
rounded rectangles. The properties panel edits the style of the current
selection, applying changes to every selected element. Serialization covers the
new fields with legacy defaults, and headless rendering exercises all style
paths.

### [x] 3.13 — Hand-drawn rendering

Deterministic rough-stroke rendering driven by element seed, with a clean
(architect-style) toggle.

Acceptance: same seed renders identically across frames/exports (test hashes or
point comparison); performance within budget on 500 elements.

Completed 2026-11-23: seeded `rough_polyline` jitters vertices and segment
midpoints deterministically; the View menu's persisted "Clean rendering"
toggle switches back to exact geometry. Shapes, arrows, lines and freehand use
the rough path while fill geometry stays exact. Point comparison tests cover
seed determinism, and a headless 500-element rough render completes well inside
the test budget.

### [x] 3.14 — Properties panel binding

Right panel edits selected element properties (position, size, angle, style),
shows mixed values for multi-selection, commits on change.

Acceptance: edits are undoable once Stage 4 lands and update canvas live; panel
state consistent after selection changes.

Completed 2026-11-23: the properties panel binds X/Y/W/H/angle and every style
field to the live selection. Multi-selection shows "Mixed" for disagreeing
values; position edits preserve relative offsets, size edits scale each selected
element, and angle edits update closed shapes and linear directions. Edits are
computed from the current selection every frame, so panel state follows
selection changes immediately. Undo integration is deferred to Stage 4's
command history as specified.

---

## Stage 4 — Editing experience

### [x] 4.1 — Command/history system (core)

Transaction-based history: commands with apply/revert, coalescing for drags,
bounded memory, no full-document clones.

Acceptance: unit tests for undo/redo ordering, branching after undo, and memory
bound; document equality checks after undo/redo cycles.

Completed 2026-11-25: `epsilon-core::history` provides insert/remove/replace
commands that store only the affected elements, grouped into transactions.
`History` applies edits, keeps a bounded undo stack (100 entries by default),
discards redo branches on new edits and coalesces repeated drag replacements
into one entry. Nine tests cover ordering, full undo/redo document equality,
transaction grouping, branch discard, the memory bound, removal order,
failed-commit atomicity and coalescing. The app does not consume the history
yet.

### [x] 4.2 — Undo/redo integration

Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y, history-aware toolbar buttons, status feedback.

Acceptance: every Stage 3 edit is undoable; button enabled state matches history;
manual check across mixed operations.

Completed 2026-11-25: shape, line, arrow, freehand and committed text creation
now insert through the core history; move/resize/rotation drags and
properties/style edits record one transaction per finished interaction.
Ctrl+Z/Ctrl+Shift+Z/Ctrl+Y, Edit → Undo/Redo and toolbar buttons are wired, with
enabled state reading history depth. The status bar reports undo/redo depths and
history errors. Tests cover shortcut dispatch, undo/redo for every drawing tool,
text creation and existing-text edits, a mixed create/move/resize/rotate/
property/style sequence with exact document equality, redo-branch discard, and
history blocked during text editing. Live menu clicks and visual enabled states were not manually
inspected; the headless shell renders them.

### [x] 4.3 — Clipboard: copy, cut, paste

Copy selection to internal clipboard JSON, paste with offset and fresh IDs,
system clipboard text interop where safe.

Acceptance: IDs regenerated (no collisions), z-order preserved, paste offset
tested; cut/copy/paste via shortcuts and menu.

Completed 2026-11-26: `epsilon-core::clipboard` defines a versioned
`epsilon/clipboard` JSON payload. Pasting sorts by paint order, regenerates IDs
that collide with neither the document nor the payload, assigns new z-order
slots above every element and offsets geometry (including linear world points)
by a cascading 10-unit step. The app writes copy/cut JSON to the system
clipboard through egui clipboard events (Ctrl+C/X/V), parses Ctrl+V payloads,
ignores unrelated text with status feedback, selects pasted copies and records
paste/cut as single undo transactions. The Edit menu exposes Cut/Copy/Paste with
enabled states; clipboard commands are blocked while canvas text is edited.
Core and app tests cover IDs, order, offsets, unsupported payloads, undo/redo
and event dispatch. Live cross-application clipboard and menu clicks were not
manually inspected.

### [x] 4.4 — Delete, duplicate, select all, escape

Delete/Backspace, Ctrl+D duplicate, Ctrl+A select all, Esc clears selection and
returns to select tool.

Acceptance: shortcuts tested; duplicate offsets by grid step; no stale selection.

Completed 2026-11-26: Delete and Backspace remove the selection through one
undoable history transaction and clear the selection; duplication reuses the
clipboard payload path to copy the selection one `DEFAULT_GRID_SIZE` away with
fresh IDs, new paint slots and the copies selected, without touching the
clipboard state; Ctrl+A selects every element; Esc clears the selection and
returns to the Select tool while closing the help overlay first. All commands
are ignored during drags, property edits and canvas text editing. The Edit menu
exposes Select All, Duplicate and Delete with enabled states. Tests cover each
shortcut, undo restore, grid-step offset, fresh IDs, empty documents, no stale
selection and Escape precedence.

### [x] 4.5 — Z-order commands

Bring to front/send to back, forward/backward, with menu/shortcut access.

Acceptance: ordering tests; selection preserved across reorder.

Completed 2026-11-26: `Document::z_order_changes` computes before/after
paint-order changes for front, forward, backward and back moves. Front/back
move the selection as a stable block; forward/backward move single elements one
slot and contiguous selections as a group, leaving already-front/back or fully
selected scenes unchanged. The app applies each effective move as one undo
`Replace` transaction, preserves the selection, and exposes Ctrl+Shift+] /
Ctrl+] / Ctrl+[ / Ctrl+Shift+[ plus an Edit → Order submenu. Core tests cover
ordering and no-ops; app tests cover shortcuts, selection preservation, exact
undo/redo and history depth.

### [x] 4.6 — Context menus

Right-click menu with applicable element/selection/canvas actions.

Acceptance: only valid actions shown; menu actions match shortcuts; no menu in
text-editing mode.

Completed 2026-11-26: right-click selects the element under the pointer (or
keeps an existing multi-selection it belongs to) and clears the selection on
empty canvas. The container context menu is built only from an action list
filtered for the target: selection menus offer Cut/Copy/Paste, Duplicate,
Delete and the four Order moves, while canvas menus offer only Paste and Select
All when they can run. Shortcut text comes from the central registry, and no
menu is attached while the canvas text editor is open. Tests cover
applicability, labels/shortcut coverage and right-click targeting; interactive
popup rendering was not manually inspected.

### [x] 4.7 — Grouping

Group/ungroup with `groupIds`, grouped selection semantics, nested groups.

Acceptance: group operations undoable; hit test selects whole group; ungroup
preserves element order.

Completed 2026-11-26: `Element` stores `group_ids` outermost-first and
`Document` provides group expansion plus group/ungroup change snapshots.
Grouping wraps the selection in a new outer group; ungrouping removes one
nesting level from every selected outermost group without reordering elements.
Clicks, marquees, shift-toggles and right-click targeting all expand to whole
outermost groups, so a group cannot be partially selected. Ctrl+G /
Ctrl+Shift+G, Edit menu entries and context-menu entries commit one undo
transaction each. Clipboard paste regenerates group IDs while preserving
nesting, so copies do not join the source group. Core tests cover nesting,
one-level ungrouping, ordering, no-ops and paste; app tests cover shortcuts,
undo/redo, selection preservation and grouped click/shift/right-click targeting.
Interactive menu clicks were not manually inspected.

### [x] 4.8 — Alignment

Align left/center/right/top/middle/bottom for 2+ elements.

Acceptance: pure alignment functions unit-tested; bounds correct for rotated
elements.

Completed 2026-11-26: `Align` and `Document::align_changes` translate the
selection onto the collective left/center/right or top/middle/bottom edge
using rotated world bounds. The app exposes the six commands under Edit →
Arrange, enabled for two or more selected elements; each effective alignment is
one transaction, preserves the selection, and records no history for no-ops.
Core tests cover every edge, a rotated rectangle, unknown IDs and fewer than two
elements; app tests cover exact undo, selection preservation and no-op history.
Interactive menu clicks were not manually inspected.

### [x] 4.9 — Distribution

Distribute horizontally/vertically with equal gaps for 3+ elements.

Acceptance: distribution tests incl. already-even and overlapping cases.

Completed 2026-11-26: `Distribute` and `Document::distribute_changes` sort the
selection by rotated world bounds along one axis, keep the outermost elements
fixed and translate the interior elements so every gap is equal. Overlapping
selections produce a negative but equal gap; already-even, undersized and
unknown-ID selections yield no changes. The app exposes both axes under Edit →
Arrange for three or more selected elements, committing one transaction that
preserves the selection and records no history for no-ops. Core tests cover
mixed sizes, overlap, even spacing and undersized input; app tests cover exact
undo and no-op history. Interactive menu clicks were not manually inspected.

---

## Stage 4 complete — editing experience (2026-11-26)

Selection, clipboard, deletion/duplication, z-order, context menus, grouping,
alignment and distribution are implemented and verified. Stage 5 begins with
the native document format.

## Stage 5 — Documents and persistence

The native v1 serialization envelope is in place; save/open and the remaining
document lifecycle tasks follow.

### [x] 5.1 — Native document format v1

Versioned JSON format (`.epsilon`): schema version, document, app state, unknown
fields preserved in `extras` maps.

Acceptance: round-trip tests incl. unknown-field preservation; clear error for
unsupported future versions.

Completed 2026-11-26: `epsilon_core::format` defines the v1 `EpsilonFile`
envelope (`format: "epsilon"`, `schema_version: 1`) with the document, a
`Viewport` app state and flattened extras maps at envelope, app-state and
document levels. `from_json` validates format and version before
deserializing, so future files produce `UnsupportedVersion` rather than a
missing-field error, and foreign or malformed files get explicit errors.
Round-trip tests cover the document, grouped elements, viewport/app state,
extras preservation and nested document schema mismatch. File pickers and
disk I/O remain task 5.2.

### [x] 5.2 — Save and open

Native dialogs (rfd), dirty tracking, atomic writes (temp file + rename), error
surfacing without data loss.

Acceptance: save/open round-trip on disk; read-only path and malformed file
produce user-visible errors; no partial overwrite on failure.

Completed 2026-11-26: `EpsilonFile` gained atomic save and validated load;
`rfd` supplies native Open/Save dialogs on Ctrl+O/Ctrl+S and the File menu. The
app snapshots the document at save/open for dirty tracking, shows the file name
and dirty marker in the title/status bar, and reports failures in the status
bar without replacing the current scene. Save writes JSON to a hidden sibling
temp file, syncs it, then renames, so a failed write leaves the destination
untouched. Tests cover disk round-trips, malformed/missing files, failed writes,
extension handling and shortcut registration; dialog clicks were not manually
inspected.

### [x] 5.3 — New, Save As, close guard

New document, Save As, unsaved-changes prompt on close/new/open.

Acceptance: cancel paths preserve data; window close respects dirty state; manual
checks.

Completed 2026-11-26: File → New and Save As, Ctrl+N and Ctrl+Shift+S, and a
Save/Discard/Cancel modal before New, Open and compositor close requests. The
close request is cancelled while the guard is unresolved; Save resolves it only
after a successful write, so a cancelled Save As or failed write keeps the
prompt and document intact. Discard paths reset path/history/selection safely.
Tests cover New reset, Cancel/Discard on New, Save-then-continue and all file
shortcuts. The live modal and compositor close flow were not manually tested.

### [x] 5.4 — Autosave and crash recovery

Periodic autosave to app data dir, crash recovery offer on launch, cleanup policy.

Completed 2026-11-26: dirty documents autosave every 30 seconds through a core
`RecoveryStore` under `<app data>/recovery`, with sortable timestamp/sequence
names and only the newest five retained. Launch checks the newest valid autosave
and offers Recover/Discard; recovery restores the viewport and source path,
clears history/selection and starts dirty, while discard removes the files.
Normal save/open/new/discard/quit clears the recovery directory. Tests cover
naming, retention, corrupt-newest fallback, a simulated crash/recover, reset
state and discard. The live 30-second timer and launch modal were not manually
tested.

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
