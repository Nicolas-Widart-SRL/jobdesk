# Escapes

Bugs that got past review and that CodeRabbit caught. Look for each class in every range you review.
A builder appends new escapes in its commit. The reviewer never edits this file.

## Classes

- **State over time**: a day rollover during a load or while the app is open, undo restoring every field, partial and interrupted runs, a stop between steps.
- **Untrusted text**: posting, note, report and tool text shown without stripping control, escape, bidi and zero-width characters, or split on fewer line breaks than it holds.
- **Copy claims more than the data**: a label or sentence that says more than the data behind it, in the empty, one or many case.
- **Hover/focus/keyboard**: hover, focus and selected states that look the same, or a control or tooltip that the keyboard cannot reach.

## Escapes

- PR 3 `src/intray/application/in_tray.rs`: undo did not restore a returned card's priority. Class: state over time.
- PR 3 `mockups/app.css`: set-aside tooltip not shown on keyboard focus. Class: hover/focus/keyboard.
- PR 3 `mockups/TrayHorizon.html`: returned cards went to the back of the queue, not the front. Class: state over time.
- PR 5 `src/shell/live_snapshot.rs`: no reload when the snapshot date changed during the load. Class: state over time.
- PR 6 `src/runs/infrastructure/stream_json.rs`: failure reasons not stripped of control characters before display. Class: untrusted text.
- PR 8 `src/runs/application/plain_text.rs`: `notice_text` not split on lone carriage returns. Class: untrusted text.
- PR 8 `src/runs/application/story.rs`: completed stop points lost; partially closed steps shown as complete. Class: state over time.
- PR 9 `src/dossier/ui.rs`: copy described every keyword as found in the CV. Class: copy claims more than the data.
- PR 9 `src/tracking/ui.rs`: expired offer reply date not marked passed. Class: state over time.
- PR 9 `src/tracking/ui.rs`: custom closed statuses dropped from card metadata. Class: copy claims more than the data.
- PR 9 `src/ui_kit/controls.rs`: hovered `status_segmented` segment fill same as the selected fill. Class: hover/focus/keyboard.
