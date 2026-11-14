# Glossary: I

**Inner layer**:
A copper layer between the outer two, `.process.inner1` counted from the top: a _Plane_ or a _Signal layer_, as the board says (ADR 0111).
_Avoid_: Internal layer, mid layer

**Interface**:
A typed bundle of pins and quantities that is connected as one. A bundle of quantities with no pins, such as a process, is also an interface. A member of an interface type is a port (`.supply: Rail`); what picks a part is a generic, not an interface. An interface of a bus (`Spi`) also has a `lines` table.
_Avoid_: Bus (for an interface in general: a **Bus** is the interfaces of one definition that share a net, and the _Bus drawing_ is how the schematic draws the members of one), bundle, group

**Island**:
The part of the network between the nets that a supply holds: the nets no source holds, joined by the elements between them, and the held nets at its edge, whose range is one of its values. An island is solved on its own, and one that has a pin nothing models, or too many combinations, is left to the bound from Ohm's law (ADR 0076).
_Avoid_: Subnet, partition, section, cluster
