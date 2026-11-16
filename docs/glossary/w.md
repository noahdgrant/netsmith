# Glossary: W

**Waiver**:
An entry of the _Layout file_ that says why the board does not meet a guideline of a part's _Layout block_: `[Board.waive."reg.ic.input_cap"]` with `reason = "..."`, keyed by the part's path and the guideline's name (ADR 0030, issue #994). `fab` then says the guideline is waived, with the reason, and neither warns nor stops on it; a waiver of a guideline the board does not have, or with no reason, is an error. It is a decision of the design, so it is in the layout file and never in the part.
_Avoid_: Exception, suppression (of a guideline)

**Walkaround**:
The way the editor's track tool draws the next segment of a track from its last corner to the pointer (issue #726): the straight segment where it keeps every space the design rules check asks of a track, else a way with corners of 45 degrees at most round the pads, the copper of other nets, the keepouts and the board edge in the area round the two ends, or the reason there is none, which names the obstacle (`netsmith_router::walk`). The corners it chooses become ordinary corners of the track when the pointer clicks; another layer picked while the track is drawn puts a via at its last corner and the walk goes on from it on that layer (issue #1009); the track is still ended through the routes file's check. The router's copper is no obstacle (an edit of the routes file throws it away) and the walkaround says which of it the track crosses, for the view to show it faded; unlocked copper of the routes file is moved out of the way first where it can be (_Shove_), and the walkaround goes round what stays (locked shapes, pads) and what could not move far enough.
_Avoid_: autoroute (the router routes a whole board), obstacle avoidance
