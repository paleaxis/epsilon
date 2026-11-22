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
//! - `history` — transaction-based undo/redo (Stage 4).
//! - `io` — native document format and Excalidraw import/export (Stage 5).
//! - `export` — PNG/SVG export (Stage 6).

pub mod document;
pub mod geometry;
pub mod grid;
pub mod viewport;
