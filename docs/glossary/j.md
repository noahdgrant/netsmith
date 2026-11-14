# Glossary: J

**Join by name**:
What `~` does between two ports that are parts, modules or interfaces of different kinds, `.headers ~ .module { gpio4 = io4 } except { gnd }` (ADR 0117): it joins their pins and pin-holding interfaces pairwise by name, a rename map pairs members whose names differ, `except` leaves members out on both sides, and a member with no partner is an error. It makes the nets the `~` lines would.
_Avoid_: bulk join, auto-connect, wildcard join
