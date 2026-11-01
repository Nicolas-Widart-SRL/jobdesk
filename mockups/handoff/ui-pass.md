# UI pass: how clickable things behave

The user asked for a pass over the whole app: "links and clickable items dont show a hand icon. job list: job title and sub title show a cursor, line not clickable there." The fixes are built, so this file keeps the rules that guide new widgets and states. The widgets and measures named here live in `src/ui_kit/`.

## Rules

- **Every clickable kit widget calls `interactive` last.** It shows the hand while hovered and enabled, paints the pressed state and draws the focus ring. egui's own controls on a surface (checkboxes, combo boxes) set the hand where they are added.
- **Hover is a fill, selected is a fill and an edge.** Hover takes `raised` everywhere. Selected adds a `PICK_EDGE` bar in `text` on a row, or a `text` hairline on an index card. All pickable rows and cards go through `pick_look`, so the two states never match, on paper and on a slip, in both themes.
- **Pressed is one step deeper than hover**: `line` for plain and danger buttons, `muted` for the primary, the `mark` swipe without its rule for a tab, `raised` for a segment.
- **Disabled is painted by the kit at full opacity**: the opaque rest fill, with ink and border at `DISABLED_OPACITY`. The grid never runs through a control. A disabled control keeps the arrow and always says why on hover.
- **One link style** (`link`): `active` ink, underlined at rest, a thicker rule on hover, `text` when pressed, a `MIN_HIT` target and the focus ring.
- **Labels are not selectable**, so clicks on a row's text reach the row and the row keeps its hover. Text that must stay copyable (Claude's documents, code boxes, the archived posting, drafts) goes in the `copyable` scope, and a copyable text never sits inside something clickable.
- **A pickable row is one target.** A table row senses the click once (`pick_table`), and its cells only hover. Tab moves one row at a time.
- **A row that does nothing takes no click.** Coming up's Now row has no hover fill and keeps the arrow.
- **Loading and failed are not empty.** Every screen gets a loading view and a failed view. Loading draws the head with "Reading your …" and the body's shape in skeleton blocks. Failed puts the alert under the head and says the content is hidden, not gone. Empty uses the one `empty_state`, with actions in plain words, never a raw command.
- **Cut text shows in full on hover** (`cut_hover`), and a mark with a spoken label shows the same words on hover.
- **A column that scrolls shows its bar at rest.**
- **Copy says Copied** for a moment, at the same width (`copy_button`).
- **Actions that open the launcher end in "…"**, and one action has one name everywhere.

## Canvas

`UiPass.html`, chosen with `?scene=states|jobs|failed|loading|empty&theme=light|dark`. Cursor-only rules have no canvas, and the drawn pointers stand for the OS cursor.

| PNG (1280x820) | State |
| --- | --- |
| [UiPass-states.png](../UiPass-states.png) | Every clickable kit widget at rest, hover, pressed, focus and disabled, with its cursor. Rows and cards at rest, hover, selected and focus. Coming up with a cut row's tooltip and a Now row that takes no click. Copy turned to Copied. |
| [UiPass-states-dark.png](../UiPass-states-dark.png) | The same sheet in dark, where selected must still show on a slip. |
| [UiPass-jobs.png](../UiPass-jobs.png) | Jobs: the hand and the row's hover over a job title, the full title in a tooltip, the picked job's edge, and the detail's actions pinned under its body. |
| [UiPass-applications-failed.png](../UiPass-applications-failed.png) | Applications, unreadable: the alert under the head and a "hidden, not gone" panel. The tab carries the failed square. |
| [UiPass-applications-loading.png](../UiPass-applications-loading.png) | Applications before the first read: the lane names, blank cards in skeleton blocks, and "Reading your applications…". |
| [UiPass-tasks-empty.png](../UiPass-tasks-empty.png) | Tasks with no run yet: the shared empty state, with Find new jobs… and New task…. |
