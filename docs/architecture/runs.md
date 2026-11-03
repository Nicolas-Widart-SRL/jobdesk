# Runs

This page covers a task's `claude` processes: how one starts, stops and signs in, how Test connection differs, and when Claude waits on the user. The features it names are mapped in [ARCHITECTURE.md](../../ARCHITECTURE.md#features).

## The child process

A run's `claude -p` child process speaks the stream-json protocol over stdin and stdout. A run has at most one process at a time, but not always the same one:

- A reply while the run's process is up goes to that process.
- A reply to a run whose process has ended starts a new process, which resumes the same Claude session with `--resume`. A process ends when it exits or fails, or when the user presses End session or Force stop. Stop run only sends an interrupt, and does not end the process.

Jobdesk drives each process like this:

- A supervisor thread owns the child, and a reader thread turns stdout lines into domain events.
- The UI thread applies them once per frame through `RunDesk::poll`.
- Dropping a session ends its process.

## Finding `claude`

`claude` is looked for in this order:

1. on the `PATH`;
2. in `.local/bin` of the user's profile folder;
3. in `.claude/local` of the user's profile folder;
4. on Windows, in npm's `%APPDATA%\npm`.

On Windows the profile folder is `USERPROFILE`, with `HOME` only when it is unset, and each folder is tried for `claude.exe`, `claude.cmd` and `claude.bat`, in that order.

## Stopping a run and Windows system programs

On Windows a run stops with `taskkill /T /F`, which ends the tools it started too.

`taskkill` and `explorer`, which shows the job search folder or the data folder, start by their full path under `SystemRoot`, or `C:\Windows` when that is unset or empty. So no program of the same name in the app's folder runs instead. When the inherited `SystemRoot` names no `taskkill`, the stop tries `C:\Windows` once.

## No console windows

Every console program Jobdesk starts goes through `no_window`, so it opens no console window. `tests/architecture.rs` counts the process starts in each file against its `no_window` calls.

Two starts are exempt: Explorer, a window program, and the login-shell `PATH` lookup, which runs on Unix only.

## Sign-in

Tasks sign in one of two ways, the sign-in method `settings` keeps:

- **The Claude sign-in**, the default: the `claude` CLI uses whatever authentication it already has.
- **An API key** kept in the system keychain: the macOS Keychain or the Windows Credential Manager.

`settings.json` holds the method and the last key test: whether it passed, when, and what it cost. It never holds the key.

### How the method changes

- Save key writes the key to the keychain, puts tasks on the Claude sign-in and tests the key at once.
- When a task's process is up or another test is finishing, Save key does not test. The key waits at Not tested yet until Use for tasks or Test connection.
- Tasks switch to the key only after a passed test.
- A refusal moves them back to the Claude sign-in.
- A test that never got an answer about the key changes nothing. Examples: a timeout, a missing CLI, or another sign-in Claude Code chose.

### How a task gets the method

`Providers` reads the keychain once per launch, until a read succeeds, and never per frame. The first of Providers showing, a key test and a task's start under the key reads it. The key then stays in process memory for the session, and the mask, Test connection and every task use it:

- A read that fails, such as a denied prompt, is not held, so the next task's start or test asks the keychain again. Providers showing does not: it shows the failure. No key saved is an answer, and is held.
- Two reads at once ask the keychain once and share its answer.
- The lock on the held key is never kept across a keychain call. While a task's start reads, the API key section of Providers says Waiting for the keychain…, offers no Save key, Replace, Remove or test, and looks again a little later. A test of a key not held yet says to wait. So the window never waits on that prompt, and a key saved in the keychain is never replaced unseen.
- Save key replaces the key held, and Remove drops it once the keychain item is deleted. Both write the keychain, so they may prompt too.
- A read that ends after a save or a remove leaves what they wrote in place.

Under the key method:

- a key not read yet, or unreadable, still gives the key, so a task fails with the reason instead of quietly billing the Claude plan;
- a key known to be absent gives the Claude sign-in.

The shell gives every desk it opens the method through `RunDesk::set_sign_in`. The method holds for each process started after it, never for one already running.

### The key in a run

`RunRequest` carries the method only, never the key.

- The gateway gets the key through the shell's `KeyReader` on the supervisor thread, right before it spawns the process, once per process. The reader gives the key `Providers` holds, and reads the keychain only when nothing read it yet this launch.
- After the spawn it keeps the key only to mask it in the process's output. It drops the key once the process ended and its output was read.
- A key that is gone or cannot be read ends the run with the reason, and that process never starts. It never falls back to the Claude sign-in.

Where else the key may go, and how it is masked: [trust](trust.md#the-api-key).

## Test connection

Test connection is not a run, so the monthly cap does not apply to it. It runs one `claude -p`:

- on Haiku, with a fixed prompt;
- with no tools: `--tools ""` and `--strict-mcp-config`;
- with `--setting-sources project`, `--no-session-persistence` and `--max-budget-usd 0.01`;
- in an empty temporary folder, removed once it ends;
- in its own process group, killed after a minute with everything it started.

It takes no data folder, so it writes no run log: Tasks never lists it and Usage never counts it.

It waits while a task or another test runs, so one Claude process runs at a time.

A test of the key uses the key held in memory, or reads it from the keychain when nothing read it yet this launch. A test of the Claude sign-in passes no key and changes no method.

## When Claude waits on the user

Claude waits on the user when:

- a prompt is pending, even while the run stops;
- a turn ended with a known stop point;
- a turn ended with a closing question: the last top-level paragraph of its last message, ending in "?".

A turn that ends with neither a stop point nor a closing question is done for now, wherever the rail stands:

- the band says nobody's turn;
- a step left part-way reads paused and does not count as done;
- the reply field stays under the story.

`runs::application::wants_reply` holds that rule. It drives the count on the Tasks tab, and the band's Yours cell and waiting step.

### The Your turn card

`intray` deals a Your turn card from `turn_card`, which follows `wants_reply` with rules of its own. It deals none:

- for a pending prompt, which gets the paused run's card;
- while another run works;
- once every step is done.

The card's only action is Open task.

### Pending prompts and background subagents

- A pending prompt stays until it is answered or withdrawn, or the process ends. A turn's end does not clear it, because a background subagent can still be waiting on it.
- Background subagents keep the run busy after its turn ends, because they can still write files.

## The step rail

The rail is display only and never steers the run. It infers the step from what a tool does:

- file tools by path;
- scripts by the program actually run;
- agents by type.

Each activity row keeps the step the rail was on when it arrived, so the Tasks screen folds rows under their step.

Each step's plain sentence comes from a fixed phrase table in the command catalog (`catalog::phrases_for`), keyed by tool and path. It never comes from Claude's text or a posting's.

The technical rows open with "Show N actions", or "Show details" for a step with no actions but other folded rows.

## Replies

- Quick replies are fixed text from the command catalog (`catalog::stop_points`). They are sent through the same path as a typed reply, on the same session.
- Quick replies never start a run. They stay disabled while another run works or the reply field holds a draft.
- `RunDesk::reply` refuses any reply while another run is busy, even on a live session.
- End session asks first, and Esc keeps the session open.
