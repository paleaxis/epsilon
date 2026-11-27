# Epsilon v0.1.0 — release checklist (DRAFT)

**Status: not tagged.** The automated end-to-end legs pass, but the manual GUI,
dialog and cross-session checks below have not been performed. Do not create the
`v0.1.0` tag until every unchecked item passes. This file becomes the release
notes when the tag is made.

## Environment for the automated pass

- Linux (CachyOS/Arch), niri Wayland session, Rust 1.100, 4 logical cores.
- `cargo build --release --locked --workspace`, release binary 10.7 MiB.
- Full suite: `cargo test --workspace` → 270 passed, 0 failed, 1 ignored
  (manual performance harness).

## Whiteboard flow

- [x] Rectangle, ellipse, diamond, line, arrow, freehand and text tools create
      elements that survive rendering, selection and undo.
      (`every_drawing_tool_creation_is_undoable`, drawing-tool and text tests)
- [x] Select, shift-toggle, marquee, group and right-click target the expected
      elements; move, eight-handle resize, rotation, properties and style edits
      apply to the whole selection.
      (`moving_selection…`, `resizing_selection…`, rotation, group, properties
      tests)
- [x] Every creation, drag, property/style edit, paste, delete, duplicate,
      order move, align, distribute, group and ungroup is one undo entry and
      redo restores the document exactly.
      (`mixed_stage_three_edits_undo_and_redo_exactly` and per-operation tests)
- [x] Undo/redo works from Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y and the Edit menu; the
      history branches correctly after a new edit.
- [x] Cut/copy/paste and duplicate use fresh IDs, preserve paint order and
      offset repeated pastes.
- [x] Save writes `.epsilon` atomically, the title/dirty state update, and a
      fresh app opens the same document with a clean history.
      (`save_and_open_round_trip_updates_dirty_state`)
- [x] New/Open/Save As respect the Save/Discard/Cancel guard; a cancelled Save
      As keeps the document intact.
- [x] PNG, SVG and Excalidraw exports write decodable files; selection exports
      crop to the selection; overwrite confirmation leaves the old file intact
      on Cancel.
      (`png_export_writes_a_decodable_image`, `excalidraw_export_writes_a_readable_scene`,
      export-dialog tests, core export tests)
- [x] Autosave recovery offers the newest valid file, restores the document and
      source path, and normal save/open/new/discard removes recovery files.
      (`autosave_is_offered_and_recovered_after_a_crash` and recovery tests)

## Keyboard-only pass

- [x] Tool shortcuts select exactly the intended tool and ignore modifiers
      (`keyboard_shortcuts_select_tools`, `modified_keys_do_not_change_tools`).
- [x] File and edit shortcut registrations match their actions; Esc closes the
      help overlay before clearing selection; delete/backspace and duplicate
      behave with no stale selection.
      (`file_shortcuts_are_registered`, `edit_shortcuts_are_registered`, …)
- [ ] Walk the whole app with keyboard only in a live window: menus, dialog
      controls, focus order and visible focus rings.

## Error recovery

- [x] Opening a malformed `.epsilon` file leaves the current document untouched
      and reports the failure (`open_errors_leave_the_current_document_untouched`).
- [x] A failed save keeps the document dirty and shows the error
      (`failed_save_reports_the_error_and_keeps_the_document_dirty`).
- [x] A missing startup file or recent entry reports an error without breaking
      the session.
- [x] Excalidraw import skips unsupported element types with a warning while
      importing the supported ones.
- [ ] Trigger real failures in the live UI (unwritable directory, missing
      portal, full disk) and confirm the toast/log behaviour and dismissal.

## Packaging and launch

- [x] The release tarball contains the binary, launcher assets, license and
      docs with a matching SHA-256 checksum.
- [x] The tarball binary and the installed command launch from a fresh `HOME`
      (exit 124 under `timeout`, session logged).
- [x] `install.sh` works from both a checkout and an unpacked tarball; the
      installed desktop entry validates and both MIME types map to Epsilon.
- [ ] Install on a machine without Rust and launch from the application menu
      (icon, desktop entry, file-manager double-click for `.epsilon` and
      `.excalidraw`).
- [ ] Verify a session without an XDG Desktop Portal and an X11-only session.

## Manual visual pass

- [ ] Draw and edit with the mouse at 100% and 150% scale; check handles,
      rotation, rough strokes, dashes/hatches, opacity and text wrapping.
- [ ] Open, inspect and compare a PNG export and an SVG export against the
      canvas; check selection crops.
- [ ] Open an Excalidraw export in excalidraw.com and re-import it.
- [ ] Paste a copied image into an external editor.
- [ ] Confirm the window icon and task-switcher icon match the installed SVG.

## Tagging

Tag `v0.1.0` only after the unchecked items above are ticked or explicitly
accepted as documented limitations in `PROJECT_STATE.md`.
