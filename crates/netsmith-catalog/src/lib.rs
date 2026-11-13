//! The catalog and supply side of a build: the pick files, the generic parts the solver left to
//! the catalogs, the supply providers (prices and stock), the catalog refresh and the findings
//! about the values a pick left open.
//!
//! Not here: the pick lock (`pick_lock`) and the provider trials (`providers`), which run the
//! pipeline driver in `netsmith`, and which catalog files a command acts on (it reads the
//! project's manifest).

pub mod picks;
pub mod refresh;
pub mod resolve;
pub mod stock;
pub mod supply;
pub mod values;
