# Project State

Last updated: 2026-11-22
Current stage: **Stage 1 — Native desktop shell**
Last completed task: **1.2 — Theme system (dark / light)**
Next task: **1.3 — Tool model and left tool panel**

## Status

The native shell is laid out and themed: menu bar, top toolbar, left tool rail,
canvas region, right properties panel and status bar all render, resize and
follow the active dark or light theme. There are still **no drawing features and
no tools**; the canvas is a placeholder region until Stage 2 and the tool rail
is empty until task 1.3. Do not describe the product as usable.

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
  src/app.rs                EpsilonApp: six-region layout, menus, About
```

Shell behavior:

- Theme: `Theme::{Dark, Light}` (default dark). `theme::apply` sets an explicit
  `ThemePreference` plus Epsilon's `Visuals`, so the OS theme cannot override
  the choice. View → "Dark mode" toggles it; every region reads the theme from
  `egui::Visuals`, and the canvas uses `extreme_bg_color`.
- Menu bar: "Epsilon" (About, Quit) and "View" (Dark mode).
- Top toolbar: wordmark, "Untitled", version.
- Left: fixed 64px tool rail with a "TOOLS" header (buttons arrive in 1.3).
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
  `Context::style_of(egui::Theme)` and `Context::theme()`.
- Remote: `origin git@github.com/paleaxis/epsilon.git`, branch `main`.

## Verified commands and results (task 1.2)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0, no diffs |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | **20 passed, 0 failed** (9 app, 11 core, 0 doc tests) |
| `cargo build --workspace` | success, no warnings |
| `timeout 8 ./target/debug/epsilon` | exit 124 (still running at 8 s), no stderr output |

App tests cover: layout budgets and the canvas-width clamp; headless shell
layout at 720×480 and 1920×1080 in dark and light themes; immediate theme
application to the egui context; WCAG contrast (primary text ≥ 7:1, weak text
≥ 4.5:1 against panel and canvas in both themes); canvas distinct from chrome;
theme switch propagates to context style.

Native launch was confirmed by process liveness. The View-menu checkbox was not
clicked through in a native session, and the palette was not visually reviewed
in a screenshot; theme propagation is covered by the headless/style tests above.

## Features implemented

- Dark and light themes with a central semantic palette, tested contrast, and
  a View-menu toggle applied immediately to every region.
- Six-region native shell with resize-safe geometry and compile-time layout
  budget assertions.
- Functional menus (About dialog, Quit, theme toggle) and properties empty state.
- UI-independent core with tested `Point`/`Rect` geometry.
- Documentation baseline (`AGENTS.md`, roadmap, decisions, changelog).

## Known issues / limitations

- Tool rail is intentionally empty until task 1.3; File/Edit/Help menus arrive in
  1.4; status readouts are static until 1.6.
- Canvas is a placeholder; drawing/navigation arrive in Stage 2.
- Theme choice is not persisted across restarts (task 1.8); app always starts in
  dark mode.
- Panel widths are not persisted across restarts (task 1.8).
- No CI configured; verification is local only.
- No visual screenshot review has been performed.
- Project license: GPL-3.0 (repository `LICENSE`); manifests declare
  `GPL-3.0-only`.

## Resume instructions

1. Read `AGENTS.md`, `project/ROADMAP.md`, this file, and `project/DECISIONS.md`.
2. `git status` — the tree must be clean at the start of a task.
3. `cargo test --workspace` to confirm the baseline before editing.
4. Implement only task **1.3** (Tool enum, active-tool state, left rail buttons
   with tooltips/shortcut hints, V/H/R/O/D/L/A/P/T shortcuts, shortcut→tool test).
5. Verify with `cargo fmt --all -- --check`,
   `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace`, `cargo build --workspace`, and
   `timeout 8 ./target/debug/epsilon`.
6. Update this file, `ROADMAP.md`, `CHANGELOG.md`, then commit and push.

Do not start Stage 2 canvas work before the Stage 1 shell tasks are complete.
