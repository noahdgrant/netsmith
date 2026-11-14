# The netsmith language

netsmith reads `.nsm` files. A file describes boards: the parts on them, the nets that join the pins, and the requirements the parts must meet. netsmith checks the file, picks real parts that meet the requirements in the worst case, and writes files for KiCad and for a board house.

This guide has a quick start, then one section for each construct of the language. Each section gives what the construct means, its syntax, a complete example, its rules, and the message you get when you misuse it. The last section lists the standard library.

`netsmith init` copies this guide into a project: this page as `docs/netsmith/LANGUAGE.md` and the chapters into `docs/netsmith/language/`. Vocabulary is in `GLOSSARY.md` and the reasons for the design are in `docs/adr/`, in netsmith's source repository.

## The chapters

The guide is one file for each chapter in `docs/language/`, in the order below. A test reads every one of them.

- [How to read the examples](language/01-how-to-read-the-examples.md)
- [Quick start](language/02-quick-start.md)
- [Definitions: board, part, interface, module, system, panel](language/03-definitions.md)
- [Members](language/04-members.md)
- [Values: units, ranges and tolerances](language/05-values.md)
- [Enums: a value from a fixed list](language/06-enums.md)
- [Narrowing and relations](language/07-narrowing-and-relations.md)
- [Connections: `~` and `~>`](language/08-connections.md)
- [Nets and net names](language/09-nets-and-net-names.md)
- [Parts](language/10-parts.md)
- [Pins, packages and ties](language/11-pins-packages-and-ties.md)
- [Catalogs and how the solver picks parts](language/12-catalogs-and-how-the-solver-picks-parts.md)
- [Interfaces](language/13-interfaces.md)
- [Generics and providers](language/14-generics-and-providers.md)
- [Attributes](language/15-attributes.md)
- [Modules](language/16-modules.md)
- [Systems and ports](language/17-systems-and-ports.md)
- [Panels](language/18-panels.md)
- [Load current](language/19-load-current.md)
- [Capacitors: marked value, effective capacitance and bias](language/20-capacitors.md)
- [The process of a board](language/21-the-process-of-a-board.md)
- [The environment of a board](language/22-the-environment-of-a-board.md)
- [The shape of a board](language/23-the-shape-of-a-board.md)
- [Imports](language/24-imports.md)
- [Comments](language/25-comments.md)
- [Projects and netsmith.toml](language/26-projects-and-netsmith-toml.md)
- [Commands](language/27-commands.md)
- [Standard parts](language/28-standard-parts.md)
- [The standard library](language/29-the-standard-library.md)
