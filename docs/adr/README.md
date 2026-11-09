# Architecture decision records

An ADR records one decision and why it was made, so that a newcomer does not undo it by accident.
It is not documentation of how a part or a check works.

## When to write one

Write an ADR when the decision meets at least one of three tests:

- It is hard to reverse: users' designs, std or the file formats would have to change.
- It would surprise a newcomer: the obvious reading of the code gets it wrong.
- It rejects an obvious alternative: someone will propose the alternative again.

The pull request that adds an ADR says which of the three it meets. If it meets none, there is no ADR.

## What an ADR is

At most one page, about 60 to 80 lines. `scripts/adr-check` fails an ADR over 80 lines or 8000 bytes. In plain paragraphs, with no template headings:

- the decision, and why;
- what was rejected, and why;
- the cost;
- what is left open.

The file is `NNNN-short-kebab-title.md`, numbered after the highest number in the repository and in open branches. Its title states the decision as a sentence (`Only a board names nets`). The models are 0005 and 0010.

## What is not an ADR

- A model rule of one part or one check, a threshold, a std interface detail. It goes in `docs/LANGUAGE.md`, the doc comments of the std definition, and a test.
- A feature plan with stages or a to-do list. It is a GitHub issue.
- Progress of any kind ("stage 2 implemented", "built on 2026-11-06"). It is in the git history and the issue.
- A review log or a list of findings. It is in the pull request.

## Status

The `status:` line has one of four words, first on the line:

- `proposed`: written and waiting for the owner. Nothing may depend on it as settled.
- `accepted`: the owner agreed. This is the only state in which a decision binds.
- `superseded`: a later ADR replaced it. The line reads `status: superseded by ADR-00NN`, and it is set when the later one lands.
- `folded`: the content now lives in the guide or the code. See below.

Nothing else goes on the status line: no dates, no stage numbers, no review notes.

## No amendments

An accepted ADR is not edited to say something else. A change of mind is a new ADR that says what it replaces and why, and the old one becomes `superseded`. A typo or a wrong file name may be fixed in place.

## Folding

When the guide, the std doc comments or the code carry a decision completely, the ADR is cut to a stub of three lines and keeps its number, because source files cite numbers (`ADR 0040`). The stub is the title, the status and where the content went (an example, not a real stub):

```
# A TVS diode has a breakdown voltage as an optional column
status: folded
Now in `docs/LANGUAGE.md` ("Catalog columns") and `std/diode.nsm`.
```

A pointer names a file and, where the file is long, a section. The full text is in the git history.

## Who decides

The language is the owner's. An agent that finds a language-design choice (syntax, semantics, what an error rule refuses, what a std definition exposes) writes the options with their costs and a recommendation, and asks. It does not write the choice as an accepted ADR. It may write a `proposed` one, and says in the pull request that the owner has not decided.

See `.agents/skills/netsmith-dev-decision/SKILL.md` for the steps, and `docs/DESIGN-RULES.md` for the rules a decision is checked against.
