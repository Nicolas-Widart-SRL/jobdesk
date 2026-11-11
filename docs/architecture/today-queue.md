# Today queue

This page covers how `intray` deals Today's cards, how it records decisions and skip reasons, and how Later sets a card aside. The features it names are mapped in [ARCHITECTURE.md](../../ARCHITECTURE.md#features). When a run gets a Your turn card is in [runs](runs.md#the-your-turn-card).

## Card actions run nothing

No card action runs anything by itself. Y opens the launcher on the slash command, and the user starts it there. Permission, question, guard and Your turn cards have no Y action.

## The queue

The queue is derived, never stored. `intray` deals cards from the latest snapshot and the live runs on every frame, then hides what the user decided.

- Paused runs rank first, then cards dated within the Coming up range or already overdue, by date, so overdue ones lead, then the rest.
- The card being read stays on top, so new work lands behind it.
- Cards brought back from later are the one exception: they deal on top, right after paused runs, in rank order among themselves, until each is decided. That lasts only for that day and lives in memory, not in `decisions.json`.
- Card keys change when the underlying state moves on, so a decision never outlives what it decided.

### Coming up

Coming up lists the dated cards of the Coming up range, overdue ones first. When none is dated, it says so in one line.

Settings › Today queue sets the range to 7, 14 or 30 days, 14 by default.

### When nothing is left

When nothing is left to decide, the end panel offers new work: "Find new jobs…" and, while postings wait for scoring, "Score {n} new jobs…". Both only open the launcher, so the user starts the run there.

In a new job search, `Queue::first_steps` replaces the end panel. It counts only while Today has nothing at all to show: no card, none set aside, no decision today, and every source read. While any run works it suggests nothing, since that run may be the search or the scoring.

- With no job on the board, the head reads "Nothing to decide yet" and Find new jobs… opens the launcher on `/scrape` with the note "Your first search, from Today." The subtitle calls the profile ready only when a profile topic is written.
- With every job still new, it offers Score {n} new jobs… instead. With no card on top, Y takes the offer's action, which only opens the launcher.
- The Your profile slip sits on the rail under Coming up while the profile is found, readable and not full.

The shell follows the run the launcher started from that note, in memory only. When one of its turns ends with its result and Claude asks nothing, Jobs opens with no list picked, so New shows while the jobs found are all new, but only while that run is open in Tasks. A failure, a stop, or a session ended while Claude asked opens nothing and keeps the run followed, so a resume in Tasks still counts. Opening another folder drops the note.

## Decisions

Decisions and skip reasons live in `decisions.json` in the app's data folder.

- The last decision can be undone until the next one, on the day it was made.
- Its row heads the Done today rail, which sits outside the card's scroll area, so it stays in view at any window height.
- A followed card returns the next day, so a command that never ran asks again.

### Skip reasons

The reasons a skip offers are the list in Settings › Today queue: Rate, Scope, Domain and Location by default, at most 8, each at most 40 characters.

- A skip stores its reason as text. So a reason taken off the list later still names the past skips that used it, and an older file's lowercase reason still reads.
- A reason stays optional, and with an empty list a skip asks for none.
- A skip suggested by a failed freelance gate records its reason even when the list lacks it.

## Later

Later is not a decision on the card. It sets the card aside for as long as Settings › Today queue says:

- until tomorrow, the default, which stands in for the morning pass until standing orders exist;
- 3 days;
- until Monday: the next Monday after the day L is pressed.

### The return day

The return day is worked out when L is pressed and stored on the decision, so changing the setting moves no card already set aside.

- It never falls after the card's own date, which the queue reads again each day. So a deadline after the day L was pressed never passes while its card is away, even one that appears or moves earlier after L.
- A card dated the day L was pressed or earlier comes back the day after L was pressed, whatever the setting. That holds whether it was dated so when L was pressed or the date came after.
- A decision stored before Later could last longer has no return day, and comes back the next day.

### While a card is away

- A card set aside adds no log entry, keeps its timeline mark and leaves a paused run paused.
- Its mark pulls it back to the top.
- Coming up's first row is dated with the first day a card comes back. It counts the cards set aside, names them on hover, and brings them all back on top from its button.
- That button brings back only the cards the queue holds away, so a card its date already brought back keeps its place.
- With cards set aside, a "Bring them back now" link in the end panel does the same as Coming up's button.
- Bringing them back replaces the card being read.
