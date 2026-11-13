---
status: superseded by ADR-0116
---

# Modules are configured only through their interface

A module's rails, pins and declared values are its interface, and the parts and modules inside it are not: a board cannot write `.stage.regulator.vout.voltage`, it sets `.stage.vout.voltage` and the module passes that on. ato v2 makes every field of every node reachable, so a board can reach in. We refuse it because a second path to the same value breaks ADR 0001, and because a board that names a module's internals breaks when the module changes how it is built. The cost is that a module author must expose, as a rail or a declared value, anything a user may need to set.

ADR 0042 gives an example of a declared value that a module ties to its parts: who assembles them.

Qualified by ADR 0048: a board's physical file may reach a part inside a module by path, read-only, to state physical intent. The circuit source still may not.
