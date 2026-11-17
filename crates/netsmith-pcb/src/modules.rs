//! The module instances of a board, read from its parts' paths, and the colours a module is lit
//! in (issue #1129).
//!
//! **A module** is every prefix of a part's path that ends before a `.` or a `[`: the part
//! `stage.bank[0]` is in `stage` and `stage.bank`. A part is in the module at `module` when its
//! path is `module` or starts with it and a `.` or a `[` (`stage` holds `stage.c1`, not
//! `stage2.r1`). This is how `layout regenerate`, the editor's Modules tab and `--highlight
//! module:<path>` read a module.
//!
//! **Colours.** Several modules lit at once each take their own colour, in the order they are
//! named: the first [`MODULE_COLOURS`], then the next, round again after the last. There is one
//! set for a light background and one for a dark one, each at least 3:1 against the board's
//! background of that theme, and none the selection's blue or the highlight's amber. The
//! editor's themes have the same colours (`--module-1` to `--module-6` in `themes.css`).

use std::collections::BTreeSet;

/// How many colours the modules lit take before the first is used again.
pub const MODULE_COLOURS: usize = 6;

/// The modules' colours on a light background (`#rrggbb`).
pub const MODULE_COLOURS_LIGHT: [&str; MODULE_COLOURS] = [
    "#c2185b", "#1b8a5a", "#7b4fd6", "#0e7f99", "#c4471b", "#6f7f00",
];

/// The modules' colours on a dark background (`#rrggbb`).
pub const MODULE_COLOURS_DARK: [&str; MODULE_COLOURS] = [
    "#ff5fa2", "#3ddc97", "#b48cff", "#22d3ee", "#ff7a45", "#d4e157",
];

/// The colour of the `index`-th module lit (from 0), on a dark background or a light one.
pub fn module_colour(index: usize, dark: bool) -> &'static str {
    let set = if dark {
        &MODULE_COLOURS_DARK
    } else {
        &MODULE_COLOURS_LIGHT
    };
    set[index % MODULE_COLOURS]
}

/// A `#rrggbb` colour as red, green and blue.
pub fn rgb_of(hex: &str) -> [u8; 3] {
    let h = hex.trim_start_matches('#');
    let at = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    [at(0), at(2), at(4)]
}

/// Whether the part at `path` is in the module at `module` (or is that part).
pub fn in_module(path: &str, module: &str) -> bool {
    !module.is_empty()
        && path
            .strip_prefix(module)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.') || rest.starts_with('['))
}

/// The modules of the parts at `paths`: every prefix of a path that ends before a `.` or a `[`,
/// sorted, each once.
pub fn module_paths_of<'a>(paths: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut all = BTreeSet::new();
    for path in paths {
        for (i, c) in path.char_indices() {
            if (c == '.' || c == '[') && i > 0 {
                all.insert(path[..i].to_string());
            }
        }
    }
    all.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_module_is_a_prefix_that_ends_before_a_dot_or_a_bracket() {
        let paths = [
            "stage.c1",
            "stage.bank[0]",
            "stage.bank[1]",
            "stage2.r1",
            "j1",
        ];
        assert_eq!(
            module_paths_of(paths),
            ["stage", "stage.bank", "stage2"].map(String::from)
        );
        assert!(in_module("stage.bank[0]", "stage"));
        assert!(in_module("stage.bank[0]", "stage.bank"));
        assert!(!in_module("stage2.r1", "stage"));
        assert!(!in_module("stage.c1", ""));
    }

    #[test]
    fn the_colours_go_round_and_read_as_numbers() {
        assert_eq!(module_colour(0, true), module_colour(MODULE_COLOURS, true));
        assert_ne!(module_colour(0, false), module_colour(1, false));
        assert_eq!(rgb_of("#ff5fa2"), [0xff, 0x5f, 0xa2]);
    }
}
