# Escapes

Bugs that got past review and that CodeRabbit caught. Look for each class in every range you review.
A builder appends new escapes in its commit. The reviewer never edits this file.

## Classes

- **State over time**: a day rollover during a load or while the app is open, undo restoring every field, partial and interrupted runs, a stop between steps.
- **Untrusted text**: posting, note, report and tool text shown without stripping control, escape, bidi and zero-width characters, or split on fewer line breaks than it holds.
- **Copy claims more than the data**: a label or sentence that says more than the data behind it, in the empty, one or many case.
- **Hover/focus/keyboard**: hover, focus and selected states that look the same, or a control or tooltip that the keyboard cannot reach.
- **Contradiction between steps**: two steps or rules in one instruction file that cannot both be followed, such as a read limit against a step that needs the file.

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
- PR 11 `.claude/agents/designer.md`: Rounds section could copy real job-search values from feedback into a committed README. Class: untrusted text.
- PR 11 `.claude/skills/ship/SKILL.md`: an interrupted CodeRabbit answer reran the builder or posted duplicate replies; a review id counted as handled after one of its findings. Class: state over time.
- PR 11 `scripts/pr-feedback.sh`: output and count silently cut at 100 threads, 100 reviews and 20 comments. Class: copy claims more than the data.
- PR 18 `.claude/agents/designer.md`: step 1 limited reads to "only" handoff files and touched canvases while step 2 required `Grid.html`, `app.css` and `support.js`. Class: contradiction between steps.
- PR 17 `.claude/skills/ship/SKILL.md`: comments posted between opening the pull request and the watch call never woke the lead. Class: state over time.
- PR 17 `.claude/skills/ship/SKILL.md`: a user comment starting with "Not changed" on a judged finding was taken as the lead's reply and never judged. Class: state over time.
- PR 21 `src/settings/infrastructure/preferences_file.rs`: atomic replace renamed the temp file without sync_all, so the crash-safety doc comment overclaimed. Class: state over time.
- PR 21 `src/workspace/ui.rs`: runtime_status returned an Option that was always None while the shell still matched on it. Class: dead signature.
- PR 21 `src/shell/app.rs`: Change folder from Settings (and Setup) replaced the RunDesk while a task was live, dropping its session. Class: state over time.
- PR 21 `src/settings/infrastructure/preferences_file.rs`: atomic replace synced the file but not the parent directory after rename. Class: state over time.
- PR 22 tests/architecture.rs: a new scanning helper landed with no fixture proving it changes the outcome. Class: untested guard.
- PR 24 src/ui_kit/marks.rs: heatmap day nodes sensed hover only, so keyboard focus never reached them. Class: hover/focus/keyboard.
- PR 25 `src/profile/infrastructure/profile_files.rs`: the save copy was created with the default mode and took a private target's bits only after the text was in, so another user could open it meanwhile. Class: state over time.
- PR 25 `ARCHITECTURE.md`: Save's unchanged-block check read as a compare-and-swap, though another program can write the file between the check and the rename. Class: copy claims more than the data.
- PR 25 `src/profile/infrastructure/profile_files.rs`: every save used one fixed copy path and removed a copy found there as stale, so a second Jobdesk saving the same file at once could remove the first save's copy and have its own renamed in, losing the first text while it reported Saved. Class: state over time.
