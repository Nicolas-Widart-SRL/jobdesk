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

## A task in plain words

Most tasks run a slash command from the catalog. A screen can also open the launcher on a task written in plain words, such as a request to read a portal's code before it installs. The launcher then works like this:

- `Launcher::prefill` opens a trimmed line that does not start with `/` as a task in plain words. A `/` line the catalog does not hold, and a task with a hidden character (see [trust](trust.md#rendering-untrusted-text)), change nothing and open no launcher.
- The form shows the text read only under "Claude will run", with no fields, and Run sends it as the prompt, unchanged. The task is dropped once it starts, when the user picks a command, when the launcher opens with no line (New task, Cmd K, or New task on the Tasks screen or in Settings) and when the folder changes, so Cmd Enter never repeats it unseen.
- Model and Spend come from the run defaults for every task, else `WORDS_BUDGET_USD`. No command's row applies.
- Run waits on the same rules as a command: one run at a time, Test connection, the monthly cap and a budget above zero. Nothing runs until the user presses Run.

Usage counts such a task with no command, and the band's place reads "Task" in place of a command word.

## Finding `claude`

`claude` is looked for in this order:

1. on the `PATH`;
2. in `.local/bin` of the user's profile folder;
3. in `.claude/local` of the user's profile folder;
4. on Windows, in npm's `%APPDATA%\npm`.

On Windows the profile folder is `USERPROFILE`, with `HOME` only when it is unset, and each folder is tried for `claude.exe`, `claude.cmd` and `claude.bat`, in that order.

## The `PATH`

A task's process, Test connection, the tool check and Runtime status all get one `PATH`, looked up once per launch by `host_path::resolved_path`. It holds, in this order:

1. on macOS, the folders of the helpers first-run setup installs, all in Jobdesk's own `~/.jobdesk`: `~/.jobdesk/python/bin`, `~/.jobdesk/bun/bin` and `~/.jobdesk/TinyTeX/bin/universal-darwin`. They are listed whether or not they exist yet, so a helper installed after the lookup is still found. When first-run setup hands over, the tool check runs again, so Setup and Runtime status find what it installed. They come before every other copy, newer ones and the login shell's own included, such as a Homebrew Bun or a MacTeX: setup installs a helper only when no copy works with ai-job-search, and then only its pinned one is known to work with the templates and tools, so once installed it shadows the others;
2. the `PATH` the login shell sets, since an app opened from the Finder gets none of the shell profiles' `PATH`;
3. the `PATH` Jobdesk inherited.

An empty part is left out, as it would name the current folder.

On macOS, `/usr/bin/python3` and `/usr/bin/git` only hand over to Xcode or its Command Line Tools. With neither installed, running one opens a dialog that offers to install them. Looking for a program skips such a stub, so the tool check counts Python as missing and runs nothing. The stub counts as backed when `DEVELOPER_DIR`, else the folder `xcode-select` chose, else Xcode's or the Command Line Tools' usual folder, holds the program.

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
- The lock on the held key is never kept across a keychain call. While a task's start reads, the API key section of Providers says Waiting for the keychain…, offers no Save key, Replace, Remove or test, and looks again a little later. It does so too over a failure it showed before, because that read may replace it, and Save key or Remove would write the keychain under its prompt. A test of a key not held yet says to wait. So the window never waits on that prompt, and a key saved in the keychain is never replaced unseen.
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

## Live figures

The Tasks screen shows three figures that change while a run works: the time since the last update, Elapsed and Cost. All three come from `Run` and the `runs::application` story, never from the disk.

### The last update

- Any line the CLI writes counts as an update, whatever it holds.
- `claude_gateway` sends `AgentEvent::Heard` for a line that gives no event of its own, at most once a second, so a stream of silent lines does not flood the channel. `RunDesk` stamps `Run::last_heard` on every event it receives, and on a send, a reply or an answer to a prompt (`decide`), which also settles the clock.
- `runs::domain::quiet` ranks the time since then: fresh under 30 s, quiet from 30 s, long quiet from 2 min. The band's Whose turn cell and its sweep follow that rank.
- The cell's screen reader label names the rank without a number ("Last update under 30 seconds ago"), so it changes only when the rank does, not each second.

### Elapsed

- `Elapsed` measures the clock from the user's send to the run's end, in spans. It never reads the clock: `RunDesk` passes `now` from its own clock, which a test can script.
- The clock runs while the run is busy or `wants_reply` holds, so it runs on while Claude waits on the user. It holds when the turn ends done for now, and when the run fails, stops or closes.
- A later reply opens a new span and adds it to the held figure.
- A run read back from its log starts from the sum of its turns' `duration_ms`, which Claude reports, and has no open span until the user replies. The log keeps no timestamps, so the clock time of the user's turns and of the gaps is lost: a run that read 9:31 live can read less after a restart or a folder switch, and a run left waiting overnight drops from hours to Claude's minutes. A later reply adds clock time on top of the restored figure.
- The screen shows it as `m:ss`, or `h:mm:ss` from an hour.

### Cost

- `StreamReader` estimates each message from its token use and one list price per family: Opus, Sonnet or Haiku, whatever the version, and Opus for a model of no known family. The estimate is approximate in either direction: an older Opus with a higher price reads low, a cheaper unknown model reads high. The CLI's figure replaces it at the turn's result.
- The tokens come from `message_start`, `assistant` and `message_delta` lines. The reader merges them by message id and keeps the larger figure of each count, because a message's figures only grow. The final usage of a message arrives in its `message_delta`, which names no model, so the reader prices it by the model it saw for the id. A `message_delta` for a message the reader never saw start is dropped, so the estimate can read low there but never counts a message twice.
- `Run::shown_cost` adds what the CLI reported, the estimates of earlier processes that ended without a result, and the estimates of the turn under way. It is marked an estimate while any estimate is in it, and the screen shows "About $X". `story::cost_note` picks the line under it: "Includes an estimate: a session ended without its final cost." while an ended process's estimate is in the figure, since that part never turns exact, and otherwise "Estimated until Claude's turn ends."
- A turn's result replaces that turn's estimates with the CLI's cost.
- A process that ends with no result keeps its estimate. A resumed run adds the cost of each process, because each process reports its own running total from zero.
- A run read back from its log has no live process, so `settle_after_restore` counts its last process as ended, and a reply that starts a new process notes the old one's end again (`Run::note_process_start`), which is safe to repeat. The cost then adds up the same live and after the next restart.

### Frames

The figures repaint only while the run screen shows. The screen asks for a frame about 30 times a second if the sweep moves, and otherwise when the first whole second shown turns over. A sweep that stands still, under reduced motion, still changes form 30 and 120 seconds after the last update, so the screen also asks for a frame at the next of those, whatever else shows. When nothing on screen counts the seconds and no quiet step is ahead, it asks for none.
