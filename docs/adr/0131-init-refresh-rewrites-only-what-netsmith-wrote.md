---
status: proposed
---

# `init --refresh` rewrites only the agent files that netsmith wrote and nobody edited

ADR 0015 had `netsmith init` write the agent files once and gave no way to bring them up to a newer netsmith, because a tool that replaces files in the project would undo the edits a person made to them. A project then keeps the guide, the skills and the rules of the netsmith that made it for good, and an agent works from a guide that no longer matches the program. This adds the refresh and keeps the reason for the old choice: `netsmith init --refresh` rewrites a file only when its text is one netsmith wrote.

How it knows. `init` keeps a record, `.agents/netsmith-files.sha256`, with the SHA-256 of each file it wrote (in `sha256sum`'s form, committed with the project). A file whose hash is the record's, or equals the text netsmith writes now, is unedited. A project made before the record has none, so the binary also carries a frozen list of the hash of every earlier version of these files (`templates/agent-files-history.txt`, made from the repository's history); it is never extended, because every file written from now on is in the record. A file that is missing is written, so a skill that a new netsmith has gains its folder. A file that matches neither is the person's: it is left alone and listed with a line diff, and the person deletes it to take netsmith's text. Nothing is deleted: a chapter or skill that a newer netsmith dropped is named and left.

`init` also writes `CLAUDE.md`, one line, `@AGENTS.md`, because Claude Code reads `CLAUDE.md` and not `AGENTS.md`. A `CLAUDE.md` that exists is the person's and is never rewritten; `init` says to add the line if it lacks it.

Rejected: rewriting every file and trusting version control to show the loss (a person who does not commit loses the edit, and a refresh would not say it); a version number in each file (a skill's text is plain Markdown that agents read, and a header would be one more thing to edit around); keeping every old text in the binary instead of hashes (the guide has 200 versions).

The cost: a person who edits a file and wants netsmith's later text has to merge by hand with the diff, and the history list adds about 50 KB to the binary. Open: a record entry for a file that the person edited back to an old version is kept, so it counts as unedited again; whether the refresh should offer a three-way merge is not decided.
