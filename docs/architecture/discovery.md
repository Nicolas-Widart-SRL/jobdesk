# Discovery

This page covers how `discovery` checks a job against the freelance terms, what the Jobs view filters hide, and where a job's check rows come from. The features it names are mapped in [ARCHITECTURE.md](../../ARCHITECTURE.md#features).

## Freelance gates

`/rank` stores no contract, day rate or commute fields. So `discovery` reads them from what it does store: the title, location, strengths and gaps.

- A gate with nothing to read flags, because silence is not a pass.
- Failed freelance gates suggest a skip and never change a job's lane, because `/rank` owns the shortlist.
- The day-rate floor comes from `01-candidate-profile.md`, and the commute tiers from `04-job-evaluation.md`.
- With neither, the gates flag instead of guessing. Two exceptions: a fully remote job still passes the commute gate, and a place the location check ruled out still fails it.

## Your terms

Settings › Your terms shows the day-rate floor and the commute tiers read only, beside the contract rule, Freelance only. No file states that rule, because it is the one contract term the gates know.

Each part keeps its state in `YourTerms`:

| The file | The part reads | The gates get |
| --- | --- | --- |
| Is not there, or states no value | Not stated, and the rail counts those parts | Nothing |
| Cannot be read | Hidden, never Not stated, and Update with Claude… is disabled until it reads again | Nothing, as from a missing file |

## Jobs view filters

The Jobs view filters in Settings hide rows from every list on Jobs:

- jobs that fail the day-rate, commute or language check;
- jobs scored under a number, 60 by default;
- jobs the safety check flagged, unless the user kept the ad on Today.

Every filter starts off. A job with no score is never hidden by the score.

The filters hide rows only and never change a job's lane, because `/rank` owns the lists. Today still deals a flagged ad as Check this ad until the user keeps it. A kept ad gets no Check this ad card: Today deals it as it deals an unflagged job, which gets a card only on the shortlist. On that card the safety row stays Unclear. The summary names the kept ad only when every other check passes.

Jobs counts what the filters hide in the open list and offers Show them, then Hide them.

## Job checks

A job's check rows, and the sentence that sums them up, come from `JobChecks` in discovery's application layer.

- Each row has a typed kind, which gives both its label and its wording.
- Today's card and the Jobs detail both read it, so the two screens word a job the same way.
- An ad the safety check never saw reads as unchecked, not as flagged.

Whether the user kept a flagged ad lives in `intray`. So the shell hands Jobs the kept job keys from intray's application layer, beside what waits in Today.
