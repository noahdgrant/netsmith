# Glossary: W

**Walkaround**:
The way the editor's track tool draws the next segment of a track from its last corner to the pointer (issue #726): the straight segment where it keeps every space the design rules check asks of a track, else a way with corners of 45 degrees at most round the pads, the copper of other nets, the keepouts and the board edge in the area round the two ends, or the reason there is none (`netsmith_router::walk`). The corners it chooses become ordinary corners of the track when the pointer clicks; the track is still ended through the routes file's check. No copper moves out of the way: that is shove, not built.
_Avoid_: autoroute (the router routes a whole board), obstacle avoidance
