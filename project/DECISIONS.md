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

**Consequences.** Commands must be tested for round-trip equality. Exact data
structures are finalized in task 4.1, not before.

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
