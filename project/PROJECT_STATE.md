# Project State

Last updated: 2026-11-22
Current stage: **Stage 1 — Native desktop shell**
Last completed task: **1.3 — Tool model and left tool panel**
Next task: **1.4 — Menus and top toolbar**

## Status

The native shell is laid out, themed, and has a working tool palette: nine tools
with painted vector icons, tooltips, active-tool highlighting, and single-key
shortcuts. Selecting a tool changes UI state only — **nothing draws yet**; the
drawing engine is Stage 3 and the canvas remains a placeholder until Stage 2. Do
not describe the product as usable.

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
  src/theme.rs              semantic palettes, Visuals setup, theme application
  src/tools.rs              Tool enum, shortcuts, painted icons, tool buttons
  src/app.rs                EpsilonApp: shell, menus, tool state, About
```

Shell behavior:

- Tool palette: Select (V), Hand (H), Rectangle (R), Ellipse (O), Diamond (D),
  Line (L), Arrow (A), Freehand (P), Text (T). Plain key presses select a tool
  unless a widget owns keyboard input; Ctrl/Alt/Cmd combinations are ignored.
  Buttons are 40×34 with 20×20 vector icons painted from the active theme colors,
  selected/hover/focus states, tooltips with shortcut hints, and accessible
  button info.
- Theme: `Theme::{Dark, Light}` (default dark), applied through an explicit
  `ThemePreference` plus Epsilon's `Visuals`; View → "Dark mode" toggles it.
- Menu bar: "Epsilon" (About, Quit) and "View" (Dark mode).
- Top toolbar: wordmark, "Untitled", version.
- Right: resizable properties panel (default 264, range 200–400) with a
  "No selection" empty state.
- Bottom: status bar ("Ready", app version).
- Canvas region: placeholder text on the palette canvas color.
- Width clamp (`layout::properties_panel_max_width`) prevents the properties
  panel from squeezing the canvas below 240px even if the WM forces a small
  window.

## Environment

- Linux (Arch/CachyOS), Wayland + X11 available; Rust 1.100.0 (system toolchain,
  no rustup), cargo 1.100.0.
- egui/eframe **0.37.0**, glow (OpenGL) backend; requires Rust ≥ 1.95.
- eframe 0.37 API: `eframe::App::ui(&mut self, ui: &mut egui::Ui, frame:
  &mut eframe::Frame)`; panels take `&mut Ui` via `egui::Panel::top/left/right/
  bottom`; `Window::show` takes `&Context`. `Context::style()` is gone — use
  `style_of(egui::Theme)`. Keyboard capture check is
  `Context::egui_wants_keyboard_input()`.
- Remote: `origin git@github.com:paleaxis/epsilon.git`, branch `main`.

## Verified commands and results (task 1.3)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0, no diffs |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | **25 passed, 0 failed** (14 app, 11 core, 0 doc tests) |
| `cargo build --workspace` | success, no warnings |
| `timeout 8 ./target/debug/epsilon` | exit 124 (still running at 8 s), no stderr output |

App tests cover: shortcut→tool mapping for all nine keys, uniqueness of shortcut
letters, unrelated keys selecting nothing, keyboard events changing the active
tool, modified (Ctrl) keys not changing it, layout budgets, headless shell layout
(720×480 and 1920×1080, both themes), theme propagation and WCAG contrast.

Native launch confirmed by process liveness. Tool-button clicks were not
automated in a native session and the painted icons were not visually reviewed;
keyboard selection and state wiring are covered by tests.

## Features implemented

- Working tool model and palette with shortcuts, tooltips, selected state and
  accessibility info (UI state only; drawing in Stage 3).
- Dark and light themes with a central semantic palette and tested contrast.
- Six-region native shell with resize-safe geometry and layout budget assertions.
- Functional menus (About, Quit, theme toggle) and properties empty state.
- UI-independent core with tested `Point`/`Rect` geometry.
- Documentation baseline (`AGENTS.md`, roadmap, decisions, changelog).

## Known issues / limitations

- Selecting a tool has no canvas effect yet; drawing engine is Stage 3.
- File/Edit/Help menus arrive in 1.4; the active tool is not repeated in the
  status bar yet (1.6).
- Canvas is a placeholder; navigation arrives in Stage 2.
- Theme and panel widths are not persisted across restarts (task 1.8).
- No CI configured; verification is local only.
- No visual screenshot review has been performed (icons and selection colors are
  covered only by code review, layout tests and launch checks).
- Project license: GPL-3.0 (repository `LICENSE`); manifests declare
  `GPL-3.0-only`.

## Resume instructions

1. Read `AGENTS.md`, `project/ROADMAP.md`, this file, and `project/DECISIONS.md`.
2. `git status` — the tree must be clean at the start of a task.
3. `cargo test --workspace` to confirm the baseline before editing.
4. Implement only task **1.4** (functional File/Edit/View/Help menus and top
   toolbar; only actions that already work, no dead entries).
5. Verify with `cargo fmt --all -- --check`,
   `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace`, `cargo build --workspace`, and
   `timeout 8 ./target/debug/epsilon`.
6. Update this file, `ROADMAP.md`, `CHANGELOG.md`, then commit and push.

Do not start Stage 2 canvas work before the Stage 1 shell tasks are complete.
