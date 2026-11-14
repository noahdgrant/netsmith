---
name: netsmith-dev-decision
description: Decide whether a choice needs an ADR, write ADRs and GLOSSARY entries in netsmith's format, and keep language-design choices with the user. Use when a change touches syntax, semantics, error rules, the std library's interface, or a design rule.
---

# Decisions in netsmith

Entry point: `AGENTS.md`. Read `docs/adr/*` and the glossary (`GLOSSARY.md` is the index of `docs/glossary/`) before you write either.

## Who decides

The language is the user's. These are language-design choices: syntax, what a construct means, what an error rule refuses, what a std definition exposes, new keywords, anything `docs/DESIGN-RULES.md` lists.

1. State the question in one sentence.
2. Give two or three options. For each, show a three-line example and its cost.
3. Recommend one and say why.
4. Ask, and wait for the answer. Do not pick silently, and do not code the option you prefer first.

Implementation details are yours: data structures, module layout, algorithms, test design, error wording. Decide them and move on.

Check any choice against the rules in `docs/DESIGN-RULES.md`: one correct way, statement order never matters, libraries state only what is always true, a module is configured through its interface, only a board names nets, a part number needs a source.

## When to write an ADR

`docs/adr/README.md` says what an ADR is. Write one only when the choice is hard to reverse, surprises a newcomer, or rejects an obvious alternative, and say in the pull request which of the three it meets. Skip it for a bug fix or an implementation detail.

These are not ADRs:

- A part's or a check's model rule, a threshold, a std interface detail: put it in the guide (`docs/language/`), the std doc comments and a test.
- A feature plan with stages or a to-do list: open a GitHub issue.
- Progress ("stage 2 implemented"): the git history and the issue have it.

## ADR format

File `docs/adr/NNNN-short-kebab-title.md`, numbered after the highest one in the repository and in other open branches. Look at 0005 and 0010 as models.

- One page at most, about 60 to 80 lines (`scripts/adr-check` fails more than 80 lines or 8000 bytes).
- Frontmatter `status:` with one word: `proposed` (written, waiting for the owner), `accepted`, `superseded by ADR-00NN` or `folded`. Nothing else on the line.
- A title that states the decision as a sentence: `Only a board names nets`.
- Plain prose paragraphs, no template headings: the decision and why, what was rejected and why, the cost, and a final `Left open` paragraph. Short sentences. Name the files, fields and numbers involved.
- No amendments. A change of mind is a new ADR, and the old one gets `status: superseded by ADR-00NN`.
- When the guide or the code carries a decision completely, cut its ADR to the three-line `folded` stub of the README. Keep the number: source files cite it.
- Language-design choices are the owner's: write options and a `proposed` ADR at most, never `accepted`.

## Upkeep after a decision

- `docs/glossary/<first letter of the term>.md` (`GLOSSARY.md` is the index): add each new term where it sorts, with a one-line definition and an `_Avoid_:` line. Use these terms in code, messages and docs.
- `docs/language/`: update the chapter of the construct and its tested examples (`netsmith-dev-extend-language`).
- `docs/DESIGN-RULES.md`: the design rules.
- Nothing is left as a known bug. If it cannot be fixed now, open a GitHub issue for it.
- Notes live in the repository, not in memory.
