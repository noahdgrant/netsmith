---
name: netsmith-dev-decision
description: Decide whether a choice needs an ADR, write ADRs and GLOSSARY entries in netsmith's format, and keep language-design choices with the user. Use when a change touches syntax, semantics, error rules, the std library's interface, or a design rule.
---

# Decisions in netsmith

Entry point: `AGENTS.md`. Read `docs/adr/*` and `GLOSSARY.md` before you write either.

## Who decides

The language is the user's. These are language-design choices: syntax, what a construct means, what an error rule refuses, what a std definition exposes, new keywords, anything `docs/DESIGN-RULES.md` lists.

1. State the question in one sentence.
2. Give two or three options. For each, show a three-line example and its cost.
3. Recommend one and say why.
4. Ask, and wait for the answer. Do not pick silently, and do not code the option you prefer first.

Implementation details are yours: data structures, module layout, algorithms, test design, error wording. Decide them and move on.

Check any choice against the rules in `docs/DESIGN-RULES.md`: one correct way, statement order never matters, libraries state only what is always true, a module is configured through its interface, only a board names nets, a part number needs a source.

## When to write an ADR

Write one when the choice is hard to reverse, surprises a newcomer, or rejects an obvious alternative. Skip it for a bug fix or an implementation detail.

## ADR format

File `docs/adr/NNNN-short-kebab-title.md`, numbered after the highest one in the repository and in other open branches. Look at 0005 and 0010 as models.

- Optional frontmatter: `status: accepted`, or `status: superseded by ADR-00NN for ...`. A superseded ADR gets its status line when the new one lands.
- A title that states the decision as a sentence: `Only a board names nets`.
- Plain prose paragraphs, no template headings: the decision and why, what was rejected and why, the cost, how it shows in the tools, and a final `Limits` or `Left open` paragraph.
- Short sentences. Name the files, fields and numbers involved.

## Upkeep after a decision

- `GLOSSARY.md`: add each new term with a one-line definition and an `_Avoid_:` line. Use these terms in code, messages and docs.
- `docs/LANGUAGE.md`: update the construct's section and its tested examples (`netsmith-dev-extend-language`).
- `docs/DESIGN-RULES.md`: the design rules.
- Nothing is left as a known bug. If it cannot be fixed now, open a GitHub issue for it.
- Notes live in the repository, not in memory.
