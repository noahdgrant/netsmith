# Glossary: Z

**Zone**:
Copper of a _Routes file_'s record, KiCad's zone of the net: filled inside an outline less its holes, on one or more copper layers, with the settings of KiCad's zone that change the copper (priority, clearance, minimum width, how it joins pads, thermal gap and spoke width, island removal), as `netsmith routes import` takes it from a KiCad board or `netsmith routes add-zone` draws it (issues #724, #599). One of the net of the design's _Pour_ or _Plane_ on its layer restates it, in place of it; one of another net cuts it, having the area it covers while the pour is filled round it, as in KiCad (issue #1314); where two zones overlap, the higher priority has the area. Its fill joins what of its net it reaches.
_Avoid_: Copper area, fill (the copper a zone makes), pour (the design's own, stated in the source)
