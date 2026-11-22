# Project State

Last updated: 2026-11-22
Current stage: **Stage 1 — Native desktop shell**
Last completed task: **1.1 — Application shell layout**
Next task: **1.2 — Theme system (dark / light)**

## Status

The native shell is laid out: menu bar, top toolbar, left tool rail, canvas
region, right properties panel and status bar all render and resize. There are
still **no drawing features and no tools**; the canvas is a placeholder region
until Stage 2. Do not describe the product as usable.

Repository layout:

```
Cargo.toml                  workspace (edition 2024, rust 1.95+)
AGENTS.md                   permanent rules
README.md                   build/run overview
project/                    STATE, ROADMAP, DECISIONS, CHANGELOG
crates/epsilon-core/        UI-independent engine (no egui dependency)
  src/geometry.rs           Point, Rect + 11 unit tests
crates/epsilon-app/         eframe desktop shell
  src/main.rs               native entry point
  src/layout.rs             shell size budgets, compile-time assertions, clamp helper
  src/app.rs                EpsilonApp: six-region layout + About window
```

Shell layout (egui 0.37 unified `Panel` API):

- Top: menu bar ("Epsilon" menu → About Epsilon, Quit) above a top toolbar
  (wordmark, "Untitled", version).
- Bottom: status bar ("Ready", app version).
- Left: fixed 64px tool rail with a "TOOLS" header (buttons arrive in 1.3).
- Right: resizable properties panel (default 264, range 200–400) with a
  "No selection" empty state.
- Center: canvas region (placeholder text) painted with the theme's
  extreme background color.
- Width clamp (`layout::properties_panel_max_width`) prevents the properties
  panel from squeezing the canvas below 240px even if the WM forces a small
  window.

## Environment

- Linux (Arch/CachyOS), Wayland + X11 available; Rust 1.100.0 (system toolchain,
  no rustup), cargo 1.100.0.
- egui/eframe **0.37.0**, glow (OpenGL) backend; requires Rust ≥ 1.95.
- eframe 0.37 API: `eframe::App::ui(&mut self, ui: &mut egui::Ui, frame:
  &mut eframe::Frame)`; panels take `&mut Ui`. `egui::Panel::top/left/right/
  bottom` replaces the old `TopBottomPanel`/`SidePanel`; `Window::show` still
  takes `&Context`.
- Remote: `origin git@github.com:paleaxis/epsilon.git`, branch `main`.

## Verified commands and results (task 1.1)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0, no diffs |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | **15 passed, 0 failed** (4 app: layout clamp + headless 720×480 and 1920×1080 shell; 11 core geometry; 0 doc tests) |
| `cargo build --workspace` | success, no warnings |
| `timeout 8 ./target/debug/epsilon` | exit 124 (still running at 8 s), no stderr output |

The headless shell tests render one real egui frame via `Context::run_ui` at the
minimum and a large window size and assert the canvas keeps its 240×240 minimum
while the tool rail, menu bar, toolbar, properties panel and status bar occupy
their reserved edges. Native launch was confirmed by process liveness; visual
inspection was not performed.

## Features implemented

- Six-region native shell with resize-safe geometry and compile-time layout
  budget assertions.
- Functional menu with About dialog and Quit; keyboard/menu navigation from egui.
- Properties panel empty state; canvas/resizable side-panel behavior.
- UI-independent core with tested `Point`/`Rect` geometry.
- Documentation baseline (`AGENTS.md`, roadmap, decisions, changelog).

## Known issues / limitations

- Tool rail is intentionally empty until task 1.3; menus are minimal
  (File/Edit/View/Help arrive in 1.4); status readouts are static until 1.6.
- Canvas is a placeholder; drawing/navigation arrive in Stage 2.
- Theme switching is not implemented yet (task 1.2); shell uses egui default
  visuals.
- Panel widths are not persisted across restarts (task 1.8).
- No CI configured; verification is local only.
- UI verified headlessly at two window sizes and by native launch; no visual
  screenshot review has been done.
- Project license: GPL-3.0 (repository `LICENSE`); manifests declare
  `GPL-3.0-only`.

## Resume instructions

1. Read `AGENTS.md`, `project/ROADMAP.md`, this file, and `project/DECISIONS.md`.
2. `git status` — the tree must be clean at the start of a task.
3. `cargo test --workspace` to confirm the baseline before editing.
4. Implement only task **1.2** (central dark/light palette, View-menu toggle,
   all shell regions follow the theme; no hard-coded shell colors left).
5. Verify with `cargo fmt --all -- --check`,
   `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace`, `cargo build --workspace`, and
   `timeout 8 ./target/debug/epsilon`.
6. Update this file, `ROADMAP.md`, `CHANGELOG.md`, then commit and push.

Do not start Stage 2 canvas work before the Stage 1 shell tasks are complete.
