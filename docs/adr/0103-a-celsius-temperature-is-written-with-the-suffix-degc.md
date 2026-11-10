---
status: proposed
---

# A Celsius temperature is written with the suffix `degC`

Owner decision of 2026-11-10, written by an agent and not yet read by the owner. Meets the first and third tests: the suffix is written in every project's boards, manifests and catalog cells, so changing it would change them; and it rejects the obvious spelling, `-40C`, which the research note recommended and ADR 0099 left open.

**Decision.** `degC` is a unit of the dimension `Temperature`: `0degC` is 273.15 K, and `-40degC..125degC` is the range 233.15 K to 398.15 K. It is the only offset unit, and the offset is applied once, where a literal is read: every value is kelvin from then on, a temperature in either spelling is the same value, and a catalog cell may be written in either. A sign written directly on a literal belongs to its number (`-40degC` is 233.15 K, not minus 313.15 K); the checker and the catalog reader both read it so. `degC` takes no prefix.

**Why.** A datasheet prints Celsius, and a board written from one should read like it. `C` is the coulomb (`Charge`, `nC`) and stays it, so a suffix that is a unit of its own could not be `C`.

**Arithmetic.** The narrowest reading, with no new type: a quantity holds kelvin and does not remember whether it is a temperature or a difference. A difference is therefore written in kelvin (a rise, `.max_temperature_rise`, the Zener's `.temperature_shift`, a tolerance): `10degC + 5K` is 15degC (288.15 K), `20degC - 10degC` is 10 K, `85degC +/- 5K` is 80degC to 90degC. A tolerance written in `degC` is refused, because its offset would be added to the centre as if it were a difference. The sum of two Celsius literals is 576.3 K and is not refused.

Rejected. Retiring the coulomb so that `C` can be Celsius: nothing in std uses it, but a charge could then no longer be written in the plain unit, and an `85C` in a file written before the change would silently become a temperature. Writing kelvin only (`358.15K`): it is what a message had to show beside a Celsius figure, and it makes every datasheet figure a conversion for the author. `°C` as the suffix: a symbol that no keyboard has, where the prefixes `µ` and `Ω` are already accepted only as alternatives to `u` and `ohm`.

Cost. Two spellings of one temperature, `degC` and `K`. One correct way (`docs/DESIGN-RULES.md`) is kept by what each is for: `degC` for a temperature as a datasheet prints it, `K` for a difference. A sum of two Celsius literals is not an error. A message shows kelvin first and the Celsius figure beside it, because the quantity's own printing is kelvin.

Left open, for the owner. Whether a quantity should carry "temperature or difference", so that `10degC + 20degC` and `85degC +/- 5degC` are refused by type and a rise has its own unit; that needs a new dimension, and is the only way to make the arithmetic exact. Whether a bare temperature should print in `degC` in every message and not kelvin first. Whether the catalog's cells should move to `degC`, which is a change of data and no change of meaning.
