# Profile

This page covers how the profile reaches the `Snapshot` as topics, how Save writes one block, and how Updated marks are set. The features it names are mapped in [ARCHITECTURE.md](../../ARCHITECTURE.md#features). How profile text renders is in [trust](trust.md#profile-text).

## Topics

The profile reaches the `Snapshot` as topics. `TOPICS` in `profile/domain/topic.rs` names each topic's sections by the headings of the upstream templates. From `CLAUDE.md`, only the `###` blocks under `## Candidate Profile` join their topics.

A topic's state is the first that matches:

| State | When |
| --- | --- |
| Hidden | A file that feeds it cannot be read. |
| Not started | Its sections show nothing but headings, blank lines and comments. |
| N to fill | `placeholders` counts N tokens. |
| Filled | `placeholders` counts no token. |

So a fresh checkout reads N to fill for every topic.

Insights and Setup count topics, not files, through `topic_counts`.

## Save

Save goes through `ProfileEditor`:

1. While a run works, it returns before reading anything.
2. Otherwise it reads the file again, finds the block by its heading and compares it with the text the editor opened.
3. When the block changed or is gone, it writes nothing and the editor shows the conflict, until the user chooses Save mine over it.
4. Then it replaces only that block's bytes, in the file's own line endings, and writes through `ProfileFiles`.

### Line endings and the byte order mark

- The editor holds the block with LF line endings, because a text field draws a CR as nothing.
- The check turns both the opened text and the block on disk to LF, so a change of line endings alone is no change.
- A byte order mark at the start of the file stays outside every block, so it stays at byte 0.

### What `ProfileFiles` refuses

`ProfileFiles` is `profile`'s infrastructure, and it knows only the fixed profile paths under the root. It refuses:

- a file whose links resolve outside the root;
- a read-only file;
- a file the current user cannot open for writing, as an access list can deny, or as on Windows another program holds open without sharing. This check runs on every system.
- on Unix, a file another user owns, because the rename would hand it to the current user.

### The write

`ProfileFiles` writes a copy beside the file, syncs it and renames it over the file. So a crash leaves the old text or the new, never half.

When the file exists, the copy takes its permission bits. When it does not exist yet, the copy is created with the usual mode for a new file.

That is one plain rename, never retried. A program that writes the file during a wait would be overwritten unseen, so a file held open is a failure the user sees. `settings.json` and `decisions.json` go through `rename_over` instead, which tries a held file again only on Windows: see [settings](settings.md#how-settingsjson-and-decisionsjson-are-replaced).

### Save copies

Each save names its copy `<file>.jobdesk-save-<process id>-<number>` and creates it only where nothing is. So two Jobdesk windows saving one file at once never write into or remove each other's copy.

- A save removes its own copy when it fails.
- A save interrupted before its rename, by power loss or a force quit, leaves its copy. That copy holds the profile's text and escapes a `.gitignore` line naming the profile file.

So each save first removes the copies of the same file that:

- are named exactly like one;
- are regular files, not links, by `symlink_metadata`;
- were last changed over an hour ago.

A save takes well under a second, so a live save loses its copy this way only when it stalls for an hour. Then its rename fails, and it reports the failure.

### Accepted limits

- The rename does not compare again. When another program, such as an editor, a sync tool or a `claude` session in a terminal, writes the file between the compare and the rename, Save replaces that write without warning. The guard on a working run covers only Jobdesk's own runs.
- ACLs, extended attributes and other hard links do not follow.
- The root check runs once, before the write. The standard library has no folder-relative open, so a folder swapped for a link between the check and the write could still send the write outside the root.

## Updated marks

Updated marks compare the profile in the `Snapshot` when a run starts working with the first `Snapshot` read after it stops, by `read_at`. When none arrives within `RUN_READ_WAIT`, five seconds, the Shell reads the profile files itself and compares that.

- A change made outside Jobdesk while a run works is marked too.
- A Save never is, because Save waits through the run and that read.
- `UpdatedMarks` keeps the marks in memory only.
- Opening a topic clears its rail mark.
