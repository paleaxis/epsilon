# Project State

Last updated: 2026-11-22
Current stage: **Stage 1 — Native desktop shell** (Stage 0 complete)
Last completed task: **0.1 — Repository foundation, architecture and scaffold**
Next task: **1.1 — Application shell layout**

## Status

The workspace exists, builds, and launches a native window. There is **no
whiteboard functionality yet**: the app window is a minimal placeholder showing
its version, and `epsilon-core` currently contains only tested geometry
primitives. Do not describe the product as usable.

Repository layout:

```
Cargo.toml                  workspace (edition 2024, rust 1.95+)
AGENTS.md                   permanent rules
README.md                   build/run overview
project/                    STATE, ROADMAP, DECISIONS, CHANGELOG
crates/epsilon-core/        UI-independent engine (no egui dependency)
  src/geometry.rs           Point, Rect + 11 unit tests
crates/epsilon-app/         eframe desktop shell
  src/main.rs               native entry point (window 1280x800, min 720x480)
  src/app.rs                EpsilonApp, minimal placeholder UI
```

## Environment

- Linux (Arch/CachyOS), Wayland + X11 available; Rust 1.100.0 (system toolchain,
  no rustup), cargo 1.100.0.
- egui/eframe **0.37.0**, glow (OpenGL) backend; requires Rust ≥ 1.95.
- eframe 0.37 API note: `eframe::App::ui(&mut self, ui: &mut egui::Ui, frame:
  &mut eframe::Frame)`; `CentralPanel::show(ui, …)` takes `&mut Ui`.
- Remote: `origin git@github.com:paleaxis/epsilon.git`, branch `main`.

## Verified commands and results (task 0.1)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0, no diffs |
| `cargo test --workspace` | 11 passed, 0 failed (epsilon-core geometry + doc tests 0) |
| `cargo build --workspace` | success, no warnings |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `timeout 8 ./target/debug/epsilon` | exit 124 (still running at 8 s), no stderr output |

Native window launching was confirmed by process liveness, not by visual
inspection. No performance or GPU-behavior claim is made yet.

## Features implemented

- Native application bootstrap with eframe/glow; window title, initial and
  minimum size; single `EpsilonApp` type.
- UI-independent core crate with `Point` (arithmetic, length, normalization,
  lerp, f32 conversion) and `Rect` (normalizing constructors, bounds, center,
  containment, intersection, union, expand, translate).
- Documentation baseline (`AGENTS.md`, roadmap, decisions, changelog).

## Known issues / limitations

- Window content is a placeholder; real shell layout is task 1.1.
- `epsilon-app` does not yet depend on `epsilon-core` (nothing shared in use);
  add the path dependency in 1.1 when the app first holds engine state.
- No CI configured; verification is local only.
- No automated UI tests by design; use native launch checks.
- Project license: GPL-3.0 (repository `LICENSE`); manifests declare
  `GPL-3.0-only`.

## Resume instructions

1. Read `AGENTS.md`, `project/ROADMAP.md`, this file, and `project/DECISIONS.md`.
2. `git status` — the tree must be clean at the start of a task.
3. `cargo test --workspace` to confirm the baseline before editing.
4. Implement only task **1.1** (shell layout: menu bar, top toolbar, left tool
   panel, central canvas region, right properties panel, status bar, resize-safe).
5. Verify with `cargo fmt --all -- --check`, `cargo test --workspace`,
   `cargo build --workspace`, and a native launch:
   `cargo build && timeout 8 ./target/debug/epsilon`.
6. Update this file, `ROADMAP.md`, `CHANGELOG.md`, then commit and push.

Do not start Stage 2 canvas work before the Stage 1 shell tasks are complete.
