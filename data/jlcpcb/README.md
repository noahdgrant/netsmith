# data/jlcpcb: sourced JLCPCB/LCSC parts

`catalog/*.csv` hold the LCSC ("C") numbers that netsmith offers: one file for each catalog (`resistor.csv`, `capacitor.csv`, `led.csv`, `ldo.csv` and the eight of the standard kinds below), one row for each part. `considered.csv` holds the parts that were looked at and not offered, with the reason.

**Price and stock are not shipped.** The `price`, `stock`, `date` and `checked` cells of every row, and the `stock`, `unit_price_usd` and `checked` cells of `considered.csv`, are empty, because a supplier's price and stock go stale within days. A price and a stock come from a supply provider when the bill of materials is made (ADR 0047); an empty cell means unknown, never zero, and the solver does not read these columns at all. A part's availability, price and library type must be read from the supplier (JLCPCB's parts page, LCSC's page for the number) before ordering. What the files do record is the identity of each part: its LCSC number, manufacturer's number, package, library type (Basic or Extended) as it was read, the electrical values from the record or datasheet, and where they were read.

## The files

A catalog file is CSV whose first line names the columns of the part's `catalog X { ... }` declaration (the field path without its leading dot) and the fixed columns `lcsc`, `mpn`, `name`, `label`, `library`, `price`, `stock`, `date`, `source` and `notes`. The format and the rules are in ADR 0022 and `docs/LANGUAGE.md` (Catalog files). In short:

- **The order of the rows is the selection policy.** The solver takes the first row that works and nothing sorts the file. These files are written Basic first (Extended parts cost a feeder fee), then in the order of the unit price read on a past date, then by ascending value, so the solver takes the cheapest Basic part that works. **The prices are not shipped, so that order was set from prices read on a past date and cannot be re-checked.** A test checks the part that can still be checked: every Basic row comes before every Extended row. When you add a row, put it after the Basic rows that cost less and before those that cost more, using the price you read at the time; and keep Basic rows before Extended ones.
- **Every row is offered.** To stop offering a part, move its row to `considered.csv` with the reason in its `not_offered` column.
- **A row needs a `source`**, the evidence described next. The loader refuses a row with an `lcsc` or `mpn` and no source.
- **Cells use the language's syntax**: `10kohm +/- 1%`, `3.3V +/- 5%`, `1.8V..2.4V`, `0.68V`, and bare text for `package` and `color`. `name` is what messages call the row and `label` is the schematic's value field.
- **`effective_capacitance`** is `derate capacitance by bias: <volts>:<fraction> ...`, the capacitor's DC-bias curve (see below). Nothing after the colon means no curve is published.
- A project adds its own parts in its own repository, not here: `catalogs = [...]` in its `netsmith.toml`.

## Evidence rules

Every row has a `source` column (`considered.csv` calls it `evidence`) with, in this order:

- `JLCPCB and LCSC part records, read <date>`: the row was written from the JLCPCB and the LCSC record of that exact number, read on that day (2026-11-01, or 2026-11-02 for the pin headers; the date says when the identity was read and promises nothing about price or stock). Both records agree on the manufacturer's number, the package and the value.
- `datasheet:<url>`: the manufacturer's datasheet, where the model uses a number from it. Regulator and LED pinouts, dropout, current and capacitor requirements come from the manufacturer datasheets (links in the `source` column), read as PDF text and, for the XC6206, as page images.
- `bias-curve:` and its test conditions, for a capacitor with a DC-bias curve (below).

The row for C5446 (XC6206) has no date: its source is the two records and the datasheet.

No part number came from memory. Part names were used only as search keywords, and a row exists only if the records returned that exact C number with the expected value and package.

## Coverage

Row counts: `catalog/resistor.csv` 219 rows, `catalog/capacitor.csv` 58, `catalog/led.csv` 14, `catalog/ldo.csv` 1, and the standard kinds' files (next section) `schottky.csv` 6, `zener.csv` 7, `tvs.csv` 9, `ferrite_bead.csv` 2, `inductor.csv` 4, `fuse.csv` 6, `n_mosfet.csv` 2, `crystal.csv` 4, `header.csv` 8 (read 2026-11-02, see Pin headers). Each row's library type is the one read with it (see "Price and stock are not shipped" above: it can have changed).

- **Resistors, 1%, thick film.**
  - 0402: the complete E24 series from 10 ohm to 1 Mohm (121 values): every Basic 0402 part (29 rows; the 0 ohm link C17168 is in `considered.csv`) and, for each other value, one Extended part, a UNI-ROYAL 0402WGF part (chosen when it was read for its preferred flag and price, which are not shipped). Where no UNI-ROYAL part was suitable (11, 13, 16, 24 ohm and 430k, 620k) another maker's part is used. Older rows (the E96 values 59k, 63.4k, 38.3k, 5.76k, 37.4k, 5k) stay.
  - 0603: every Basic 0603 part, 79 rows (E24 values from 1 ohm to 10 Mohm; the 0 ohm link C21189 is not offered).
  - 0805: 12 KOA RK73H2ATTD parts of 250 mW (10, 22, 47, 100, 330 ohm, 1k, 2.2k, 4.7k, 10k, 47k, 100k, 1M), all Extended, for power use. The ordinary 125 mW 0805 parts are not offered. The solver checks the power against the `max_power` column (below), so the 0805 parts are what a design that dissipates more than 100 mW is given.
  - There is no Basic 0.1% or thin-film resistor: the 182 Basic resistors are all 1% thick film (the other three Basic parts are 4-resistor arrays).
- **`max_power` of the resistors** is the `Power(Watts)` attribute of the JLCPCB record and `paramVOList` entry of the LCSC record for each of the 219 rows, both read 2026-11-01 and equal for every row (62.5 mW for 127 0402 parts, 63 mW for one, 100 mW for the 79 0603 parts, 250 mW for the 12 0805 parts). No row is blank. A row with no `max_power` is never offered (an unrated part cannot be shown to stay within its rating), and the solver's message says how many rows that is.
- **The standard kinds' catalogs (ADR 0025).** Fetched 2026-11-01 as for the others (JLCPCB record and LCSC record, which agree on the model, plus the manufacturer's datasheet, linked in each `source`). Packages are the names of `std/kicad.toml`, so every row can be exported. A rating is the worst case of the datasheet table, and each row's `notes` says which column it took and what the JLCPCB record lists where that differs.
  - `schottky.csv`: SS14, SS34, SS54 and SS210 (MDD, SMA), B5819W (SOD-123) and 1N5819WS (SOD-323), all Basic. `forward_voltage` is the maximum at the rated current; the datasheet gives no minimum.
  - `zener.csv`: BZT52C3V3, 3V6, 5V1, 9V1, 12, 15 and 24 (Changjing, SOD-123, 500 mW), all Extended; there is no Basic Zener. `zener_voltage` is the minimum to maximum at IZT = 5 mA.
  - `tvs.csv`: SMAJ5.0A, 12A, 15A, 24A, 33A (SMA) and SMBJ5.0A, 12A, 15A, 24A (SMB) from MDD, unidirectional, all Extended. Vc at Ipp is the datasheet table's.
  - `ferrite_bead.csv`: Murata BLM18PG121SN1D (Basic) and BLM18PG221SN1D, 0603. Rated current is the 85 C value of the reference specification; DC resistance its initial maximum.
  - `inductor.csv`: Chilisin MHCD322512A-2R2M-A8S (Isat 3.6 A, RDC 85 mohm maximum) and Murata LQH32PN3R3, 4R7 and 100M (1210). Isat is the rated current by inductance change, DC resistance the upper end of the specification's tolerance. These are the only power inductors in a package the KiCad mapping has; the 4x4 to 8x8 mm shielded inductors are in the JLCPCB library but have no footprint in `std/kicad.toml`.
  - `fuse.csv`: RUILON polymer PTC resettable fuses, 0603 (0.35 A, 0.5 A) and 1206 (0.2, 1, 1.5 and 2 A). `rated_current` is the hold current at 23 C; `resistance` runs from Rimin to R1max.
  - `n_mosfet.csv`: 2N7002 (Changjing) and AO3400A (AOS), both Basic, SOT-23. `on_resistance` is the largest RDS(on) maximum of the table and holds for a drive at or above the voltage it is given at.
  - `crystal.csv`: Epson FC-135 32.768 kHz (Basic, 3215) with its whole range over temperature (-216 to +20 ppm), and YXC 8, 16 and 25 MHz in 5032 (8 MHz Basic) at +/- 30 ppm. The 8 and 16 MHz datasheet PDF could not be read; their figures are the records' and those of the readable 25 MHz sibling.
  - Not offered and recorded in `considered.csv`: the Sunlord ferrite beads C1002, C1015, C1017 (LCSC has no record of them), Murata BLM18AG601 (record and datasheet disagree on current and resistance), BLM21PG221 (only a typical impedance), BLM18PG600 (typical only), RUILON SMD1206P050TF (400 mA in the record, 0.5 A in the datasheet), and every 4-pad crystal and HC-49 crystal (no mapping for them). No P-channel MOSFET or plain diode catalog.
- **Ceramic capacitors**: every Basic 0402 and 0603 part (21 + 33 rows; X7R, X5R and C0G, 6.3 V to 50 V, 10 pF to 22 uF), the 100 nF 50 V C307331, and the older rows. Extended capacitors were not added.
- **LEDs**: 0603 red C2286 (Basic), white C2290 (Basic), and XINGLIGHT 0603 and 0402 parts in red, green (emerald, InGaN), yellow, blue and white; Lite-On yellow-green C125094 and blue C125103. Forward-voltage ranges are the minimum and maximum of the datasheet's electrical table, read as text from the PDF the JLCPCB record links (the `datasheet:` URL in each `source`). Not read: the Gui guang, CHONGTIAN and TUOZHAN parts (their datasheets do not say clearly at what current Vf was measured), so they are not offered.
- Out of stock when read, so not offered and recorded in `considered.csv`: C1723147 (45.9k, 0.1%), C2505121 (796k, 0.5%) and C4346185 (10R, 2512). Read their availability from the supplier before relying on them.

### How the rows were found

Part names were used only as search keywords in JLCPCB's parts library and LCSC's catalog: the Basic resistors and ceramic capacitors by listing the library type, the E24 Extended 0402 parts by searching the `0402WGF` series to the end. A row is written only if the JLCPCB record and the LCSC record agree on the MPN, the package and the value. Rows dropped for lack of a public source: none.

## Answers

### 300R 0402 1% for the LED example

C25102, UNI-ROYAL 0402WGF3000TCE, exists. It is **Extended**, flagged "preferred" by JLCPCB (no extra feeder fee). There is no Basic 0402 300R. Basic 300R exists only as 0603 (C23025), 0805 (C17617) and 1206 (C17887).

Current check with 5 V +/-5%, Vf 1.8..2.2 V, R +/-1%:

| R | min mA | max mA | fits 8..12 mA |
|---|---|---|---|
| 270 | 9.35 | 12.91 | no (over 12) |
| 300 | 8.42 | 11.62 | yes |
| 330 | 7.65 | 10.56 | no (under 8) |

300R is the only E24 value that works. 330R would need the window widened to about 7.5..12 mA. 270R would need about 9..13 mA.

### LED

No Basic 0603 green LED exists. Real InGaN greens (KT-0603G C12624, NCD0603G1 C84267) have Vf 2.6..3.1 V, so they do not fit 1.8..2.2 V. At 3.1 V a 300R gives only 6 mA.

The nearest green in the file is Lite-On LTST-C190KGKT (C125094), an AlInGaP yellow-green: Vf min 1.9, max 2.4 V @ 20 mA (typ not printed; JLCPCB lists 2 V), dominant 567.5..576.5 nm. It is Extended. Its max Vf of 2.4 V at 20 mA is above the example's 2.2 V. Vf is lower at 10 mA, but the datasheet does not guarantee it.

If a Basic part is acceptable, the red KT-0603R (C2286) has Vf 1.8..2.4 V @ 20 mA.

### Regulator (5 V in, 3.3 V out, 1 uF in, 4.7 uF ceramic out, 3 pins)

| Part | LCSC | Pkg | Lib | Pins | Dropout | Iout max | Cout requirement | Vin max |
|---|---|---|---|---|---|---|---|---|
| XC6206P332MR-G (Torex) | C5446 | SOT-23 | Basic | 1 GND, 2 VOUT, 3 VIN | typ 75 mV / max 350 mV @30 mA; typ 250 / max 680 mV @100 mA | 200 mA (SOT-23 Pd 250 mW) | "Low ESR, ceramic compatible"; tested with CIN = CL = 1.0 uF ceramic | 6.0 V op, 7.0 V abs |
| ME6211C33M5G-N (Micro One) | C82942 | SOT-23-5 | Extended | 1 VIN, 2 VSS, 3 CE, 4 NC, 5 VOUT | typ 120 mV @100 mA, 260 mV @200 mA | 500 mA | tested with CIN = CL = 1 uF; "ceramic compatible" | 6.0 V op, 6.5 V abs |
| AP2112K-3.3TRG1 (Diodes) | C51118 | SOT-25 | Extended | 1 VIN, 2 GND, 3 EN, 4 NC, 5 VOUT | typ 125 / max 200 mV @300 mA | 600 mA | "Stable with 1.0 uF ceramic, tantalum or Al electrolytic"; X5R/X7R advised | 6.5 V abs |
| AMS1117-3.3 (AMS) | C6186 | SOT-223 | Basic | 1 GND, 2 VOUT, 3 VIN, tab = VOUT | 1.1 V typ / 1.3 V max @0.8 A | 1 A | needs the output cap for compensation; "22 uF solid tantalum ensures stability". No ceramic-only support | 15 V abs |

**Recommendation: C5446 (XC6206P332MR-G).** It is the only 3-pin, Basic, ceramic-capable choice.

- **Pin-to-pad map**, KiCad `Package_TO_SOT_SMD:SOT-23`: pad 1 = GND (VSS), pad 2 = VOUT, pad 3 = VIN. Datasheet SOT-23 top view: pin 3 is the lone pin on one side, pins 1 and 2 are on the other.
- **AMS1117-3.3 (C6186)** is the wrong part for the example. The datasheet asks for a 22 uF tantalum output cap, and a 4.7 uF ceramic is not a supported configuration.
- **Power dissipation, `max_power`.** Page 3 of the datasheet (absolute maximum ratings, Ta=25 C) lists Pd for SOT-23 as 250 mW (SOT-89 500, USP-6B 100, TO-92 300) and notes `IOUT = Pd / (VIN - VOUT)`. Read on 2026-11-01 from the datasheet PDF that the JLCPCB record of C5446 links (the same file as the `datasheet:` URL in the CSV row). netsmith uses 0.25 W for the XC6206P332MR-G, not derated for a hotter board.
- **Limit 1: current.** The XC6206 SOT-23 package is limited to 250 mW. With 5.25 V in and 3.3 V out that is about 128 mA. The example states a 0 to 50 mA load on the output, and the solver checks that against the 100 mA modelled limit and the 250 mW.
- **Limit 2: output tolerance.** The datasheet gives 3.3 V +/-2% (3.234..3.366 V) only at 25 C, Vin 4.3 V, Iout 30 mA. The datasheet adds typ +100 ppm/C, load regulation of 60 mV max over 1..100 mA, and line regulation. The worst case over temperature and load is therefore wider than +/-2%. The example's `3.3V +/- 2%` constraint cannot be guaranteed by this part. Either relax it to about +/-4%, or use ME6211C33M5G-N (+/-1%), which is Extended and 5-pin.
- **Limit 3: 4.7 uF output cap.** The datasheet only states 1 uF ceramic for CL and gives no maximum. Larger ceramic is normal practice but not specified. A 4.7 uF 10 V 0402 X5R loses a large share of its capacitance at 3.3 V DC bias; see "Capacitor DC-bias curves" below for the numbers.
- **5-pin alternatives (EN, NC).** Tie EN/CE (pin 3) to VIN. Both ME6211 and AP2112K are active-high with a 1.0 to 1.5 V minimum, and VIN is 5 V. Leave NC (pin 4) unconnected, or give it a no-connect net.

### Capacitors (0402, 10 V or more, X5R/X7R)

| Value | LCSC | Rating | Lib |
|---|---|---|---|
| 100n | C1525 | 16 V X7R +/-10% | Basic |
| 1u | C52923 | 25 V X5R +/-10% | Basic |
| 4.7u | C23733 | 10 V X5R +/-20% | Basic |
| 10u | C315248 | 10 V X5R +/-20% | Extended |

For 10u, no Basic part is 10 V or more in 0402. C15525 is Basic but only 6.3 V.

### Capacitor DC-bias curves

The `effective_capacitance` cell of `catalog/capacitor.csv` gives, for each capacitor that netsmith can pick, the fraction of its marked capacitance that is left at each DC voltage across it. The cell holds the curve as `volts:fraction` pairs from 0 V to the rated voltage. Where the curve is, the test conditions (`measured`) and the `margin` are in the row's `source` cell after `bias-curve:`, and its description is in `notes` after `Bias curve:`.

- **31 of the 58 capacitors have a curve, all Samsung.** `bias-curve: samsung-componentcenter:` rows come from Samsung's own Component Library, `https://weblib.samsungsem.com/mlcc/mlcc-ec-data-sheet.do?partNumber=<MPN without its last letter>`. The page embeds the "DC Bias" series as numbers (`datasheetData`, graph type `DCBias`, 25 C), from 0 V to the rated voltage. The last letter of the MPN is the packaging code and the tool leaves it off (`CL05A105KA5NQN` for C52923). Re-check one with `curl -sL 'https://weblib.samsungsem.com/mlcc/mlcc-ec-data-sheet.do?partNumber=CL05A105KA5NQN'` and read the `DCBias` block; the first Y value is the change in percent at 0 V and the X values are volts.
- **C23733 (CL05A475MP5NRNC) uses Samsung's characteristics sheet instead**, `https://media.digikey.com/pdf/Data%20Sheets/Samsung%20PDFs/CL05A475MP5NRNC_Char.pdf`, graph 3 (1 kHz, 0.5 Vrms). Its curve is deeper than the Component Library's 1 Vrms series for the same MPN (about -70% against -57% at 4 V, -90% against -85% at 10 V), and the lower one is the safe one. I took the points off the graph by pixel position against the gridlines (reading error at most 1 point). The same sheets for C47339 (CL05A474KP5NNNC) and C15525 (CL05A106MQ5NUNC) agree with the Component Library to within a point or two, and were not used further.
- **Murata C3845593 (22 uF 10 V) has no curve.** The datasheet LCSC links for it (`datasheet.lcsc.com/datasheet/pdf/8d242c3da788daca84b3abba710d6196.pdf`) is Murata's generic reliability text and has no per-part curve, and Murata's product page (`pim.murata.com`) returns no data without a browser. SimSurfing was not tried. So the part is in the catalog, and the solver refuses it whenever a design constrains the effective capacitance (ADR 0013); a design that names only its marked `.capacitance` may use it.
- **Parts without a curve (27).** The Murata (below); the Fenghua (FH) and Yageo parts, whose makers publish no DC-bias data I could fetch (their C0G and small X7R values, 21 rows); and five Samsung class II parts of 2.2 uF or more: C12530 (2.2 uF 6.3 V), C23630 (2.2 uF 16 V), C19666 (4.7 uF 16 V), C19702 and C96446 (10 uF 10 V and 25 V). Their Component Library curve is measured at 1 Vrms, and for the one part where both exist (C23733) that curve is much shallower than the 0.5 Vrms characteristics sheet (the section above), so a curve for them from the Library alone could promise capacitance that is not there. The sheets for the 0603 parts could not be downloaded. They can be marked-value parts only. The Samsung C0G curves are flat (the Library publishes 0 change), so they are 1 at 0 V and 0.99 elsewhere, the margin. A test (`a_capacitor_without_a_curve_is_one_of_the_recorded_cases`) pins this rule.
- **What the curves say**, as the fraction left, as in the cell (so with the margin taken off, before the +/-10% or +/-20% tolerance), at 3.465 V and 5.25 V, the top of the regulator example's two rails: C52923 0.70 and 0.52; C1525 0.95 and 0.89; C16772 0.92 and 0.83; C15195 0.99 and 0.98; C47339 0.62 and 0.44; C315248 0.35 and 0.22; C23733 0.32 and 0.19; C15525 0.35 and 0.21; C16780 0.43 and 0.28.
- **How the curves are made.** Each curve is the source series sampled at 20 equal steps from 0 V to the rated voltage, each fraction rounded down to 0.001, then lowered by `margin` (0.01 for the Component Library rows, covering the straight line between samples, which was at most 0.7 points above the source; 0.02 for the C23733 graph, adding its reading error). A fraction above 1 (100 nF X7R rises 0.2% at 1 V) is held at 1.
- **What is not covered.** A curve is the typical value at 25 C, 1 kHz and the AC level in `measured`; Samsung says its data are typical, for design reference. The effective capacitance is the marked value with its tolerance times the fraction on this curve (the tolerance of the part is in its `capacitance` cell). **Temperature is not modelled.** Samsung's Component Library pages carry a typical temperature curve (`TCC`) for each part and the characteristics sheet of C23733 draws it, but no page read states the X5R/X7R class limit as a number for these parts, and one from memory is not recorded here. Ageing (class II capacitors lose capacitance over time), the AC level in use and the manufacturer's tolerance on the curve are not modelled either. No datasheet or standard was found that bounds a whole class of capacitors, so there is no class-level fallback: a part without a curve has none.

### Pin headers

Through-hole, hand-soldered (`catalog/header.csv`, ADR 0040). Fetched 2026-11-02 as for the other standard kinds (JLCPCB record and LCSC record, which agree on the model), plus the maker's datasheet drawing, read as an image because the PDFs have no text: each states `Current Rating: 3 Amps`, the rating of one contact, and the JLCPCB record says 3A. Eight rows, all Extended, one for each of 2, 4, 8 and 10 contacts, single row, 2.54 mm pitch:

- Headers: JXTCONN `PH2.54-1XnP-H25` for 2 (C42431798), 4 (C42431808) and 10 (C42431810) contacts, whose drawing (`9b94c2ed...pdf`) covers 1x1 to 1x40. JLCPCB has no JXTCONN 1x8, so the 8-contact header is Hanxia `HX PZ2.54-1x8P ZZ` (C32713274), whose approval sheet (`85243369...pdf`, text) gives 3 A under its electrical performance for the whole family; the AC and DC boxes of that line are both blank in the text.
- Sockets: JXTCONN `PM2.54-1XnPM-H85` for 2 (C42431841), 4 (C42431839) and 8 (C42431840) contacts (drawing `959b1cc7...pdf`), and `PM2.54-1XnP-H85` for 10 (C42431852; drawing `970ff14a...pdf`), because the `PM` series has no 10-contact socket in the JLCPCB library.
- `mating.pin_current` is the rating of one contact. No datasheet read gives a derating for several contacts loaded at once or for temperature, so none is modelled and N contacts carry N times it. The label of a row is the KiCad symbol's name (`Conn_01x10`, `Conn_01x10_Socket`), so the value field of a schematic reads as it did before the part had a row.
- Not rated, so no row: the two-row headers `Header2x3`, `Header2x5` and `Header2x10`. They stay unrated, and the check of the current through a mate lists them as not evaluated.

## V-cut panel rules (`std::mfg::jlcpcb::VCutPanel`)

Not parts: the numbers of `VCutPanel` in `std/mfg/jlcpcb.nsm` (ADR 0040), read from the pages on 2026-11-02 with WebFetch (the page text, summarised by a tool, so each figure was asked for by name and is quoted below). Nothing is from memory.

| Rule | Figure | Page |
|---|---|---|
| Copper clearance from a V-cut edge | 0.4 mm | capabilities |
| Copper from the V-cut centreline | at least 0.4 mm, 1.0 mm recommended (stated as a note, not a refusal) | blog |
| Panel size | at least 70 x 70 mm, at most 475 x 475 mm | capabilities, help (agree) |
| Panel for assembly | "not bigger than 250x250mm" | assembly FAQ 10 |
| Board spacing | zero by default | capabilities |
| Lines | at most 25 on either side; straight, across the whole panel | help |
| Two V-cuts | at least 2 mm apart, 3 mm recommended | capabilities |
| Groove angle | 25 degrees (not used by netsmith) | capabilities |
| Thickness | "PCB thickness >= 0.6 mm" (V-cut tolerance row); not on 0.4 mm | capabilities, help |
| Marking the lines | "mark V-CUT on the same layer as the board outline"; V-cut lines, slots and cutouts "in the same layer with the board outline" or "it will be missed" | blog, ordering instructions |
| Rails and marks | rails 5 to 8 mm; 5 mm tooling edges, 2 mm tooling holes, 1 mm fiducials centred 3.85 mm from the panel edges; with Gerbers the customer adds fiducials and tooling holes | capabilities, blog |
| Designs | at most 10 designs in a panel made by the customer | ordering instructions |
| Assembly | V-cut "not supported in Economic Assembly", available in Standard | help, assembly FAQ 11 |

Pages: capabilities `https://jlcpcb.com/capabilities/pcb-capabilities`; help `https://jlcpcb.com/help/article/pcb-panelization`; blog `https://jlcpcb.com/blog/v-cut-panelization-standards`; ordering instructions `https://jlcpcb.com/help/article/instructions-for-ordering`; assembly FAQ `https://jlcpcb.com/help/article/pcb-assembly-faqs`.

Where the pages differ the stricter number is stated: the panel for assembly is held to 250 mm (the assembly FAQ) and not 475 mm; the copper clearance is 0.4 mm and the blog's 1.0 mm is advice; rails are required to be at least 5 mm (the capabilities page's tooling edge minimum is 3 mm, its assembly figure and the 5 to 8 mm recommendation are 5 mm). The blog also gives a minimum connecting edge of 3 mm (5 mm for boards 0.8 mm thick and under) where a V-cut meets routed slots, which only the routed panels of P2 need.

**Not found on any page, so unverified:** the designators JLCPCB accepts in the BOM and CPL of a panel made by the customer (the help pages say nothing; a 6-year-old EasyEDA forum thread has a user guessing that the CPL follows the panel); the V-cut file format their Gerber viewer reads beyond the instruction above (a forum user reports a V-cut on a user layer appearing in the outline layer of their viewer); the mask opening round a fiducial (netsmith uses the project's own `Fiducial_1mm_Mask2mm`, twice the copper); the tab count of a routed panel. JLCPCB has not accepted a netsmith panel.

## Datasheet numbers in `std/` (not catalog rows)

A part in `std/` that states numbers from a datasheet records the document and the table of each number in its doc comment, and here.

- `std/parts/logic.nsm`, `Sn74lvc1g04` (ADR 0046). Datasheet: Texas Instruments, SN74LVC1G04 Single Inverter Gate, SCES214AF (April 1999, revised October 2025), https://www.ti.com/lit/ds/symlink/sn74lvc1g04.pdf, fetched on 2026-11-02 (10 pages; SHA-256 begins 5322c1ab3cd3bd3f). Tables read from the PDF text, not from a summary: 4 (Pin Functions, DBV column: NC 1, A 2, GND 3, Y 4, VCC 5), 5.1 (Absolute Maximum Ratings: VI -0.5 V to 6.5 V; VO high or low state -0.5 V to VCC + 0.5 V), 5.3 (Recommended Operating Conditions: VIH 2 V and VIL 0.8 V at VCC = 3 V to 3.6 V) and 5.5 (Electrical Characteristics: VOH min VCC - 0.1 V at IOH = -100 uA and VOL max 0.1 V at IOL = 100 uA, VCC = 1.65 V to 5.5 V, the same for -40 C to 85 C and to 125 C). The KiCad symbol is `74xGxx:74LVC1G04`, whose Datasheet field names the same document. No LCSC number is recorded for it, so it has no catalog row and `fab` refuses a board with it until one is added with its evidence.

## What was not verified

- The jlcpcb.com HTML part pages were not read; the data came from the JLCPCB part records.
- C22369487 (23.7k 0402) appears in JLCPCB's records but LCSC has no record of it, so it is left out.
- Search coverage: JLCPCB's search returns a limited number of results for a keyword. "No Basic 270R/300R 0402" is based on scans of all returned rows for "0402 270Ω" and "0402 300Ω". The Basic LDO list came from keyword scans, so it is not guaranteed complete.
- The LCSC-hosted datasheet URLs in the `source` cells are canonical but block scripted download. The same PDFs were read from the file link of the JLCPCB record and their text extracted with `pdftotext`.
- Price and stock are not shipped (see the top of this file), so nothing here says that a part can be bought today.
- JLCPCB Extended parts normally add a one-time loading fee per unique part unless flagged Preferred. This was not checked on the order page. Preferred rows: C25102, C47339.

## Refreshing the rows

`netsmith catalog refresh` repeats **the LCSC cross-check only** for each row; JLCPCB data is not fetched (ADR 0039, decisions of 2026-11-02). It compares LCSC's manufacturer's number and package with the row's and, for a row that agrees (`VERIFIED`), writes the day in the `checked` column (`--dry-run` shows the diff, `--lcsc C5446` does one row). **It never writes `price`, `stock`, `price_breaks`, `library` or `date`.** A row whose LCSC record has another `mpn` or package than the row is a CONFLICT, and a part LCSC no longer lists is MISSING; both are left as they were. A new row's `checked` is empty until a refresh verifies it. LCSC's terms of use for the record the command asks for have not been read; a person must read them before the command is advertised.

A project that keeps its own prices and stock in its own catalog files (`price`, `stock`, `date`, ADR 0022) gets them in its bill of materials as the last resort, after a supply provider's answer, with the age of each `date` and a stale mark after 90 days; a standard row has none of them, so its line says its price and stock are unknown unless a provider has the part, and `netsmith catalog age` counts it as unknown.

## Re-checking one row

Look the number up on lcsc.com for the manufacturer's number and package, and on JLCPCB's parts page (jlcpcb.com/parts) for the library type, price and stock. A person does this by hand; nothing in netsmith reads JLCPCB's price, stock or library. Official APIs (JLCAPI, the LCSC API) need an account and are future work.

## How netsmith uses these files

`Catalogs::jlcpcb()` in `netsmith-solve` embeds every `catalog/*.csv` (`build.rs` lists the directory, so a new file needs no Rust) and a part that lists `catalog resistor { ... }` reads `resistor.csv` against its columns when the solver picks. `src/jlcpcb.rs` is only the loader. Each file is ordered Basic first, then by the prices read on a past date (not shipped, so that part of the order cannot be re-checked), then by ascending value, so the solver takes the cheapest Basic part that works. A test checks that every row has its evidence and no price, stock or date, that Basic rows come before Extended ones, that every curve names its source and ends at the rated voltage, and that nothing in `considered.csv` is offered.

The modelling choices are recorded here, and for the LEDs and the regulator in the rows' `notes`: resistors are 0402 +/-1% without the 0 ohm link (a jumper has no tolerance and a zero would let any current through); capacitors carry the tolerance and rated voltage from the supplier's record, their DC-bias curve as `effective_capacitance`, and `capacitance` as the marked value; the 1206 100 uF is not offered because KiCad has no footprint for it here; the LEDs' forward-voltage ranges are the datasheet minimum to maximum at the test current, and the Lite-On yellow-green C125094 is the "GREEN" LED; the XC6206 is +/-5% output (the datasheet's +/-2% holds only at 25 C), 0.68 V dropout, 0.1 A and 0.25 W. With these catalogs `examples/led.nsm` picks the Basic 0603 390 ohm C23151: the 330 ohm 0402 C25104 would carry up to the 15 mA of the window through up to 5.9 V, 88 mW, over its 62.5 mW (the 300 ohm C25102 is Extended and not needed once the window is 5..15 mA).
