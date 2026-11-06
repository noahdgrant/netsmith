# Compare with a git ref: the page (design note)

Status: 2026-11-06, the page side of ADR 0051. The server is `GET /api/git`, `GET /api/compare` and `GET /api/git/file`; this is what the editor does with them. The benchmark (`ui-benchmark.md`) puts "compare to a git ref" first among the things a code-first EDA tool can do that a GUI tool cannot: the question an engineer has about a branch is "what did it change in the circuit?", and the answer is parts, nets, rails and cost, not a text diff.

## What was decided

- **One section, and marks in the others.** The section (`views/Compare.tsx`, key 8) is where the answer is read: the sentence, five tiles, seven tabs of rows with before and after. The other views get **marks** behind one global switch, because a change is easiest to see where the circuit is drawn. The switch is one signal (`compare/store.ts: showChanges`) with a button in each view's toolbar, a palette command and the status chip's menu.
- **A subscriber, not a re-render.** `compare/compareDom.ts` finds what stands for each changed part, net, rail, module and board (`data-ref`, `data-net`, `data-hv`, `data-path`, `data-cmp-board`) and toggles classes, as `hoverDom.ts` does for the hover; a drawing that is replaced is marked again. The assignment of a class to a selector is pure and tested (`compare/model.ts: markTargets`). Where a view needs more than a class (the BOM's `Change` column and its struck-through removed lines, the Source's margin, the schematic's list of removed parts) it reads the same store.
- **Identity is the server's.** The page never decides what is the same part; it shows what ADR 0051 says: a path, a name, a rail's net. A renumbered part is listed apart and folded: adding one capacitor renumbers the rest, and that is noise. A removed part has no head side, so it is listed (inspector, BOM) and not drawn.
- **The line diff is small and in the page.** A file is compared as text only where a person asks for it (`Base`, `Show changes in source`, the margin): Myers over lines with the common ends taken off, words marked inside paired lines (`compare/linediff.ts`, tested against a replay of its own script). The view is read-only CodeMirror with the page's own gutter and tokens; no new dependency.
- **Words.** The glossary says _ref comparison_ and avoids _diff_: the page says `Compare`, `changes`, `comparison of a file`; `diff` stays in code only (and the progress stage `diff` is shown as `compare`).

## Beside the other views (night 3)

- **A mark, the hover and the pick are three signals on one element.** The schematic's parts, the board's, the cards of the power tree and the system each can have a mark (`cmp-*`), the hover's `hover-lit` and the pick; they are classes from three writers and the properties they set overlap (the outline and `filter`, the border and `box-shadow`). The rule, written once in `compare.css`: the pick owns the outline and the border; the hover adds its light to the glow or a ring outside the mark's; the mark stays visible under both (the glow on a picked part, a ring outside the pick's on a picked card).
- **Subscribers write the difference.** Views that render again replace the `class` of their elements, so a subscriber that only added on a signal lost its classes on the next render (a picked card lost its mark). Both DOM subscribers now work out what each element should have and write only what differs (`domMarks.ts`), and they watch `class` as well as the nodes that come and go; a write that changes nothing writes nothing, so there is no loop and no taking of the other's class.
- **Escape belongs to what is on top.** The key is handled at the window before the panel's own handler, so the commands that also take Escape (`Back`, `Clear the selection`) step aside for anything marked `data-esc-closes`.
- **The status bar has priorities.** Nothing in it was ever dropped as the window narrowed; now the chip and the cursor outlast the analysis time, the language and the theme, and the chip loses `vs main:` before it is cut.

## Tokens

| Token | Each theme | Held by `theme.test.ts` |
|---|---|---|
| `--cmp-added`, `--cmp-changed`, `--cmp-removed`, `--cmp-renumbered` | green, amber, red, a cool neutral | 3:1 on every surface, the schematic's sheet and the board (graphics); the four are different; `--text` and `--text-muted` stay at AA on a 16 % tint of each over every layer (the value chips) |

Outlines are not the only signal: a changed part also has its designator in the mark's colour, a renumbered part is dashed, a removed line is struck through, and the badge words (`Added`, `Changed`, `Removed`) are text.

## What the server would make better

- A `kind` on a changed part (`ComparePartChange` has fields and paths but no `Capacitor`), so a changed row can say what it is.
- `board` on every problem in `ProblemsDiff` (it is there for most; a problem of a module that two boards share has none), and the module path on a net's change, so a list can be grouped by module without reading pins.
- A way to compare two refs (the head is the files on disk), for a review of a pull request against its base.
- The problems of a ref that does not analyse are errors only; its warnings and notes would say what was already wrong.
