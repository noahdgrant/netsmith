# Only a board names nets

A net name is board-wide: every pin that asks for `GND` is on the one net called `GND`. So a module or part that named a net would give every instance of itself the same name and silently join them, which is wrong for a module's internal nodes and right only for a rail. The compiler therefore rejects `.net &=` outside a `board`; the board names a module's nets through the module's rails and pins (`.stage.vout.pos.net &= "3V3"`), and unnamed nets are generated from the instance path, so two instances never collide. We considered per-instance name scopes, as hierarchical schematics have, and rejected them for now because the board is the only place that knows which nets are the same rail.

The rule is on any field called `net`, not only a pin's: the net of an inner-layer plane in a manufacturing process is a net name too (ADR 0012), so a module cannot set it either.
