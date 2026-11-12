//! The `netsmith` command line: one file per command in `commands/`. The binary is `main.rs`;
//! the library lets the tests read clap's model of the commands.

pub mod commands;
mod view_client;
