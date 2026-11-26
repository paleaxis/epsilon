//! Epsilon core: UI-independent document model, geometry and canvas engine.
//!
//! This crate deliberately has no dependency on `egui`, `eframe` or any other
//! windowing/rendering crate. The application layer (in `epsilon-app`) owns the
//! UI and translates user intent into operations on the structures defined here.
//!
//! Module layout (added as the roadmap progresses):
//!
//! - [`geometry`] — 2D primitives shared by every layer (done, Stage 0).
//! - [`document`] — document envelope, elements and their styling (Stage 3).
//! - [`viewport`] — camera, zoom and screen/world transforms (Stage 2).
//! - [`history`] — transaction-based undo/redo (Stage 4).
//! - [`clipboard`] — portable clipboard payload and paste preparation (Stage 4).
//! - [`format`] — native `.epsilon` file envelope (Stage 5).
//! - [`recovery`] — rotating crash-recovery autosaves (Stage 5).
//! - [`recent`] — bounded recent-document paths (Stage 5).
//! - [`excalidraw`] — Excalidraw import/export with preserved extras (Stage 5).
//! - `export` — PNG/SVG export (Stage 6).

pub mod clipboard;
pub mod document;
pub mod excalidraw;
pub mod format;
pub mod geometry;
pub mod grid;
pub mod history;
pub mod recent;
pub mod recovery;
pub mod viewport;
