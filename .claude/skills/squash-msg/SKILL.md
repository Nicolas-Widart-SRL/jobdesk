---
name: squash-msg
description: Writes the squash merge message for a pull request, from its title and commits. Use when the user asks for a squash commit message or a merge description for a PR.
argument-hint: "<pr-number>"
---

# Squash merge message

Pull request:

!`gh pr view $ARGUMENTS --json number,title,headRefName,baseRefName --template '#{{.number}} {{.title}} ({{.headRefName}} into {{.baseRefName}})'`

Its commits:

!`gh pr view $ARGUMENTS --json commits --jq '.commits[] | "- " + .messageHeadline'`

Write the message the user pastes in GitHub's squash merge box:

- **Subject**: one Conventional Commits subject, at most 72 characters, ending in ` (#<number>)`.
  Take the type that sets the version: `feat` when any commit is a `feat`, else `fix`, else the most common type.
- **Body**: a short paragraph on what the change does for the user, then one bullet per change
  a reader of the changelog cares about, in plain words. Wrap lines at 72 characters.
  Merge commits that touch the same thing. Drop review fixes, formatting and snapshot updates.
- No co-author line, unless the user asks for one.

Print the subject and the body in one code block, and nothing else.
