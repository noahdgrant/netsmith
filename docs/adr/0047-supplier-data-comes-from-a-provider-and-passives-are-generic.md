---
status: provisional (written by the agent from issues #8 and #9, 2026-11-03; to be reviewed by the maintainer; open questions 1 and 7 decided by the owner, 2026-11-03, see _Owner decisions, 2026-11-03_). Supersedes the data-in-files parts of ADR 0022 (items 4 and 6: the `price`, `stock` and `date` columns and the stock-0 rule) and of ADR 0039 (items 9 and 10, the ages, and `catalog age` reading `date`). The identity check of `catalog refresh` stays.
---

# Price and stock come from a provider when the bill of materials is made, and passives are generic until a later step resolves them

Issues #8 and #9. Extends ADR 0022 (catalog files) and ADR 0039 (catalog refresh); nothing here changes the language. Every choice below that the maintainer may want otherwise is listed under _Open questions_.

## The problem

Two things are one problem. A catalog row mixes what a part is (its LCSC number, manufacturer's number, package, values) with what a supplier says about it today (price, stock, library type, date). The second kind goes stale within days, the suppliers' terms of use are unread, and ADR 0039 ended with the shipped files holding blank cells, so every standard row now counts as "undated" in the bill of materials, in `catalog age` and in the editor's stale chip. And the solver can pick only a value that has a row, so a value nobody stocks blocks the build, although the circuit is fine and any resistor of that value would do.

## Audit: how the solver picks values today

Read from `crates/netsmith-solve/src/solve.rs` and `table.rs` at ca13564.

**Policy: the first row that works.** For each part with a catalog, `pick_parts` reads the catalog's file as candidates in file order. `fits` drops a candidate that has no value in a column, whose value is outside what the design already says about the field (`Domain`), whose text differs from a stated text, or whose package lacks a pin the board connects. Parts that share a relation form a group. A depth-first search (`search`) tries each fitting candidate: `try_candidate` writes its values into the model, propagates, and checks every relation that mentions the part in the worst case. The first candidate for which the whole group holds is kept. Nothing compares candidates: no closest value, no cheapest, no best margin. The file order is the whole policy (ADR 0022 item 6): Basic rows before Extended, then the order of a price read on a past date (which cannot be re-read now), then ascending value. A candidate that behaves like one whose every completion failed (`interchangeable`) is skipped, and the search stops after 250,000 trials (`gave up`).

**What is in the files.** `resistor.csv` has 219 rows: the complete E24 series from 10 ohm to 1 Mohm in 0402 (121 values), the Basic E24 values in 0603 (79 rows), 12 in 0805, and a few E96 values that were once needed. There is no E96 series, no 0402 value below 10 ohm or above 1 Mohm, and no 1206. A row with an empty cell in a column the part lists (an unrated `max_power`) is never offered (ADR 0025).

**When no row works.** The part gets a diagnostic, so the build fails (`outcome_with` returns the problems when `solution.diagnostics` is not empty). There are four messages: "the `resistor` catalog has nothing for `.r1`, which needs resistance within ..." (nothing fits what the design says), "no `resistor` candidate satisfies `.r1`: the closest of the N that fit, X, fails because ..." (some fit and every one fails a relation), "no combination of candidates satisfies A and B together" (a group), and "gave up after trying 250000 combinations". The message names the closest failing candidate and notes rows left out for an empty cell, a missing curve, or a package without a pin. It never says that a value exists as a part number somewhere else, and it never offers a nearby value that would pass. The part is also in `Solution::unresolved`, which `notes` turns into "not chosen" lines for the cases that raise no diagnostic (no such catalog, an unreadable catalog, a keyed catalog with no key).

**Where stock 0 blocks a build.** `Table::candidates` reads every cell of a row and then skips the row if its `stock` cell is exactly `0` (ADR 0039 item 9). An empty `stock` is unknown and the row is offered. So a stock of `0`, written by hand or (in the first version of ADR 0039) by a refresh, removes the row from the catalog before the search. If it was the only row of its value, the part gets the "nothing for" message, which counts rows left out for empty cells and rows without a pin, but not rows left out for stock: **a sold-out row is dropped silently**. The shipped files have no stock, so today nothing is blocked this way, but a project that enters stock gets a solver whose answer depends on a number that is wrong the next day, and a design that built last week may not build today.

**Where price, stock and date are read.** `Catalogs::supplier` finds the first row with an LCSC number and returns its `price`, `stock` and `date`. Only the editor's bill of materials (`editor/sourcing.rs`) and `catalog age` (`refresh/mod.rs`) use them. `fab` writes `bom.csv` without a price and refuses an assembled part that has no LCSC number. A line is stale when it has an LCSC number and its date is missing or older than 90 days, so every standard row is stale and every one shows "no date" (`Bom.tsx`, `refresh/mod.rs`).

Findings: (1) the solver cannot work on a value without a row; (2) it depends on stock only through the `0` rule, silently; (3) the order of the rows encodes prices nobody can re-check; (4) price and stock are read from files that ship none.

## Decision

**1. Identity stays in the repository; price, stock and date do not.** A catalog row keeps `lcsc`, `mpn`, `library` as it was read (Basic or Extended is part of what a person chose), `package`, the electrical values, `source`, `checked`, `name`, `label`, `notes`. `price`, `price_breaks`, `stock` and `date` are no longer the way to get a price. A file that has them is still read, and its cells are the answer of the last provider in the chain, the `catalog` provider, so a project that keeps its own prices loses nothing; the standard files ship them blank. The solver never reads them: **the stock-0 rule of ADR 0039 item 9 is withdrawn**, and a row is offered whatever its stock cell says. Stock is a fact about a day, not about a part, and only the resolution step (3) uses it.

**2. Prices come from a provider, at `bom` and `fab` time, and are cached.**

The interface is a trait in the `netsmith` crate (the solver, the checker and the language know nothing of it):

```rust
pub trait SupplyProvider {
    fn name(&self) -> &str;                 // "file", later "lcsc", "jlcapi"
    fn cacheable(&self) -> bool;            // false for a provider that reads a local file
    fn lookup(&self, part: &PartKey) -> Result<Vec<Quote>, SupplyError>;
}
pub struct PartKey { pub lcsc: Option<String>, pub mpn: Option<String> }
pub struct Quote {
    pub provider: String,
    pub channel: Channel,                   // which stock this is
    pub stock: Option<u64>,                 // pieces; None if the provider does not say
    pub price_breaks: Vec<PriceBreak>,      // (from quantity, price for one piece)
    pub currency: String,                   // "USD"; nothing converts it
    pub fetched: u64,                       // Unix seconds, when the provider answered
}
pub enum Channel { JlcpcbAssembly, LcscRetail }
```

`lookup` returns every quote the provider has, one for each channel, and an empty list for a part the provider does not list. An error (`Offline`, `Refused`, `Unavailable`) is not an answer. **The answer says which stock it is.** JLCPCB assembly stock (the pieces JLCPCB holds for its assembly service, which is what a board ordered with assembly uses) and LCSC retail stock (the pieces LCSC sells to a buyer) differ for the same number: ADR 0039 recorded 1,085,700 against 2,059,008. A bill of materials for a JLCPCB order shows `JlcpcbAssembly`; if a provider has only `LcscRetail` it shows that, labelled, and never presents it as assembly stock. Which channel a project means is the manifest's `channel`.

A price is a ladder, and a board order has a quantity. A line is priced at the highest break at or below the quantity the line needs; if the quantity is below the first break, the first break's price is shown, marked "from N pieces". Prices are not converted between currencies; a provider that answers in another currency is shown as it answered.

**3. The cache.** Answers of a cacheable provider are kept in `<cache dir>/supply/<provider>.csv` (the directory is `$NETSMITH_CACHE_DIR`, else `$XDG_CACHE_HOME/netsmith`, else `~/.cache/netsmith`, as for the refresh cache), one row per part and channel: `lcsc,mpn,channel,stock,price_breaks,currency,fetched`, with `fetched` in Unix seconds. It is plain CSV so a person can read and delete it; it is a cache, never read as design data, and never in a project. Age rules:

- an entry younger than the cache time (24 hours, the manifest's `cache_hours`) is used without asking the provider;
- an older entry makes a request; a good answer replaces it;
- if the request fails, or the run is offline, the older entry is used and shown with its age ("read 12 days ago"), never silently refreshed, and never dropped for being old;
- an entry older than 90 days (the number `catalog age` and the stale mark already use) is marked stale wherever it is shown;
- a part the provider does not list, and a failed request, are not cached.

An unreadable row of the cache is ignored with a note. The file provider reads its file on every run and is not cached: its file is already a local copy, and its `fetched` is the date in the file.

**4. Offline and unknown.** A part with no quote has price and stock **unknown**. That word is shown wherever a price or stock would be (bill of materials, editor, `catalog age`); a total counts the priced lines and says how many lines have no price; a stock of unknown is never shown as 0 and never compared as 0. Unknown is the state with no provider configured, with `--offline`, with the network down, and with a part the provider does not list (the last is worded "not listed by <provider>"). Nothing guesses a price from a neighbouring row. A made-up part (no `lcsc`) has nothing to look up and shows nothing.

The wording replaces "undated": a line with no quote says "price and stock unknown", with the reason (no provider configured, offline, not listed); a line with a quote says "<provider>, <channel>, read <date>, <age>". `catalog age` lists the rows whose quote is older than `--days` days or unknown, and counts them separately.

**5. Configuring a provider; credentials are never in the repository.** A project names its provider in a `[supply]` table of `netsmith.toml`:

```toml
[supply]
provider = "file"                 # "file" now; "lcsc" or "jlcapi" later
file = "supply/quotes.csv"        # for the file provider, relative to the project
channel = "jlcpcb-assembly"       # the stock the bill of materials shows
cache_hours = 24
```

The manifest has no field for a key, token or password, and its parser refuses an unknown key (as it does now), so a secret cannot be committed by mistake. A provider that needs a credential reads it from the environment (`NETSMITH_LCSC_KEY`, `NETSMITH_JLCAPI_KEY` and the secret that goes with it) or from a file in the user's configuration directory (`$XDG_CONFIG_HOME/netsmith/supply.toml`, never inside a project); a missing credential is the message "the lcsc provider needs NETSMITH_LCSC_KEY; set it in your environment", and then the lines are unknown. No provider that talks to a supplier is built by this ADR (Open questions). *(Owner decision, 2026-11-03: the `[supply]` table is the way to configure a provider, and a credential is given there only by the name of an environment variable, never by its value; see _Owner decisions, 2026-11-03_.)*

The **file provider** reads a local CSV, `lcsc,mpn,channel,stock,price_breaks,currency,date`, which a person writes from a supplier's export or by hand. It is how the tests work, and how a company with its own price list or a snapshot from a supplier's file download feeds the bill of materials with no network. The row's own cells (item 1) come last.

**6. Generic passives and their resolution.** For a resistor the solver chooses among **generic parts**: a nominal value in a stated E-series (E12, E24 or E96), a tolerance, a package and a power rating, with no supplier and no part number. A generic part is an ordinary candidate that has no `lcsc` and says it is generic. The series is stated by the manifest's `series` key (E24 by default; the language is untouched, the series is policy of the project like `assembly`). The solver may pick any value of the series that satisfies the relations, so it is never stuck on a value that no known supplier row stocks. The power rating of a package is the usual one (0402 62.5 mW, 0603 100 mW, 0805 125 mW); these are packages' ratings, not a supplier's data. Capacitors, inductors and LEDs follow the same design (a capacitor also needs a bias curve, which a generic part has no source for, so it is not built first). *(Owner decision, 2026-11-03: a generic ceramic capacitor resolves only to a catalog row that has a recorded DC-bias curve; see _Owner decisions, 2026-11-03_.)*

**Resolution** is a separate step, run when the bill of materials or the fab package is made, over the lowered sourcing. For each generic part it picks an identity row of the same catalog:

1. same package, nominal equal to the generic value, tolerance no wider, power rating no lower (the row must be at least as good as what the solver checked);
2. among those, the order of the **preferred-parts list**: JLCPCB Basic, then Preferred, then Extended, and inside a class the order of the file (so a project's own rows, which come after the standard ones, are tried last; a project that wants its part first says so in its own list: Open questions);
3. if a provider has stock for the project's channel, a row with stock 0 is passed over and noted; unknown stock does not exclude a row.

A resolved part gets the row's `lcsc`, `mpn` and `library`; its value label is unchanged. If no row fits, the part is **unresolved**, and `fab` refuses with a message that names the part and offers the nearest alternatives:

```
R3 (generic 340 ohm 1% 0402, E24): unresolved. No row of the `resistor` catalog has this value.
nearest in the catalog: 330 ohm 1% 0402 (-2.9%, Basic), 360 ohm 1% 0402 (+5.9%, Basic), 330 ohm 1% 0603 (-2.9%, Basic).
Change the value in the design and build again: the solver then checks every relation again with it.
```

The alternatives are the rows nearest in value (first in the same package, then others), up to three. The bill of materials lists the part with the same message and the line stays unpriced. The re-check after a swap is the build itself: a different value written in the design (or chosen from the editor's list of parts) is solved again against every relation, so a swap that breaks a current limit is reported by the checks it breaks, as any change of value is. A later step may run that check for each alternative and show pass or fail beside it (Open questions).

**7. Terms of use.** Nothing in this ADR contacts a supplier. No provider that does is built until the maintainer has read the supplier's terms and the official API's licence: JLCPCB's `robots.txt` disallows `/api` and ADR 0039 recorded that its terms could not be read. A real provider uses the supplier's official API with the user's own key; it is a trait implementation that does not change anything else here.

## What the first implementation builds, and where it differs

The first implementation (the pull request after this ADR) is the smallest slice that works from the manifest to the bill of materials:

- `crates/netsmith/src/supply/`: the trait, `Quote`, `Channel`, the cache (`cache.rs`, the format above), the file provider (`file.rs`) and `Supply`, which applies the age rules and the channel. `Supply::lookup` answers `Quoted` or `Unknown(reason)`. The `[supply]` table is read by `Manifest`; `Project::supply()` builds it.
- The last resort ("the `catalog` provider" above) is not a type: the bill of materials and `catalog age` fall back to the row's own `price`, `stock` and `date` cells when `Supply` has no answer, and say so ("catalog file, entered by hand").
- The editor's bill of materials and `catalog age` use it, say `unknown` and why, and no longer call a standard row stale or undated: a line with no price is not stale. `BomLine` has three new fields: `supply`, `unknown` and `unresolved`.
- `crates/netsmith-solve/src/generic.rs`: E12, E24 and E96, generic resistors in 0402, 0603 and 0805 at 1%, and `resolve`. The stock-0 rule is gone from `Table::candidates`. `crates/netsmith/src/resolve.rs` applies it to a board's sourcing; `fab` refuses an unresolved assembled part by name, and the bill of materials lists it. Stock reaches `resolve` through `Catalogs::with_availability`, which `Project::catalogs()` sets when a `[supply]` table exists, so every command that has the catalogs resolves the same way.
- The generic parts come after the rows. The solver therefore takes a row of the value if there is one, even a row the provider says has stock 0, and the bill of materials shows that 0 (item 4: a stock of 0 is only ever a provider's answer). Passing over a sold-out row happens when a generic part is resolved.

Not built: a provider that asks a supplier, the `--offline` flag (`Supply::with_offline` exists and is tested), generic capacitors, inductors and LEDs, running the checks for each alternative of an unresolved part, a named preferred-parts file, the quantity of an order across boards, and the editor's swap.

## What this replaces

- ADR 0022 item 4: `price`, `stock` and `date` are no longer fixed columns that mean the board house's numbers; they are read only as the `catalog` provider's last-resort answer. Item 6: the sentence about a row with `stock` 0 not being offered. The rest stands: CSV, the order of the rows as a policy (now only Basic first and the maintainer's order, not prices), `source`, the manifest list.
- ADR 0039 item 9 (stock 0 is not offered), item 10 (`price_breaks` is read by nothing; the provider's quote has price breaks), and the sentences that make `date` the age of a price. `catalog age` ages quotes. The refresh stays as the identity check against LCSC, with `checked`.

## Consequences

- A standard catalog never goes stale: it has no number that moves. A bill of materials is as fresh as the provider's last answer and says so.
- A design builds the same offline and online; only the prices and stock differ. A sold-out part no longer changes which part the solver picks; it changes what resolution picks, and says so.
- The solver's behaviour with identity rows is unchanged: a design that built keeps its parts. Generic candidates are appended after the identity rows in the first version, so a value that has a row is still picked by the old rule and only a value without a row is new. The end state is that the solver picks generic parts only and resolution chooses the row; the order of the rows then stops being a solver policy and is the preferred-parts list. That changes which part existing designs get, so it is left to the maintainer (Open questions).
- Two new places must be kept honest: the quote file format and the cache format are interfaces.

## Open questions for the maintainer

1. **Is the `[supply]` table in `netsmith.toml` the way to configure a provider?** The alternative is a user-level file only (a provider is a property of the person, not the project) with the channel in the manifest. **Decided by the owner, 2026-11-03: the `[supply]` table, with credentials given by environment-variable name and never stored.**
2. **The key for the series is `series` in `[project]`.** A syntax-free choice that is still a user-facing name. The alternative is a statement in the language (`series E96`), which is the maintainer's to decide.
3. **Generic parts appended after the identity rows or instead of them?** Appended keeps every design as it is; instead is the clean model and changes which part existing designs get.
4. **Is "JLCPCB Basic, then Preferred, then Extended, then file order" the preferred-parts list, or is it a file of its own** (`preferred.csv`, a project's list that wins)?
5. **The price rule for a quantity below the first break** ("from N pieces") and the 24 hour and 90 day numbers.
6. **Which real providers, and in which order:** the LCSC API, JLCAPI, an aggregator (the Nexar API, which Altium's bill of materials and KiCost use) or a community snapshot (the KiCad JLCPCB plugin downloads one daily). Each needs the maintainer to read its terms and to decide whose key a user brings.
7. **Capacitors, inductors and LEDs as generic parts.** A capacitor needs a bias curve for its effective capacitance (ADR 0013); a generic capacitor has none. One option is a generic dielectric class with a conservative curve, which is a modelling decision. **Decided by the owner, 2026-11-03: not a conservative curve and not a derating rule; a generic ceramic capacitor resolves only to a catalog row that has a recorded DC-bias curve, and one with no such row is reported unresolved, by name.**
8. **Should the editor offer a swap with the checks run for each alternative,** and should `fab` refuse an unresolved part (this ADR says it does) or build without it?
9. **The shipped `price`, `stock` and `date` columns:** keep as the `catalog` provider's input (this ADR), or remove the columns from the format.

## Owner decisions, 2026-11-03

On 2026-11-03 the owner answered two of the open questions above (issues #33 and #31).

1. **Provider configuration: a `[supply]` table in `netsmith.toml`** (owner decision, 2026-11-03, issue #33). The table stays as item 5 describes it. A provider that needs a credential names the environment variable that holds it, in the table; the value is read from the environment when the provider is used and is never stored, not in the manifest, not in a file of the project and not in the cache. The manifest therefore gains a field that holds a variable's name and still has no field that holds a key, token or password. This replaces the sentence of item 5 that the manifest "has no field for a key", with "no field for a key's value". The user-level credentials file that item 5 also names is not part of this decision; "never stored" excludes it, and it is dropped unless the owner says otherwise. The spelling of the field (for example `key_env = "NETSMITH_LCSC_KEY"`) is not decided.
2. **Generic ceramic capacitors resolve only to a row with a recorded DC-bias curve** (owner decision, 2026-11-03, issue #31). A generic ceramic capacitor is resolved (item 6) to a row of the `capacitor` catalog only if that row has a recorded DC-bias curve (the `effective_capacitance` cell of ADR 0022 item 3 and ADR 0013), whatever else matches. A generic capacitor with no such row is unresolved and `fab` refuses it by name, as for a resistor. The bias curve is a fact about a real part, so no row is guessed to have one, and no derating rule is stated in its place. The same issue's other items (generic inductors and LEDs, the check for each alternative of an unresolved part, the editor's swap) are not decided here.

Not built for either: the manifest field for the variable's name, and generic capacitors themselves (only resistors are generic).

## Not decided here

- Quantity breaks across several boards of an order (ADR 0035): the line's quantity is for one board in the first version.
- Converting currencies, taxes, shipping, JLCPCB's feeder fee for each Extended part.
- Any network client.
