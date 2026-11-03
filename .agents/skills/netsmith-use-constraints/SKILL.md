---
name: netsmith-use-constraints
description: State where parts must be on a board (a case, a panel, a carrier) as requirements in the source, run fab, and read the constraint findings. Use when a user gives positions, distances from an edge, rotations or a face for parts, or when fab reports `[constraint]`.
---

# Use constraints

A requirement on where a part goes is a line of source on the part's built-in `.place` (ADR 0045). The placer keeps to it, the layout file keeps the solved position, and `netsmith fab` checks it and stops when it is not met. The language guide's section "Where a part goes" has the fields and the real errors.

## Loop

1. **Get the numbers from the user's drawing.** Ask for the case, panel or carrier drawing, or for the numbers. Never invent one: an unstated requirement is better than a guessed one. Put the source in a comment above each line (`// Case drawing, slot A: ...`). A DXF of the case gives the outline, cutouts and mounting holes with `netsmith dxf import` (see `netsmith-design-a-board`), but not the requirements on parts: read those off the drawing yourself. A STEP model or a PDF is read with tools outside netsmith; turn it into numbers (millimetres, from the board's top left corner, y down) and write those.
2. **Give the board a fixed outline** when a requirement is measured against its size: `.outline: Outline { .width &= 62mm  .height &= 30mm }`. Positions are from the outline's top left corner. A requirement written with `.outline.height` on an outline that is a range is refused.
3. **Write the lines.** `.<part>.place.<field>` where the field is `x`, `y` (the centre of the part's body), `left`, `right`, `top`, `bottom` (the body's edges), `rotation` (degrees, a multiple of 90) or `side` (`"top"` or `"bottom"`).

   ```nsm-fragment
   // Case drawing, slot A: the receptacle's shell is centred and its mouth sticks out 0.5 mm.
   .usb.place.left     &= -0.5mm +/- 0.05mm
   .usb.place.y        >= .outline.height / 2 - 0.1mm
   .usb.place.y        <= .outline.height / 2 + 0.1mm
   .usb.place.rotation &= 270
   .usb.place.side     &= "top"
   ```

   The body is the outline the footprint draws on `F.Fab`, else its courtyard. A tolerance (`+/-`) needs a constant on both sides; a window round an expression is two bounds (`>=`, `<=`).
4. **A part inside a module** cannot be reached by the board. A module that wants one placed declares a length (`.slot_y: Length`) and ties the part to it (`.j.place.y &= .slot_y`); the board narrows `.block.slot_y`. Do not do it unless the user needs it. A module's own `.place.left`, `.right`, `.top`, `.bottom` bound the box of all its parts.
5. **A part that hangs over the board edge** (a plug that sticks out of the case) is allowed only where the requirement says so: a window for the body's own `left`, `right`, `top` or `bottom` that is at or beyond the same edge of the outline (`.plug.place.bottom >= .outline.height`, or `.usb.place.left &= -0.5mm +/- 0.05mm`). A window on `x` or `y` does not say it, and neither does a bound on a module. A part the layout file locks may hang over wherever its courtyard is. This rule is provisional (ADR 0045, section 10): tell the user you used it. The pads must still be inside the outline and the process's copper to edge space away from it, and the body is not checked.
6. Run `netsmith build`, then `netsmith fab`. Read the first error only.

## Reading a finding

```text
error [constraint] constraint usb.place.left (widget.nsm:14) is violated: the left edge of the body of J1 (F.Fab) is 0.18 mm from the left edge of the outline, and it allows -0.55 to -0.45 mm (0.73 mm over)
```

It names the line to change, the part, the drawing it measured, the measured value, the window, and how far over. Change the range if you misread the drawing, or change the circuit. Never edit the layout file to hide a failure: the check measures the layout file's positions too, and a part `lock`ed there against a requirement is refused before placing ("is locked in the layout file ... unlock it and let the placer move it, or change the requirement"). A message that says no position meets the requirements names each one with its numbers: two lines that cannot both hold, or a window narrower than the part.

A violated requirement stops `fab`, and `--allow-drc-violations` does not waive it (it waives design rules, not requirements). Do not weaken that on your own. Only `constraints = "warn"` under `[project]` in `netsmith.toml`, or `netsmith fab --constraints warn` for one run, lists them as warnings; `--constraints error` forces errors for one run over a manifest that says `warn`. If the user wants to see the board anyway, ask first, use the flag for that one run, and say so in your report.

A courtyard past an edge nobody stated is `J1 is not wholly inside the board outline: its courtyard is 4.00 mm past the right edge, and the design does not say it may hang over an edge; ...`. It names the edge, how far, and the line that would allow it (`.plug.place.right >= outline.width`). Do not add that line to make the error go away: ask whether the part is meant to stick out. `a pad of J1 is 13.00 mm outside the board edge` is never allowed by an overhang: move the part. If the placer says `the parts need ... placing them in the outline anyway ended with ...`, the parts do not fit the outline, and the numbers say how many pairs overlap and how many parts are outside; it then lists the ten worst pairs and parts by name (area, place, edge) and names the rest. A body whose centre is off the board is fine when an edge field (`.left`, `.right`, `.top`, `.bottom`) states the overhang (provisional); the same window on `.x` or `.y` is refused.

## Report

Tell the user which requirements you wrote and from which drawing, each one `fab` reports as not met with its numbers, and the overhangs you stated (and that the rule is provisional), what is not built yet: a relation between two parts (a pitch, an offset), part heights and height-limited zones, and a polygon outline (`dxf import` refuses an outline that is not a rectangle with equal rounded corners).
