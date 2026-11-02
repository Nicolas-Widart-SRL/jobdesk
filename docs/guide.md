# Jobdesk guide

How each screen of Jobdesk behaves. To install the app, see the [README](../README.md).

Jobdesk is a native macOS and Windows view over an [ai-job-search](https://github.com/ai-job-search-dev/ai-job-search) checkout, built with Rust and egui.

## Today

Today is the home screen: one queue of decisions, one card at a time, with paused runs first, then deadlines, then the rest. The day stamp counts what you decided today, and Coming up lists the next two weeks' deadlines, with a button to bring back what you set aside.

Y opens the suggested task, L sets the card aside until tomorrow (Coming up shows how many, and its button brings them back to the front, after any paused run), N skips, U undoes the last decision. Until your next one, and only that day, it waits at the top of Done today, where a skip also takes its optional reason. A followed card's note names the task it opened, for example "Task opened: draft application".

The card's actions stay pinned under it, and Show the command reveals the command line a task would run. Y opens the slash command in the launcher, and nothing runs until you start it. Permission, question, guard and Your turn cards have no Y action. Skip reasons stay in the app's data folder.

When nothing is left to decide, Today offers Find new jobs…. It shows a loading view while your files first load, and a welcome with three steps until you choose a folder. Its second step points to Profile, which shows each topic, what is filled in and what is missing.

## Jobs, Applications and Insights

Today, Jobs, Applications, Insights and Profile update live as commands change your files. A file that fails to load raises one alert that names what failed and what still works, and marks its tab. The screens it feeds say their content is hidden until the file is fixed, never that it is empty.

Click anywhere on a job, an application or a run to open it. On Jobs, Up and Down move to the next job while a row or nothing has the focus, and Tab stops once on each row. A cut title shows in full under the pointer. Below 1100 pixels wide, the table leaves out Found on, which the detail still shows.

Insights' Profile stage counts how many profile topics are filled, not files, and says how many are hidden when a file cannot be read.

## Application dossier

Open an application from Applications for its dossier: compiled PDFs, verification results, keyword coverage, evaluation, archived posting, prep packs, outcome notes and follow-up drafts. Verification, keywords and evaluation exist only in `/apply` output, so they appear for applications run through Jobdesk.

## Tasks

The Tasks screen starts any slash command through your local `claude` CLI and shows its progress, questions, cost and history. Jobdesk allows tool permissions by default and lists each one in the activity. Settings › Permissions turns prompts on.

A run reads like a report: Claude's messages come first, and each step is one plain sentence, with its actions a click away. A message's text keeps to a readable column, and a table that stands on its own uses the whole width of the run, scrolling sideways only when wider still. A table inside a list or a quote keeps to the column, and so do a message's tables when a footnote points across one. A band at the top says where the run is, whose turn it is and what comes next.

When Claude waits on you, the Tasks tab counts it, Recent runs tags the run Your turn, Today deals a card that opens the task, and Claude's question sits above the reply box, with one-click replies at known stop points such as /apply's proceed with drafting. They wait while you write a reply of your own. When Claude finishes a turn without asking anything, the band says it is done for now and the reply box stays under the story.

End session asks first, and Keep it open is the default. New task (Cmd+K, Ctrl+K on Windows) opens the command launcher, even while you type in a field. In the launcher, Cmd+Enter (Ctrl+Enter on Windows) runs the task, and Esc goes back.

## One run at a time

Only one run works at a time, because two commands could overwrite each other's files. A session waiting for your reply lets a new run start only once its turn has ended and all its background agents have finished. A Save on the Profile screen waits for the run too.

## Copying text

Text in Claude's messages and questions, your replies, error alerts, command lines, company research, archived postings, follow-up drafts, the Profile paper and the folder path in Settings › General can be selected and copied. A question cut to fit is not, and shows in full under the pointer. Copy says Copied.

## Settings

Settings (Cmd+, , Ctrl+, on Windows, or Settings at the right of the status bar) replaces the menu and the screen until you press Back to app or Esc. A task keeps running meanwhile, and the status bar says when one waits for you. The rail groups the sections. A word beside a section says its state where it has one: the theme beside Appearance, what is missing or still to fill beside Runtime status, this month's spend beside Usage statistics, and Later beside the sections still to come. Search keeps only the sections with a match and marks the matched words. Esc clears the search first. Changes apply at once and stay on this computer. If the settings file cannot be read, the rail's foot says so: the defaults are in use, and your next change replaces the file. If a change cannot be saved, it says the change lasts until Jobdesk closes. On the first launch after the theme moved into Settings, the theme and text size you had are kept.

- General: your job search folder, with Show in Finder (Show in Explorer on Windows) and Change folder, and the screen Jobdesk opens on. Without a folder, Jobdesk opens on Today. While a Claude session is open, even one waiting on your reply, Change folder waits here and on Setup, so the task keeps its folder. In Tasks, stop it (Stop run) if it is working, then end it (End session).
- Appearance: the theme (System, Light or Dark), the text size from 90 to 150 percent (Cmd + and Cmd −, Ctrl + and Ctrl − on Windows, change it too), and Reduce motion, which drops the card slide and the day stamp's fill.
- Runtime status: Claude Code's version and the sign-in the last task used, then how many profile topics are filled, with Open Profile, and your tools, portals, templates and sync, as Setup shows them. The tools' ready count leaves out Claude Code, which has its own row above. Until a folder is chosen and read, only Claude Code and the tools show, and a search matches only those.
- Usage statistics: what your tasks cost and did, from Jobdesk's own run logs for this folder. It counts tasks only and fetches nothing. A period toggle picks the current month, the last 90 days, This year or All time. Four figures follow: Spent, Tokens in (with tokens out in the note), Cache hits, and Tasks, split into finished, stopped and failed. Per application sent divides every task's cost by the applications you marked sent in the period, and hides when there are none. Below sit the last 12 months by day: hover a day, or Tab to a day with tasks, to see its date, tasks and spend. Then come two tables, By command and By model.
- Run defaults: the model and Spend at most that each new task starts from, a table of commands with their own model and budget (a command's row beats the defaults, and Add a command picks from the commands Jobdesk knows), and a monthly cap. Left empty, Spend at most keeps each command's suggested budget. You can still change both in the launcher. At or over the cap, Run is disabled with the reason and a link back to the cap, and it opens again on the 1st. A task already running is not stopped, and replies in a session already open still send. The cap counts this month's tasks in the open folder, as Usage statistics does. Right after a task ends, Run waits until its cost is read.
- Permissions: Allow and note in the activity, the default, or Ask me before each tool. Either way, questions from Claude always reach you, and a rule you let it remember lasts only until the task ends and is never saved to your folder.
- Data and privacy: where Jobdesk keeps its data, with the size of the run logs and the tray decisions. Keep run logs for 30 days, 90 days or Forever, which is the default. A shorter time deletes older logs when Jobdesk starts, never those of the current month, and Usage statistics then counts only the logs kept. Clear tray decisions asks first and forgets what you decided on Today in this folder. Other folders keep theirs.
- About: the Jobdesk and Claude Code versions with Copy details for a bug report, and links to the auth review and the changelog. The licences are not available there yet.

The other sections show in the rail and open in later versions.

## Profile

Profile shows what Claude reads about you, one topic at a time, on paper beside a rail of topics. On your CV holds Identity and languages, Experience, Education, Skills, Projects and publications, References and Profile statements. What guides Claude holds How I work, Goals and deal-breakers, Interview stories and Job search. Each topic reads Not started while its sections hold nothing but headings, blank lines and comments, N to fill while it still holds placeholders such as `[FIRST_NAME]`, and Filled when none are left. It reads Hidden when a file that holds it cannot be read, and the alert under the head says which. A fresh checkout reads N to fill for every topic, and the screen opens on the first topic with something left to fill.

The paper shows each topic as its files hold it, placeholders included, as written. Its text keeps to a readable column, and a table fills the width of the paper. Only a role's title, company and dates line shows them as "to fill" blanks. A placeholder counts where the page shows it, in text or in code, never in a link's address or in HTML such as a comment. Notes the templates keep in comments stay off the paper. The footnote names the file and headings the topic is kept in. Parts from your `CLAUDE.md` read "Summary in CLAUDE.md" or, for example, "Certifications in CLAUDE.md", and only its Candidate Profile part shows, never the rest of that file.

Update with Claude… opens the launcher on `/setup --section` for that topic, and nothing runs until you start it there. It plays out in Tasks. In a test, one topic took about three turns and cost about $1.00. It waits while another run works and while the topic's file cannot be read. Interview stories and Profile statements have no setup section, so you edit them by hand only. A topic not started offers Update with Claude… where it can, and Write it myself, with the outline it will hold faded below.

When a task ends, each topic it changed carries Updated on the rail, and the topic's head says which blocks changed and when. Opening the topic clears its rail mark, and leaving it removes the head line. The marks compare your profile when the task started with the first read after it ended, so a change made outside Jobdesk while a task runs is marked too. They last until Jobdesk closes.

Edit on a role or block, and Edit section on a section, open its text in place. Add a role opens a new one after the last role, and Write it myself opens the whole section. One editor is open at a time. Preview shows the draft as the paper will. Save (Cmd+Enter, Ctrl+Enter on Windows) writes only that block and leaves the rest of the file as it was. Cancel (Esc) closes the editor. While you type, the first Esc leaves the field. With unsaved text, Cancel, opening another topic and leaving the tab ask first, and Keep editing is the default.

While a task runs, Save waits, because the task may change the same file. Your draft stays, and you can save once the task ends and Jobdesk has read the files it changed, which takes a few seconds at most. Just before it writes, Save reads the block again. If it changed on disk since you opened it, Save writes nothing and shows Changed on disk with the block's new lines. Use the new version replaces your draft with them, and Save mine over it writes your draft over them. If the block is gone from the file, Save mine over it puts your draft back at the end of its section, or at the end of the file when its section is gone too. Changes elsewhere in the file never stop a Save.

## Setup and the status bar

Setup shows whether your profile, tools, portals, templates and sync are ready. Its Profile panel is one line: how many of the eleven topics are filled, how many are hidden if a file cannot be read, and Open Profile, which opens the Profile screen. With no profile file found, the panel, like the Profile screen, offers Set up your profile…, which opens `/setup` in the launcher. Writing style and cover letter templates no longer show there. Setup's tab stays in the menu only while a required item is not ready, so through your first setup too, and comes back if one goes missing later. A required item is your job search folder or a required tool. Your profile never keeps the tab. Optional tools never keep it either, and never count against the ready total. Settings › Runtime status always shows the same. The status bar says "Claude is ready", or "Claude Code is not installed". Before you choose a folder it says that Claude is installed, and that Jobdesk uses its sign-in and stores no passwords. The theme is set in Settings › Appearance.

## Privacy and trust

Slash commands write your checkout. Jobdesk changes it only when you save a Profile edit, and then it writes only that block of its profile file, never while a task runs, and only when the block on disk is as you opened it or you chose Save mine over it. That check happens a moment before the write, so if another program, such as an editor, a sync tool or Claude in a terminal, writes the same file at the moment you save, Jobdesk can overwrite that change without warning. A file named like `<profile file>.jobdesk-save-…` beside a profile file is a copy left by a save that was interrupted, for example by a power loss: it holds that profile's text, a later save removes it once it is an hour old, and you can delete it yourself. Because Jobdesk allows the commands' tool requests without asking unless you turn prompts on in Settings › Permissions, a posting written to trick Claude could get a command run before you see it. The workspace checks postings with `posting_guard` before scoring them. It stores no credentials and uses the login of your installed Claude CLI. Run logs, which include the text of your runs, and your tray decisions are kept in the app's data folder and are readable only by your user. Run logs stay there forever unless you choose a shorter time in Settings › Data and privacy.

See [ARCHITECTURE.md](../ARCHITECTURE.md#trust) for how Jobdesk renders postings, notes and reports.
