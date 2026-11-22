# Changelog

Notable changes, newest first. Version sections are added at release time.

## [Unreleased]

### Added — 2026-11-22 (task 0.1)

- Cargo workspace with an egui-independent `epsilon-core` crate and an
  `epsilon-app` crate producing the native `epsilon` binary.
- `epsilon-core::geometry`: `Point` and `Rect` primitives with 11 unit tests.
- Native application bootstrap using eframe 0.37 (glow backend, Wayland/X11),
  1280×800 window with a 720×480 minimum.
- Project documentation: `AGENTS.md`, `README.md`, `project/PROJECT_STATE.md`,
  `project/ROADMAP.md`, `project/DECISIONS.md`, this changelog.
- Tooling baseline: rustfmt check, workspace tests, clippy with `-D warnings`,
  and a native launch smoke check.

### Notes

- No whiteboard features exist yet; the window shows placeholder content until
  Stage 1 layout work.
- Manifests declare the repository's existing GPL-3.0 license (`GPL-3.0-only`).
