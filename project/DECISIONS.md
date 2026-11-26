# Technical Decisions

Newest decisions are appended. Each entry: context, decision, consequences.

---

## D-001 — Two-crate workspace: `epsilon-core` + `epsilon-app` (2026-11-22)

**Context.** The document model and canvas engine must stay independent of egui.

**Decision.** Cargo workspace with `crates/epsilon-core` (pure Rust, zero UI deps,
`unsafe_code = "forbid"`) and `crates/epsilon-app` (eframe shell, produces the
`epsilon` binary). Core owns geometry, document model, viewport math, history,
serialization and export; the app owns windows, widgets, input translation and
painting.

**Consequences.** The dependency rule is enforced by Cargo rather than by
convention: `epsilon-core` simply cannot use egui. Slight manifest overhead;
core APIs must be designed without widget-oriented shortcuts. `epsilon-app` will
depend on core starting with task 1.1 (first shared state), at which point the
path dependency is added back to `epsilon-app/Cargo.toml` and
`[workspace.dependencies]` to avoid Cargo's unused-dependency warning.

---

## D-002 — egui/eframe 0.37 with the `glow` backend (2026-11-22)

**Context.** eframe 0.37's default feature set pulls wgpu plus its shader stack;
Epsilon targets lightweight Linux desktops, including software rasterizers
(llvmpipe) on machines without usable GPU drivers.

**Decision.** Use `eframe 0.37` with `default-features = false` and features
`default_fonts`, `glow`, `wayland`, `x11`. Pick the API year deliberately: 0.37
renamed the app entry point to `App::ui(&mut Ui, &mut Frame)`, and panels
(`CentralPanel`, `SidePanel`, …) take `&mut Ui` instead of `&Context`.

**Consequences.** Smaller dependency graph and broad GL compatibility. wgpu-only
improvements are unavailable; glow is mature enough for 2D painting. Clipboard
and accessibility features are added only when their stages need them.

---

## D-003 — World coordinates are `f64`, Y-down, 1 unit = 1 logical px at 100% zoom (2026-11-22)

**Context.** An infinite canvas with deep zoom needs more precision than `f32`
at large coordinates; egui paints in `f32` screen space.

**Decision.** Core geometry uses `f64`, Y axis points down (matching Excalidraw
and screen conventions), and world units equal logical pixels at zoom 1.0.
Conversion to `f32` happens only in the app's rendering boundary.

**Consequences.** Stable hit tests and handles far from the origin; a single
explicit conversion point to audit. Angles are radians, clockwise-positive under
Y-down.

---

## D-004 — Geometry primitives use a single `Point` type (2026-11-22)

**Context.** A full `Point`/`Vector` split (as in glam) prevents unit errors but
adds conversions.

**Decision.** One `Point { x: f64, y: f64 }` used as position and displacement,
with arithmetic operator impls, [`Point::normalized`] returning `Option`, and
`Rect` normalized to min/max corners. `Rect::from_corners`/`from_xywh` normalize
negative input instead of rejecting it.

**Consequences.** Small, testable surface; no accidental mixing bugs mitigated by
the type system. Revisit only if vector-heavy geometry (e.g. curve math) makes
the distinction valuable.

---

## D-005 — GPL-3.0 project license; Excalidraw is a reference only (2026-11-22)

**Context.** Epsilon is "inspired by Excalidraw". The repository's initial
commit already includes `LICENSE` (GNU GPL v3), so the project is
GPL-3.0-only. Excalidraw is MIT-licensed: "Copyright (c) 2020 Excalidraw", with
the standard MIT permission and the requirement to include the copyright and
permission notice in copies or substantial portions of the software.

**Decision.** Declare `license = "GPL-3.0-only"` in the workspace manifests. Do
not copy Excalidraw source; implement from the documented formats and observable
behavior. No Excalidraw code is present, so no notice is required today. If any
MIT-licensed code is ever incorporated, its copyright/permission notice must be
included and the incorporation recorded here (MIT is compatible with GPLv3, but
the resulting distribution stays GPLv3).

**Consequences.** Dependencies must be GPLv3-compatible (egui/eframe are
MIT/Apache-2.0, which is compatible). Format compatibility work is original
code; import/export must be validated against fixtures (Stage 5), not assumed.

---

## D-006 — Dependency policy: just-in-time, workspace-pinned (2026-11-22)

**Context.** "Lightweight" is a product requirement, and cargo's unused-dependency
lints surface speculative deps immediately.

**Decision.** Add runtime dependencies only in the task that consumes them,
declare them in `[workspace.dependencies]`, and record rationale here. Current
runtime deps: `egui`, `eframe` only. Anticipated, not yet added:
`serde`/`serde_json` (document format, task 3.1), `rfd` (native dialogs, 5.2),
a CPU rasterizer such as `tiny-skia` if offscreen egui capture proves
insufficient for PNG export (spike 6.1). No UI/web frameworks, no async runtime
without a demonstrated need.

**Consequences.** Lean builds and explicit audit trail. Some manifests change
more often; that is accepted.

---

## D-007 — Document format strategy (2026-11-22)

**Context.** Epsilon needs a durable native format and Excalidraw interop.

**Decision.** Native format is versioned JSON (`.epsilon`, introduced in 5.1)
holding `schema_version`, document state, and UI/app state. Unknown fields are
preserved in explicit `extras` maps rather than discarded. Excalidraw
import/export (5.6–5.8) maps through the same core model and preserves
unsupported fields on round-trip where practical.

**Research: Excalidraw `.excalidraw` envelope.** JSON object with `type:
"excalidraw"`, numeric `version` (currently 2), `source` (e.g.
`https://excalidraw.com`), `elements[]`, `appState{}`, and `files{}` (image data
keyed by file id, each with `mimeType`, `id`, `dataURL`, `created`,
`lastRetrieved`). The clipboard variant uses `type: "excalidraw/clipboard"` and
omits the editor envelope.

**Research: element base fields** (verified against
`packages/element/src/types.ts`): `id`, `type`, `x`, `y`, `width`, `height`,
`angle` (radians), `strokeColor`, `backgroundColor`, `fillStyle` (`hachure`,
`cross-hatch`, `solid`, `zigzag`), `strokeWidth`, `strokeStyle` (`solid`,
`dashed`, `dotted`), `roundness`, `roughness`, `opacity`, `seed`, `version`,
`versionNonce`, `index` (fractional z-order key), `isDeleted`, `groupIds`,
`frameId`, `boundElements`, `updated`, `created` (ms epoch or null), `link`,
`locked`, optional `customData`. Type-specific: linear elements add points,
bindings and arrowheads; text adds `text`, `fontSize`, `fontFamily`,
`textAlign`, `verticalAlign`, `containerId`, `lineHeight`, `autoResize`.

**Consequences.** Element IDs and z-order must be explicit (`Vec` order plus
stable ids) so import/export never depends on fractional indices. Epsilon will
implement its own subset first and reject unsupported types with a warning
instead of crashing.

---

## D-008 — Rendering is a projection of structured data (2026-11-22)

**Context.** Pixel-only canvases make editing, undo, export and interop
impossible.

**Decision.** Core stores elements as structured geometry + style. The app
tessellates/paints them each frame in screen space using the viewport transform;
no persistent pixel buffers are the source of truth. Rough/deterministic styling
will be seeded per element (Stage 3), so repeated frames and exports agree.

**Consequences.** Undo, SVG export and Excalidraw interop become tractable;
large scenes need culling/batching later (Stage 7 profiling), not now.

---

## D-009 — Undo strategy: commands with inverse operations (planned, Stage 4)

**Context.** Full document clones per change are simple but wasteful as scenes
grow.

**Decision (planned).** `epsilon-core` owns a transaction history where each
command carries enough data to apply and revert, coalescing continuous drags
into one history entry. The app submits transactions; core never asks the UI for
state.

**Consequences.** Commands must be tested for round-trip equality. The final data
structures are recorded in D-037 (task 4.1).

---

## D-010 — Shell layout budgets are compile-time checked and headlessly tested (2026-11-22)

**Context.** Task 1.1 must keep six regions usable at the 720×480 minimum window,
with a user-resizable properties panel, and must not regress silently as panel
sizes change.

**Decision.** All shell dimensions live in `crates/epsilon-app/src/layout.rs`.
Compile-time `const` assertions tie the minimum window size to the tool rail,
maximum properties width and canvas minimums. The properties panel's maximum
width is computed from the space actually left over, so a window manager forcing
a smaller window cannot starve the canvas. Layout is regression-tested headlessly
by running one real egui frame with `Context::run_ui` at `RawInput::screen_rect`
720×480 and 1920×1080 and asserting the canvas rect stays within the chrome.
No browser or screenshot tooling is involved.

**Consequences.** Changing the minimum window size or panel budget requires
updating `layout.rs`; the compiler rejects inconsistent budgets. Headless UI
layout tests with real egui fonts are available for later shell tasks, while
purely visual qualities still need a manual native check.

---

## D-011 — Central semantic palette with explicit theme preference (2026-11-22)

**Context.** Stage 1 needs dark and light themes, switchable at runtime, with
legible contrast and no per-widget color decisions scattered through the UI.
Eframe/egui can follow the OS theme, which would fight an in-app toggle.

**Decision.** `crates/epsilon-app/src/theme.rs` owns two `Palette` values with
identical semantic slots (canvas, panel, window, border, faint, widget states,
text, weak text, accent). `Theme::visuals()` builds `egui::Visuals` from them.
`theme::apply()` sets an explicit `ThemePreference` (never `System`) and then
the palette, so OS theme changes cannot override the user's choice. The shell
reads colors only through `Visuals`; the canvas uses `extreme_bg_color`. Default
theme is dark. Contrast is enforced by tests: primary text ≥ 7:1 and weak text
≥ 4.5:1 against both panel and canvas (WCAG AA/AAA for normal text).

**Consequences.** New widgets automatically match the theme; adding a third
theme means adding one `Palette` value. The theme is not persisted yet (task 1.8),
so every launch starts dark. Alpha-blended accent colours were avoided so the
contrast tests stay meaningful.

---

## D-012 — Painted tool icons; tools are UI state only until Stage 3 (2026-11-22)

**Context.** The tool palette needs recognizable icons without shipping image
assets, and the bundled fonts cannot be assumed to cover shape glyphs. Drawing
behavior belongs to Stage 3, so tool selection must not imply working tools.

**Decision.** `tools.rs` defines `Tool::{Select, Hand, Rectangle, Ellipse,
Diamond, Line, Arrow, Freehand, Text}` with shortcut letters V/H/R/O/D/L/A/P/T.
Icons are drawn as 20×20 vector paths with `egui::Painter` from the active
theme's foreground color, so there is no icon-font or image-asset dependency and
no glyph-coverage risk. Buttons are 40×34, which fits all nine above the canvas
at the 720×480 minimum window. Shortcuts fire only for unmodified key presses and
are skipped while `Context::egui_wants_keyboard_input()` is true (text editing).
Selecting a tool only updates `EpsilonApp::tool`; Stage 3 will consume it.

**Consequences.** Adding a tool requires an enum variant, a shortcut, an icon
arm and tests; no external assets. Painted icons have no automated visual
regression test — layout and launch checks only — until manual review.

---

## D-013 — Settings live in eframe persistence; fixed properties-panel range (2026-11-22)

**Context.** Theme, tool, window geometry and panel widths should survive a
restart without adding a config-file abstraction. The earlier properties-panel
width clamp (D-010) derived its maximum from the current window width, which can
shrink a saved width during transient startup frames.

**Decision.** Enable eframe's `persistence` feature. App-owned settings
(`theme`, `tool`) are a single `PersistedSettings` struct with `#[serde(default)]`
stored under the stable key `epsilon.settings` via `eframe::get_value` /
`eframe::set_value`; missing or corrupt values fall back to defaults. Window
geometry uses eframe's built-in `persist_window`, and resizable panel sizes use
egui memory (eframe persists it; `persist_egui_memory` is asserted in a test).
The properties panel uses a fixed 200–400px range instead of a dynamic clamp;
the compile-time layout budget already reserves a 240px canvas at the 720px
minimum window. Documents never live in the settings store — they get real files
in Stage 5.

**Consequences.** Minimal code and no new dependency beyond `serde`/eframe's
persistence feature. Storage keys and serde defaults must stay backward
compatible; incompatible schema changes need a new key or migration. Window and
panel restoration is framework behavior, so it is verified by inspecting the
`app.ron` payload rather than re-measuring a tiled native window.

---

## D-014 — Menus expose only working actions (2026-11-22)

**Context.** Task 1.4 asks for File/Edit/View/Help menus, but acceptance also
forbids dead or placeholder entries. At Stage 1, File has nothing beyond Quit and
Edit has no working commands until Stage 4.

**Decision.** The menu bar shows File (Quit), View (theme) and Help (About). The
Edit menu is omitted until undo/clipboard actions exist; File gains New/Open/Save
in Stage 5. Actions live in small shared helpers (`toggle_theme`, `open_about`,
`request_quit`) used by both menus and toolbar, so each visible entry has a
tested implementation. About is available from Help and the toolbar; the theme
toggle appears in View and the toolbar.

**Consequences.** Menus grow stage by stage instead of showing disabled or fake
commands. Empty menus are not allowed; when an action becomes available, add it
to the relevant menu, wire it to the shared helper and test it.

---

## D-015 — Properties panel shell uses egui collapsible panels (2026-11-22)

**Context.** Task 1.5 requires a collapsible, resizable properties panel with
section headers and a real empty state, while keeping the fixed width range and
in-session persistence from D-013.

**Decision.** Use `egui::Panel::show_collapsible` with a `properties_open` flag
and a "Hide" control in the panel header. The body uses a `CollapsingHeader`
("Selection") containing the "No selection" empty state; property sections will
be bound in Stage 3. Collapse state is in-session only, while the expanded
width continues to persist through egui memory. The fixed 200–400px range and
compile-time layout budget remain unchanged.

**Consequences.** The panel can be hidden without losing its saved width, and
the collapsed/expanded paths both render at the minimum window size. No dead
placeholder sections are shown; future property editors attach to the existing
section header.

---

## D-016 — Central keyboard shortcut registry (2026-11-22)

**Context.** Task 1.7 requires a central action/shortcut table, a help overlay,
and detection of duplicate bindings. Tool shortcuts were previously decoded
ad hoc in `handle_shortcuts`.

**Decision.** `app.rs` owns `Action`, `KeyBinding` and the `KEYBINDINGS` slice.
`handle_shortcuts` dispatches through that table; `Tool::from_key` is removed in
favor of `Action::SelectTool` bindings. F1 and `?` toggle the help overlay,
Escape closes it, and the overlay lists every registered binding. Duplicate
bindings and tool coverage are tested.

**Consequences.** New shortcuts are added in one place and appear in help
automatically. Panels and menus can later consume the same table instead of
reimplementing key checks.

---

## D-017 — Viewport convention and transform API (2026-11-22)

**Context.** Task 2.1 introduces the first canvas camera. The convention must
be simple, testable and free of egui types.

**Decision.** `Viewport { offset, zoom }` treats `offset` as the screen-space
position of the world origin and `zoom` as the world-to-screen scale.
`world_to_screen(p) = p * zoom + offset`; `screen_to_world(p) = (p - offset) /
zoom`. Zoom is clamped to `[0.1, 30]`. Cursor-anchored zoom keeps the world
point under the cursor fixed. `fit_to_rect` (aliased `fit_bounds`) centers a
world rect in a screen size with a small margin.

**Consequences.** Conversion is a closed-form, invertible pair with no matrix
type. The app can later drive these methods from pointer events without adding
UI dependencies to core.

---

## D-018 — Canvas origin marker and pointer readout (2026-11-22)

**Context.** Task 2.2 needs visible proof that the viewport pipeline is wired
into the shell.

**Decision.** The canvas central panel paints its background and projects the
world origin through `Viewport`, drawing a small marker. Pointer hover positions
are converted to world coordinates relative to the canvas rect and shown in the
status bar. The viewport remains in memory only; persistence is not added yet.

**Consequences.** The marker gives a fixed world-space reference for later
pan/zoom checks. Status-bar coordinates can be compared directly against
`Viewport::screen_to_world`.

---

## D-019 — Pan state lives in the app layer (2026-11-22)

**Context.** Task 2.3 adds panning without a full input state machine yet
(Stage 2.6).

**Decision.** Pan state is a small pair of fields on `EpsilonApp`
(`panning`, `pan_last`) plus the existing `Viewport`. Screen-space deltas are
added directly to `Viewport::offset`, giving 1:1 motion at any zoom. Space-based
panning is disabled while `Context::egui_wants_keyboard_input()` is true. No
pointer capture API is used; the drag ends when no pointer button is down.

**Consequences.** The implementation is simple and reversible. Stage 2.6 can
replace these fields with a proper state machine without changing the viewport
API.

---

## D-020 — Zoom input uses egui deltas plus raw wheel events (2026-11-22)

**Context.** Task 2.4 needs Ctrl+wheel, pinch, scrolling and keyboard zoom.

**Decision.** The canvas applies `InputState::zoom_delta()` for Ctrl+wheel and
pinch gestures, and consumes raw `MouseWheel` events for ordinary scrolling.
Keyboard zoom uses the central shortcut registry and anchors at the pointer or
canvas center. `0` resets to the default viewport; `Shift+0` is reserved for
fit-to-content and currently resets as well because no document content exists.

**Consequences.** Zoom and scroll stay responsive without duplicating egui's
touch handling. Fit-to-content can be refined when document bounds exist.

---

## D-021 — Adaptive grid step uses powers of two (2026-11-22)

**Context.** Task 2.5 requires adaptive spacing without visible drift and
snapping-ready math.

**Decision.** `epsilon-core::grid::grid_step` derives the world spacing from the
configured base size by powers of two until its on-screen size lies in the
requested range. Snapping rounds to the same step. The app renders the grid as
world-aligned lines and keeps the toggle in settings, the View menu, properties
and status bar.

**Consequences.** Grid spacing changes in stable, predictable steps and snapping
stays consistent with the visible grid. Rendering cost is bounded by the minimum
on-screen spacing.

---

## D-022 — Pointer interaction uses an explicit mode (2026-11-22)

**Context.** Task 2.6 requires idle/panning/dragging states, hover tracking,
correct capture/modifier/focus handling, and accurate coordinates.

**Decision.** `EpsilonApp::pointer_mode` is an enum rather than a boolean plus
optional last position. Panning starts only when the pointer is inside the
canvas and the relevant button/modifier conditions hold; space-pan is disabled
while egui wants keyboard input. Hover position is tracked separately, and
viewport math stays in `f64`.

**Consequences.** Pointer state is explicit and testable, panel widgets cannot
start a canvas pan, and panning remains stable at extreme zoom/pan values.

---

## D-023 — Document schema stores z-order explicitly (2026-11-22)

**Context.** Task 3.1 requires ordered elements plus a z-order base field, and
stable serialization for future file I/O.

**Decision.** `Document.elements` preserves insertion order; each `Element`
carries an explicit `z_order`. `Document::paint_order` sorts by `z_order` and
uses insertion order to break ties. `ElementId` is a transparent `u64` newtype
generated from a process-wide atomic counter. Styling and colors are plain core
types, serialized with serde.

**Consequences.** Paint order is deterministic and independent of vector order,
round-trips preserve both order and IDs, and core remains UI-independent.

---

## D-024 — Elements render through a screen-space adapter (2026-11-23)

**Context.** Task 3.2 requires rectangles that persist in the document, render at
10%–1000% zoom, and match their drag bounds without relying on screenshots.

**Decision.** `EpsilonApp` owns the in-memory `Document`. Rendering projects each
element's world bounds through `Viewport` into a screen-space `egui::Rect`;
`element_screen_rect` is a pure helper so zoom behavior is unit-tested. Shape
drags store normalized world bounds and are discarded when either screen
dimension is below 3 pixels. The live preview uses the same projection helper.

**Consequences.** World geometry stays canonical and zoom-independent, while the
app only adapts it at the rendering boundary. Shape-specific drawing can reuse
the same adapter in later tasks.

---

## D-025 — Shape drags carry their element kind (2026-11-23)

**Context.** Task 3.3 adds ellipses while keeping rectangle behavior and the
same minimum-size/preview semantics.

**Decision.** `PointerMode::Dragging` stores the `ElementKind` chosen when the
drag starts. `drawing_kind(tool)` maps drag-created tools to element kinds, and
`commit_shape` creates the corresponding element from normalized world bounds.
Preview and document rendering switch on the kind.

**Consequences.** Both tools share one drag pipeline, the committed shape cannot
change if the active tool changes mid-drag, and new drag-created tools only need
a mapping and a render arm.

---

## D-026 — Diamond vertices are canonical core geometry (2026-11-23)

**Context.** Task 3.4 needs a diamond that is a polygon inscribed in the
element's bounds, with unit-tested geometry and rendering that stays aligned at
every zoom.

**Decision.** `Rect::diamond_points` returns the four edge-midpoint vertices in
core, clockwise from the top. The app projects those world-space vertices
through the viewport for both committed elements and the live drag preview.

**Consequences.** The polygon definition is shared and UI-independent, and
because rendering derives its vertices from the element bounds that future hit
testing and export will use, no separate screen-space diamond math can drift.
Other polygon shapes can follow the same pattern.

---

## D-027 — Linear elements store world-space points with derived bounds (2026-11-23)

**Context.** Task 3.5 needs two-point lines whose endpoints survive
serialization and keep their direction, while the shared element schema still
provides `x`/`y`/`width`/`height` bounds for paint order, projection and future
selection handles.

**Decision.** `Element` gains a `points` vector for linear geometry, serialized
only when non-empty. `Element::line` stores both endpoints in world space and
sets the base bounds from their min/max corners. A line drag therefore commits
the exact world endpoints rather than a screen-space path or a bounds diagonal.

**Consequences.** Positive and reverse drags remain distinguishable, JSON
round-trips preserve endpoints, and later resize/rotate operations can
transform the points and recompute bounds without ambiguity. Closed shapes keep
`points` empty. The new field is serde-defaulted, so older documents remain
readable without a schema-version bump.

---

## D-028 — Arrowheads are clamped world geometry scaled from screen pixels (2026-11-23)

**Context.** Task 3.6 needs arrowheads that stay usable across 10%–1000% zoom
and do not grow behind the start of short arrows.

**Decision.** `geometry::arrowhead_points` returns the tip plus two base
vertices. It clamps the head length to half the shaft and the half-width to the
head length. The app requests a 12px head at the current zoom, converts that to
world units, and projects the resulting vertices through the viewport.

**Consequences.** Arrowheads keep a stable on-screen size, short arrows retain a
visible shaft, and the polygon is testable in core without egui. Zero-length
arrows have no head.

---

## D-029 — Freehand strokes use world-space samples with bounded reduction (2026-11-23)

**Context.** Task 3.7 needs smooth strokes from pointer samples, point
reduction for storage, and protection against unbounded memory on long drags.

**Decision.** Each freehand sample is converted to world coordinates before it
is stored, so panning or zooming mid-stroke does not invalidate earlier points.
Samples closer than 1.5 screen pixels are skipped, the trail is hard-capped at
4096 points, and commit applies one moving-average smoothing pass followed by
Ramer–Douglas–Peucker at a 1px screen tolerance.

**Consequences.** Strokes stay world-stable, rendered and stored polylines are
smaller, and memory is bounded by construction. Pressure is intentionally not
part of the model yet, so input devices behave uniformly.

---

## D-030 — Text edits happen in a foreground overlay (2026-11-23)

**Context.** Task 3.8 needs multiline editing over the canvas without turning
the canvas into a text widget or letting canvas drags fire underneath the
editor.

**Decision.** A new text element starts empty with a wrap width from the click
or drag. The app stores the element ID and a text buffer, renders a foreground
`TextEdit` over the element's screen rect, and blocks canvas input while that
editor is active. Escape or a click outside the editor commits; empty content
removes the element. Commit measures the wrapped layout at the current zoom and
stores its world-space height.

**Consequences.** Editing uses egui's native text behavior and remains visually
anchored to the element. Text layout is measured in the app boundary while the
document stays UI-independent, and deleted empty text leaves no placeholder.

---

## D-031 — Hit testing rotates the query into local element space (2026-11-23)

**Context.** Task 3.9 needs one hit-test implementation for every element kind,
including rotated elements, plus marquee intersection and stroke tolerance.

**Decision.** `Element::hit_test` rotates the world query point around the
element center by `-angle`, then tests the unrotated bounds/shape. Transparent
fills only hit near the stroke; filled shapes also hit inside. Linear and
freehand elements use point-to-segment distance. Marquee intersection uses
rotation-expanded world bounds. The app converts a 6px selection tolerance to
world units at the current zoom.

**Consequences.** Rotation handling is shared and does not require duplicating
shape math for transformed coordinates. Selection feels consistent across zoom
levels, and filled versus outlined behavior is explicit and testable.

---

## D-032 — Move/resize transforms start from drag snapshots (2026-11-23)

**Context.** Task 3.10 requires multi-element resizing without drift and with
screen-space handles that work at any zoom.

**Decision.** A drag captures clones of the selected elements and the starting
selection bounds. Every frame computes the full transform from those snapshots:
moves add a world delta, and resizes map the captured geometry from the start
bounds into the new bounds returned by `resize_rect`. Handles are drawn and hit
tested in screen space, with the resize minimum converted from 8 screen pixels
to world units. Shift preserves corner aspect, Alt mirrors around the center.

**Consequences.** Continuous drags cannot accumulate floating-point drift, all
selected elements share one transform, and handle size/hit areas stay usable
across 10%–1000% zoom. Rotation remains a separate transform in task 3.11.

---

## D-033 — Rotation snapshots transform from the collective center (2026-11-23)

**Context.** Task 3.11 needs multi-element rotation with Shift snapping, plus
usable handles after a selection has been rotated.

**Decision.** A rotation drag captures the selected elements and the center of
their combined world bounds. Each frame computes an angle delta from the drag
start, optionally snaps it to 15°, and applies `rotated_around` to every
snapshot. Closed shapes accumulate `angle`; linear point geometry is rotated in
world space and keeps `angle = 0`. Selection handles continue to use the
rotation-expanded world bounds in screen space.

**Consequences.** Multiple elements rotate around one common pivot, Shift gives
predictable angles, and handle placement remains axis-aligned and usable.
Rotated closed shapes render as projected polygons; text rotation rendering is
still axis-aligned and can be revisited when text layout is enhanced.

---

## D-034 — Style patterns are resolved at the app rendering boundary (2026-11-23)

**Context.** Task 3.12 adds opacity, stroke patterns, fill patterns and corner
roundness without giving core any egui dependency.

**Decision.** `ElementStyle` stores portable enums and numbers, serde-defaulted
for older documents. The app applies opacity to both colors, draws dashed and
dotted paths with screen-space dash lengths, fills hatch/cross-hatch by
intersecting diagonal scanlines with the projected polygon, and applies
roundness only to unrotated native rectangles. Properties controls copy the
first selected element's style, expose the options, and write changed values to
every selected element.

**Consequences.** Core remains renderer-agnostic while all style options are
serializable and round-trip safe. Mixed selections currently adopt one edited
style, which task 3.14 can refine with explicit mixed-value presentation.

---

## D-035 — Rough strokes are seeded core geometry; clean mode is a persisted toggle (2026-11-23)

**Context.** Task 3.13 needs hand-drawn strokes that are reproducible across
frames and exports, without giving up an exact architect-style mode.

**Decision.** `geometry::rough_polyline` uses a SplitMix-style deterministic
generator seeded by the element seed to jitter vertices and segment midpoints
by a screen-space amplitude converted to world units. Fill geometry remains
exact; only stroke paths are roughened. `clean_rendering` defaults to false,
lives in app settings, and toggles from View → Clean rendering.

**Consequences.** Re-rendering the same seed is point-identical, exports can
reuse the same function, and users can switch to clean geometry without changing
the document. Rough paths add only O(points) work per element; a headless
500-element render stays under the two-second test budget.

---

## D-036 — Properties derive from the live selection and show mixed values (2026-11-23)

**Context.** Task 3.14 binds position, size, angle and style to the selection
without introducing Stage 4's command/history layer.

**Decision.** Each frame the panel recomputes uniform values across selected
elements. Disagreeing numeric fields display `Mixed` through a DragValue
formatter and still accept a new value. Position edits apply a delta so relative
layout survives; size edits scale each selected element around its own origin;
angle edits set closed-shape rotation and adjust linear direction. Style edits
write one resolved style to every selected element.

**Consequences.** Panel state always follows the current selection and edits
update the canvas immediately. Undo will wrap these mutations when Stage 4
lands; until then each edit is a direct document change.

---

## D-037 — History stores reversible commands, not document snapshots (2026-11-25)

**Context.** Task 4.1 needs undo/redo with drag coalescing, bounded memory and
exact document restoration, without cloning the whole document for every edit.

**Decision.** `epsilon-core::history` defines an `Insert`, `Remove` and `Replace`
command enum. Each command carries only the affected elements: inserts keep the
target vector index, removals keep each element plus its original index, and
replacements keep before/after snapshots matched by `ElementId`. `Transaction`
groups commands for one user action and applies or reverts them atomically;
commands validate before mutating, and a failed transaction rolls back the
commands it already applied. `History` applies committed transactions, keeps a
redo stack that is discarded by a new commit, and trims the undo stack to
`limit` entries (default 100, configured with `with_limit`; zero means edits are
applied but not undoable). Repeated drag updates use `commit_coalescing`, which
merges consecutive single-`Replace` transactions touching the same IDs: the
first before-state and last after-state win. Separating user actions must use
plain `commit`. History is session-only and is not serialized into documents.

**Consequences.** Memory is proportional to edited elements rather than total
scene size, vector order and `z_order` restore exactly, and undo/redo equality is
testable without serialization. The app must route every mutation through a
transaction for it to be undoable; task 4.2 wires shape creation, transforms,
style edits and text commits accordingly. Direct document mutation outside the
history remains possible at the API level and is intentionally not tracked.

---

## D-038 — Undo wraps finished user actions; pending edits block history (2026-11-25)

**Context.** Task 4.2 wires the core history into an editor where creation,
continuous pointer drags and properties widgets mutate the document at different
cadences.

**Decision.** Shape, line, arrow and freehand creation commit one
`Transaction::insert` through `insert_element`. Text creation stays out of
history while the canvas editor is open and is recorded as one insert with the
final content and measured height on commit; empty text leaves no entry. Move,
resize and rotation record one `Replace` per drag from the drag-start snapshot
when the pointer is released. Properties transform/style edits keep a pre-edit
selection snapshot while any pointer button is down and commit one `Replace` on
release; discrete changes such as typing a value or toggling a style commit
immediately. `Action::Undo`/`Action::Redo` live in the central registry
(Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y) and drive the Edit menu, toolbar buttons and
shortcuts; button enabled state reads `can_undo`/`can_redo`. History operations
are ignored while the canvas text editor is open or a drag/properties edit is in
flight. Undo/redo shortcuts are still dispatched when a non-canvas widget owns
keyboard focus, so property fields do not swallow them, while the canvas text
editor keeps its own TextEdit undo. The status bar shows undo/redo depths and any
history error.

**Consequences.** Every Stage 3 document edit becomes one undo step regardless
of frame cadence, without full-document clones. Undo/redo cannot interleave with
an unfinished edit and reapply stale snapshots on the next frame. History stays
session-only; persistence belongs to Stage 5.

---

## D-039 — Clipboard payloads are versioned JSON; shortcuts use egui clipboard events (2026-11-26)

**Context.** Task 4.3 needs copy/cut/paste that survives across paste cycles and
can interoperate with the system clipboard. Elements already serialize as JSON,
but raw element vectors have no type marker, so unrelated clipboard text could
be mistaken for a selection. eframe's winit integration also translates
Ctrl+C/X/V into `Event::Copy`, `Event::Cut` and `Event::Paste(text)` before any
key press reaches egui, and egui offers no synchronous clipboard read.

**Decision.** `epsilon-core::clipboard` owns a `ClipboardPayload` with an
`epsilon/clipboard` format marker, a payload version and the copied elements.
Copy/cut serialize it to system clipboard text via `Context::copy_text`; Ctrl+V
parses `Event::Paste` text and rejects anything unsupported. Menu Paste uses the
last in-app payload because the platform clipboard cannot be read on demand.
Pasting goes through core's `paste_elements`, which sorts by paint order,
reserves document and payload IDs before generating fresh ones, assigns new
z-order slots and applies a cascading 10-unit world offset. Paste inserts in one
history transaction and selects the copies; cut also removes in one transaction.
`serde_json` moves from a core dev-dependency to a runtime dependency. The
Ctrl+C/X/V entries stay in the central shortcut registry for the help overlay
and duplicate checks, and serve as a key-event fallback, while the native path
is the clipboard events.

**Consequences.** Copies never collide with live IDs or reuse the copied ID,
relative order survives, repeated pastes stay visible, and both paste and cut
are undoable. The format is self-describing, so future versions can be rejected
rather than corrupted. Only currently supported payload versions paste; menu
and cross-window paste differ because there is no clipboard read API. Clipboard
handling is skipped whenever a text widget owns the keyboard, keeping TextEdit
clipboard behavior intact.

---

## D-040 — Selection commands share one gated action path; duplicate reuses paste (2026-11-26)

**Context.** Task 4.4 adds Delete/Backspace, Ctrl+D duplicate, Ctrl+A select all
and Esc. These must not fire while the canvas text editor owns the keyboard, an
ongoing drag/properties edit holds history snapshots, or the help overlay is
open. Duplicate also needs the same fresh-ID and paint-order guarantees as
paste, and Esc already closes both the help overlay and the text editor.

**Decision.** Delete and Backspace map to one `Action::Delete` that calls the
same history-removing `delete_selection` used by cut; Esc maps to
`Action::ClearSelection`, dispatched only after the help overlay's own Escape
handling and suppressed while any widget owns keyboard focus. Duplicate builds
an in-memory `ClipboardPayload` from the selection and calls its
`paste_elements` with a fixed `DEFAULT_GRID_SIZE` offset (20 world units,
whether or not the grid is visible), then inserts the copies in one history
transaction and selects them. Duplicate deliberately leaves `clipboard` and
`paste_count` untouched. Select all replaces the selection set with every
document element. All four no-op when `history_blocked()` is true, which covers
the text editor, active drags and properties edits. The Edit menu exposes the
three non-Escape commands with the same enabled predicates.

**Consequences.** The shortcuts, menu and future context-menu entries share one
behavior, and selection commands cannot corrupt an in-flight edit. Duplicate
offset is deterministic and independent of grid visibility. Esc precedence is
help, then text editor (which commits), then clear selection and return to
Select; no command silently mutates the selection while a drag is unfinished.

---

## D-041 — Z-order moves are computed in core and committed as one dense Replace (2026-11-26)

**Context.** Task 4.5 needs front/back and one-step forward/backward moves that
respect multi-selection, are testable without egui, survive undo and never leave
a stale selection. Elements are painted by ascending `z_order` with vector order
breaking ties, and z values can in principle be imported non-dense.

**Decision.** `ZOrderMove` and `Document::z_order_changes(ids, movement)` live in
`epsilon-core::document`. The method derives the current paint order, computes
the target order (stable block partition for front/back; descending/ascending
adjacent swaps for forward/backward so a contiguous selection moves as one
group), then assigns dense `0..n` paint slots and returns before/after snapshots
only for elements whose slot changed. The app builds one `Transaction::replace`
from those pairs and commits it through history; it does not touch `selected`.
Shortcuts are Ctrl+Shift+] / Ctrl+] / Ctrl+[ / Ctrl+Shift+[ and the same moves
appear under Edit → Order. Unknown IDs, all-selected scenes and moves blocked at
the front/back return no changes, so no empty history entry is recorded.

**Consequences.** Ordering semantics are pure, unit-tested and independent of the
UI. One user action is one undo step and selection survives reorder. Rewriting
slots densely normalises non-dense z values the first time a move touches an
element, at the cost of sometimes replacing unselected neighbours that swap
slots. Mid-drag and text-editing states block reorder through the existing
`history_blocked()`/`can_copy()` gates.

---

## D-042 — Group membership is an outermost-first ID stack; selection expands to whole groups (2026-11-26)

**Context.** Task 4.7 needs grouping and ungrouping, nested groups, and a
selection model where clicking any member selects the whole group. Excalidraw
stores `groupIds` as a string array on every element, so the core type must
remain serializable to that shape for Stage 5 interop.

**Decision.** `Element` gains a serde-defaulted `group_ids: Vec<GroupId>` where
index 0 is the outermost group and the last entry is the innermost. `GroupId` is
a transparent string newtype generated from a process counter (`g1`, `g2`, …).
Grouping expands the selection to whole outermost groups and prepends one new
group ID to every member; ungrouping removes index 0 from every member of each
selected outermost group, exposing the next nesting level. `Document` owns
`expand_selection`, `group_changes` and `ungroup_changes`, all pure and
UI-independent; the app commits their snapshots as one `Transaction::replace`.
Clicks, marquees, shift-toggles and right-click targeting call
`expand_selection`, so a group is always selected as a unit. Clipboard paste
maps every source group ID to a fresh ID while preserving the stack order, so
copies keep their nesting without joining the original group. Ctrl+G and
Ctrl+Shift+G, the Edit menu and the context menu expose the commands.

**Consequences.** Group operations are undoable without cloning the document and
never reorder elements. Nested groups remain intact until their outer level is
ungrouped. Group IDs round-trip as Excalidraw-compatible strings, while empty
membership stays out of JSON so ungrouped documents are unchanged. Selection is
always group-complete, which also makes delete, copy, duplicate and transforms
operate on whole groups without special cases.

---

## D-043 — Alignment uses collective rotated world bounds and Edit → Arrange (2026-11-26)

**Context.** Task 4.8 needs six alignment commands whose results are correct for
rotated elements, not just unrotated local rectangles, and whose behaviour can
be tested without egui.

**Decision.** `Align { Left, CenterX, Right, Top, CenterY, Bottom }` and
`Document::align_changes(ids, align)` live in core. The function collects the
axis-aligned `world_bounds()` of the selection, derives the collective edge or
centre, and returns before/after snapshots translated onto it. It ignores
unknown IDs, needs at least two elements and omits elements that are already on
the target, so the app records no empty history entry. The app commits all
snapshots in one `Transaction::replace`, preserves the selection, and exposes
the commands under Edit → Arrange rather than assigning six new shortcuts.

**Consequences.** Alignment matches what the user sees for rotated elements and
keeps the core UI-independent. The Arrange submenu is enabled only for a
multi-selection, and each alignment is one undo step. No new dependency or
shortcut-table surface was introduced.

---

## D-044 — Distribution equalises gaps between world bounds and fixes the extremes (2026-11-26)

**Context.** Task 4.9 needs horizontal and vertical distribution for three or
more elements, including already-even and overlapping selections, with results
that can be unit-tested independently of the UI.

**Decision.** `Distribute { Horizontal, Vertical }` and
`Document::distribute_changes(ids, axis)` live in core. Elements are ordered by
their rotated world bounds along the axis (ties by element ID), the outermost
elements stay fixed, and the interior elements are translated so the gap
between consecutive bounds is `(outer extent - total size) / (n - 1)`. The gap
may be negative for overlapping selections rather than being rejected.
Already-even selections, fewer than three elements and unknown IDs produce no
changes. The app commits changes in one `Transaction::replace`, preserves the
selection, and exposes both axes under Edit → Arrange beside alignment; no
shortcuts were assigned.

**Consequences.** Distribution is deterministic, handles mixed element sizes and
overlap, and never adds empty history entries. The outer extent is stable, so
repeated distribution is a no-op. No new dependencies or shortcut-table surface
were introduced.

---

## D-045 — `.epsilon` v1 envelope validates version first and preserves extras at file/document/app-state levels (2026-11-26)

**Context.** Task 5.1 needs a durable native file format with a schema version,
document, app state and unknown-field preservation, plus a clear failure for
files written by a future build.

**Decision.** `epsilon_core::format::EpsilonFile` is the v1 envelope:
`format: "epsilon"`, `schema_version: 1`, `document`, `app_state`, and a
flattened `Extras` map. `AppState` holds a serializable `Viewport` plus its own
extras; `Document` also carries flattened extras. Unknown keys survive
serialization at all three levels. `from_json` parses to `serde_json::Value`
first, validates the format marker and envelope version (then the nested
document schema), and only then deserializes, so future versions fail with
`UnsupportedVersion` before any incompatible field shapes can produce a
misleading error. App settings such as theme and tool remain in `app.ron`;
only per-document view state belongs in the file.

**Consequences.** The format is human-readable and forward-compatible for
additive fields without ever accepting a future schema as if it were v1. The
document model gains one serialized field and `Viewport` gains serde derives,
with no new dependencies. Save/open, dirty tracking and atomic writes are
intentionally left to 5.2.

---

## D-046 — Save uses rfd, atomic sibling writes, and a saved-document snapshot for dirty state (2026-11-26)

**Context.** Task 5.2 needs native file dialogs, dirty tracking, and writes that
cannot truncate an existing file when serialization or I/O fails.

**Decision.** Add `rfd` 0.18 (xdg-portal/Wayland defaults on Linux; no GTK or
WebView). Core owns path I/O: `EpsilonFile::save_to_path` writes pretty JSON to a
hidden unique sibling temp file, calls `sync_all`, then renames over the target;
`load_from_path` reads and validates through the existing v1 checks. The app
keeps the path and a clone of the document at the last successful save/open, so
`is_dirty` is a direct document comparison that also becomes clean again after
undoing back to the saved state. File → Open/Save and Ctrl+O/Ctrl+S invoke the
dialogs; errors and successes appear in the status bar, and the title/status bar
show the file name plus a dirty marker. Save As appends `.epsilon` when the
chosen extension differs. Viewport-only changes do not mark the document dirty.

**Consequences.** Failed writes leave the destination untouched and the document
dirty. Dirty tracking costs one document clone and a comparison only when the
title or status label is drawn. Dialog interactions cannot be exercised
headlessly; path-level round-trips and failure paths are covered by tests. New,
Save As and the close guard remain task 5.3.

---

## D-047 — Destructive document actions share a pending-action guard; close requests are cancelled first (2026-11-26)

**Context.** New, Open and quitting can discard unsaved work, and a compositor
close cannot be vetoed after the window is gone. Save As can also be cancelled,
which must not lose the document.

**Decision.** `PendingAction { New, Open, Quit }` records the action waiting on a
single Save/Discard/Cancel modal. Dirty New/Open and File → Quit open the guard
without touching the scene. A compositor close request first commits any live
text edit, then sends `ViewportCommand::CancelClose` and opens the guard while
the document is dirty and no close has been approved. `resolve_unsaved` only
continues after `save_document()` succeeds; a failed save or a cancelled Save As
leaves the prompt and document intact. Discard executes the pending action
directly. Quit sets `allow_close` before sending `Close` so the guard does not
cancel its own close request. The snapshot at the last successful save/open
defines dirty state, so undoing back to that state needs no prompt.

**Consequences.** All destructive paths share one tested decision point, and a
failed save cannot silently discard work. The live modal and compositor close
flow remain manual checks; headless tests exercise the resolution methods.

---

## D-048 — Autosaves rotate in the app-data recovery directory and are offered only on launch (2026-11-26)

**Context.** Task 5.4 needs periodic autosaves that survive a crash, a launch
offer that can be declined, and a bounded directory. Overwriting one file would
make retention and corrupt-file fallback impossible to test.

**Decision.** Core `recovery::RecoveryStore` writes ordinary `EpsilonFile` JSON
to `<app data>/recovery/autosave-<13-digit-ms>-<4-digit-sequence>.epsilon`
using the same atomic write as normal saves. Only the newest five names are
kept. `load_latest` walks newest to oldest and skips corrupt files. The source
document path, when present, is stored in the envelope's app-state extras under
`recovery_source_path`; it is not part of normal saves. The app autosaves dirty,
idle documents every 30 seconds, never while the recovery offer is unresolved,
and shows one Recover/Discard window at startup (`EpsilonApp::new`). Recover
restores document/viewport/path, clears session history and selection, and marks
the document dirty so saving prompts. Successful normal save/open/new, explicit
discard and approved quit remove the recovery files.

**Consequences.** A crash can lose at most 30 seconds of work, older valid
autosaves survive a corrupt newest file, and cleanup is bounded and tested
headlessly. Autosaves include document data, so the app-data directory should be
treated as document storage (no telemetry/cloud). The live timer and launch modal
remain manual checks.

---

## D-049 — Recent documents are a bounded core list persisted in app settings, with stale entries removed lazily (2026-11-26)

**Context.** Task 5.5 needs a recent list in the File menu and welcome state,
deduplication, a cap, persistence, and graceful handling of deleted files.

**Decision.** Core `recent::RecentFiles` stores at most eight `PathBuf`s,
most-recent first, deduplicated by exact path. It is a serde-transparent field
in `PersistedSettings` (`epsilon.settings`), not document data. Successful
save/open records the path; File → Open Recent and the dismissible welcome panel
list names with full paths as hover text. Clicking a stale path removes it and
reports an error instead of attempting a read. Recent opens route through the
same unsaved-changes guard as File → Open, carrying an optional path in
`PendingAction::Open`. "Clear Recent" resets the list. No filesystem watcher is
introduced; missing files are handled on interaction.

**Consequences.** The list is deterministic, bounded and testable without UI,
and persistence reuses the existing eframe settings path. Stale entries remain
visible until clicked or explicitly cleared, which avoids blocking startup on
network/slow filesystems.
