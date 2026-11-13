//! Packing the pieces of a sheet into rows.

use super::*;

/// How far a row reaches above and below the line its pieces are level on.
pub(super) fn row_height(pieces: &[Piece], row: &[usize]) -> (f64, f64) {
    let above = row
        .iter()
        .map(|&i| pieces[i].middle - pieces[i].rect.min.y)
        .fold(0.0, f64::max);
    let below = row
        .iter()
        .map(|&i| pieces[i].rect.max.y - pieces[i].middle)
        .fold(0.0, f64::max);
    (above, below)
}

/// How wide a piece counts when pieces are cut into rows: its width up to the next multiple of
/// [`SLOT_X`], so that a little growth (a longer value, one more capacitor beside a pin) does
/// not move the end of a row onto the next piece. (The pieces stand as close as before.)
pub(super) fn slot_width(piece: &Piece) -> f64 {
    snap_up(piece.rect.width(), SLOT_X)
}

/// The space between a row and the one under it: the grounds' rails of the first and the
/// supplies' of the second need room, and so do their symbols.
pub(super) fn row_gap(pieces: &[Piece], upper: &[usize], lower: &[usize]) -> f64 {
    let rails = |row: &[usize], ground: bool| -> usize {
        row.iter()
            .flat_map(|&i| {
                if ground {
                    pieces[i].grounds.iter()
                } else {
                    pieces[i].supplies.iter()
                }
            })
            .collect::<BTreeSet<_>>()
            .len()
    };
    ROW_MARGIN + rail_room(rails(upper, true)) + rail_room(rails(lower, false))
}

/// The pieces in rows, as indices into `pieces`: one row if the sheet comes out no wider than
/// [`MAX_ASPECT`] that way, else as many rows as bring it nearest [`TARGET_ASPECT`]; the
/// pieces keep their order, a row is filled from the left.
pub(super) fn pack(pieces: &[Piece]) -> Vec<Vec<usize>> {
    if pieces.is_empty() {
        return Vec::new();
    }
    let width = |row: &[usize]| -> f64 {
        row.iter().map(|&i| slot_width(&pieces[i])).sum::<f64>()
            + BLOCK_GAP * (row.len() - 1) as f64
    };
    let rails = |row: &[usize], ground: bool| -> usize {
        row.iter()
            .flat_map(|&i| {
                if ground {
                    pieces[i].grounds.iter()
                } else {
                    pieces[i].supplies.iter()
                }
            })
            .collect::<BTreeSet<_>>()
            .len()
    };
    let shape = |rows: &[Vec<usize>]| -> f64 {
        // The frame has a border round the drawing and a strip for the title above it, and
        // the first row's supplies and the last row's grounds need room too.
        let w = rows.iter().map(|r| width(r)).fold(0.0, f64::max) + 2.0 * BORDER;
        let mut h = TITLE_ROOM
            + 2.0 * BORDER
            + rail_room(rails(&rows[0], false))
            + rail_room(rails(&rows[rows.len() - 1], true));
        for (k, row) in rows.iter().enumerate() {
            let (above, below) = row_height(pieces, row);
            h += above + below;
            if k > 0 {
                h += row_gap(pieces, &rows[k - 1], row);
            }
        }
        w / h.max(1.0)
    };
    let all: Vec<usize> = (0..pieces.len()).collect();
    let one = vec![all.clone()];
    // A small sheet stays as it is: a few parts are not worth a second row. So does a sheet of
    // a spine and a few more pieces: the spine is the row, and a connector that sets the power
    // going in is at its left end, not above it.
    let spine = pieces.len() <= 3 && pieces.iter().any(|p| !p.lanes.is_empty());
    if shape(&one) <= MAX_ASPECT || width(&all) < MIN_WRAP_WIDTH || spine {
        return one;
    }
    // The pieces are filled into rows in order, a row ending where the next piece would pass a
    // width: a piece's row depends on the pieces before it and on that width, which is the
    // width of a sheet of the page's shape for the area the pieces cover, in coarse steps, so
    // that one more small part does not re-flow the sheet.
    let area: f64 = pieces
        .iter()
        .map(|p| (slot_width(p) + BLOCK_GAP) * (p.rect.height() + ROW_MARGIN))
        .sum();
    let widest = pieces.iter().map(slot_width).fold(0.0, f64::max);
    let centre = snap_up((area * TARGET_ASPECT).sqrt(), ROW_QUANTUM);
    let fill = |wanted: f64| -> Vec<Vec<usize>> {
        let mut rows: Vec<Vec<usize>> = vec![Vec::new()];
        for &i in &all {
            let row = rows.last().expect("a row");
            let mut with = row.clone();
            with.push(i);
            // A source and its load are never cut apart.
            let held = row.last().is_some_and(|&last| pieces[last].joined);
            if !row.is_empty() && width(&with) > wanted && !held {
                rows.push(vec![i]);
            } else {
                rows.last_mut().expect("a row").push(i);
            }
        }
        rows
    };
    // A few widths a step apart round that one, the one that makes the sheet nearest the page's
    // shape (the least width of equals): the same few for a small edit, and rows that differ
    // only where the pieces are cut.
    let mut best: Option<(f64, Vec<Vec<usize>>)> = None;
    for k in -2..=2 {
        let wanted = (centre + f64::from(k) * ROW_QUANTUM / 2.0).max(widest);
        let rows = fill(wanted);
        let miss = (shape(&rows) / TARGET_ASPECT).ln().abs();
        if best.as_ref().is_none_or(|(m, _)| miss < *m - 1e-9) {
            best = Some((miss, rows));
        }
    }
    best.expect("a candidate").1
}
