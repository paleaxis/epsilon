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
