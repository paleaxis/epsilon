# AGENTS.md — Epsilon

Permanent instructions for any agent (or human) working on Epsilon. Read this file,
`project/PROJECT_STATE.md`, `project/ROADMAP.md` and `project/DECISIONS.md` at the
start of every session.

## What Epsilon is

Epsilon is a fast, lightweight, offline-first native desktop whiteboard written
entirely in Rust. It is inspired by Excalidraw but is an independent
implementation. The long-term vision includes notes and knowledge management, but
that is explicitly out of scope until the whiteboard is genuinely usable.

## Hard technical constraints

- All application logic and UI in Rust. No JavaScript/TypeScript, no Python, no
  Node.js, no Electron, no Tauri, no WebView, no browser engine, no external server.
- Native desktop app, Linux first. UI framework: `egui` + `eframe`.
- Offline-first. No telemetry, no ads, no cloud dependency, no accounts.
- Lightweight: embed assets at compile time where practical; keep dependencies
  small and justified.
- Do not claim the binary is fully statically linked unless verified.

## Architecture rules

- **`epsilon-core` must never depend on `egui`, `eframe`, or any windowing/
  rendering crate.** This is enforced by the workspace layout: enable engine work
  only through core types.
- `epsilon-app` owns windows, widgets, input translation and rendering. It talks
  to the engine through core APIs; the engine must not know about widgets.
- Documents are structured data (elements, styles, bounds), never screen pixels.
  Rendering is a projection of the document.
- Planned boundaries: app shell, UI, canvas engine, document model, elements,
  rendering, input, selection/transforms, undo/redo, serialization, storage,
  import/export.
- Do not introduce a trait/abstraction until a second implementation or a test
  seam demands it.

## Conventions

- World coordinates: `f64`, Y axis pointing down, one world unit = one logical
  pixel at 100% zoom. Convert to `f32` only at the rendering boundary.
- World/viewport logic in core is pure and unit-tested; UI code does layout and
  drawing only.
- IDs: stable strings generated once per element; never derive identity from
  position or array index (array order is z-order).
- Angles in radians, clockwise positive (consistent with Y-down).
- Production code must not `unwrap`/`expect` on user input, file contents, or
  other fallible external data. Model errors explicitly; no silent `let _ =`.
- `unsafe` is forbidden via crate lints. Do not add crate-level lint suppressions
  to evade this.
- Follow `rustfmt`; keep clippy clean for touched code.
- All UI colors come from `epsilon-app/src/theme.rs`. Widgets read
  `egui::Visuals`; do not hard-code `Color32` values in shell/widget code. Keep
  the palette contrast tests passing when changing colors.
- Comments explain *why*, not *what*. Do not leave dead code, stub UIs, or
  placeholder features presented as complete.

## Dependency policy

- Add a dependency only in the task that needs it, and record why in
  `project/DECISIONS.md`.
- Declare shared versions in `[workspace.dependencies]`; use `default-features =
  false` where it meaningfully shrinks the graph.
- Prefer mature, small crates under licenses compatible with the project's
  GPL-3.0-only license (MIT/Apache-2.0 are compatible). No web/JS-based crates.
- Current runtime deps: `egui`, `eframe` (glow backend). See DECISIONS.md for
  researched candidates for later stages.

## Testing and verification

- `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo build
  --workspace` must pass before a task is complete.
- Unit-test core logic: geometry, coordinate transforms, serialization,
  transforms, undo/redo, export math.
- UI changes require a native launch check (`timeout N ./target/debug/epsilon`)
  and, where meaningful, manual inspection. Never install Playwright, browser
  automation, or Node-based tooling.
- Record the exact commands run and their real results in `PROJECT_STATE.md`.
  A feature is complete only when verified; code existing is not completion.
- Do not weaken or delete tests to make a build pass.

## Scope protection

Out of scope until the whiteboard is complete: AI features, cloud sync,
accounts, collaboration, plugins, task/notes/knowledge management, mobile, web.
Do not add polish for those.

## Workflow — one task per agent run

1. Read AGENTS.md, PROJECT_STATE.md, ROADMAP.md; DECISIONS.md when relevant.
2. Inspect `git status` and the actual source; trust the repository over notes.
3. Pick the first incomplete roadmap task (or the one the user names). Do not
   start a second task.
4. Implement it, tests included.
5. Run format check, tests, build; fix regressions you introduced.
6. Update `PROJECT_STATE.md`, `ROADMAP.md`, `CHANGELOG.md`, and `DECISIONS.md`
   when a decision was made.
7. Commit the verified work; push if authentication and connectivity allow.
8. Report: task ID, what changed, files, commands/results, commit hash, push
   status, known issues, next task. Then stop.

If forced to stop mid-task, leave the tree buildable where possible and write an
accurate checkpoint instead of claiming progress.

## Git rules

- Conventional commit subjects: `feat(scope): …`, `fix(scope): …`,
  `test(scope): …`, `docs: …`, `refactor(scope): …`, `chore: …`.
- Commit only files related to the task. Never commit secrets, keys, tokens,
  build output, or unrelated changes.
- Never force-push, rewrite published history, or run destructive git commands
  without explicit permission.
- Never claim a push succeeded unless git confirms it. If push fails, keep the
  local commit and record the failure in PROJECT_STATE.md.

## Environment notes

- Rust 1.100 on Arch Linux; egui/eframe 0.37 (requires Rust ≥ 1.95).
- eframe 0.37 API: implement `eframe::App::ui(&mut self, ui: &mut egui::Ui,
  frame: &mut eframe::Frame)`. Panels take `&mut Ui`
  (`CentralPanel::show(ui, …)`), not `&Context`; use `egui::Panel::top/left/
  right/bottom` (the old `SidePanel`/`TopBottomPanel` types are gone).
- Shell dimensions belong in `epsilon-app/src/layout.rs`; headless layout tests
  use `egui::Context::run_ui` with an explicit `RawInput::screen_rect`.
- Native launch check: `timeout 8 ./target/debug/epsilon` → exit 124 means it
  stayed alive; any other exit is a failure to investigate.
