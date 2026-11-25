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
