# Settings

Brief: a Settings screen with grouped sections, including signing tasks in with an Anthropic Console API key kept in the system keychain. DeepTutor's settings were the starting point, redrawn in Jobdesk's own look. Approved.

Only what is not built yet stays here.

- [Canvas](#canvas)
- [Pages](#pages)
- [Code home](#code-home)
- [Doc changes](#doc-changes)

## Canvas

`Settings.html?state=…&theme=light|dark`, scope `.st`, built on `Grid.html`'s shell. Each PNG is 1280x820.

| PNG | State |
| --- | --- |
| [Settings-general.png](../Settings-general.png) | Folder, start screen, notifications. |
| [Settings-appearance.png](../Settings-appearance.png) | Theme, text size, reduce motion. |
| [Settings-about.png](../Settings-about.png) | Versions, then Read more: How tasks sign in, What changed, Licences. |

## Pages

**General**: Notifications: "When Claude waits on you", on by default, and "When a task finishes", off. The OS asks for permission the first time one is sent.

**Appearance**: Reduce motion is on by itself when the OS asks for less motion.

**About**: a link to the licences.

**About, How tasks sign in**: the first setting row under Read more, before What changed. It replaces the old How Jobdesk signs in link to a doc that no longer exists, so About still says who holds the sign-in.

- Title `How tasks sign in`. Help text `Claude Code signs in on its own. Jobdesk keeps only an optional API key, in your keychain.` On Windows the help ends `in Credential Manager.` instead of `in your keychain.` The canvas draws the macOS copy.
- Control: the link `Open Providers`, in the same link look as Open the changelog. It is an in-app jump, not an external URL: it switches Settings to the Providers page, with Providers selected in the side list, and opens no browser. It starts no run and changes no setting.
- Why a jump and not more text: Providers already explains sign-in and holds the key, so the row stays one sentence and points there.

## Code home

- The planner picks the notifications crate and checks it on macOS and Windows.

## Doc changes

- `docs/guide.md`, the About line under Settings: add that About also has How tasks sign in, which says Claude Code signs in on its own and Jobdesk keeps only an optional API key in the keychain (Credential Manager on Windows), with Open Providers to go to the Providers page.
