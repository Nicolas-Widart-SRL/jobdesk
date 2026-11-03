# Settings

Brief: a Settings screen with grouped sections, including signing tasks in with an Anthropic Console API key kept in the system keychain. DeepTutor's settings were the starting point, redrawn in Jobdesk's own look. Approved.

Only what is not built yet stays here.

- [Canvas](#canvas)
- [Pages](#pages)
- [Code home](#code-home)

## Canvas

`Settings.html?state=…&theme=light|dark`, scope `.st`, built on `Grid.html`'s shell. Each PNG is 1280x820.

| PNG | State |
| --- | --- |
| [Settings-general.png](../Settings-general.png) | Folder, start screen, notifications. |
| [Settings-appearance.png](../Settings-appearance.png) | Theme, text size, reduce motion. |
| [Settings-about.png](../Settings-about.png) | Versions and links. |

## Pages

**General**: Notifications: "When Claude waits on you", on by default, and "When a task finishes", off. The OS asks for permission the first time one is sent.

**Appearance**: Reduce motion is on by itself when the OS asks for less motion.

**About**: a link to the licences.

## Code home

- The planner picks the notifications crate and checks it on macOS and Windows.
