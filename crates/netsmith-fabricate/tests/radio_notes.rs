//! The conditions of a radio's grant that a person must verify (issue #1278) are read from the
//! model `fab` writes its notes from: the built board's, with the board's own paths.

use netsmith_check::Loader;
use netsmith_pipeline::outcome_with;
use netsmith_solve::Catalogs;

const SOURCE: &str = "import std::radio::{Antenna, AntennaBand, AntennaType, Radio, RadioBand}
import std::sources::Rail

/// A chip antenna of 1 dBi in 2.4 GHz. Illustrative.
part ChipAntenna {
    .designator &= \"AE\"
    .antenna: Antenna {
        .type &= AntennaType::Chip
        .low: AntennaBand { .band &= 2.4GHz..2.5GHz; .gain &= 1 }
    }
    pins {
        .antenna.feed passive \"FEED\" \"1\"
        .gnd          passive \"GND\"  \"2\"
    }
}

/// A radio module whose grant allows 3 dBi in 2.4 GHz. Illustrative; not a real part.
part Module {
    .designator &= \"U\"
    .vdd: Rail
    .radio: Radio {
        .grant &= \"FCC ID 2ABCD-EXAMPLE\"
        .antenna_type &= AntennaType::Dipole
        .exposure_distance &= 200mm
        .low: RadioBand { .band &= 2.4GHz..2.4835GHz; .max_antenna_gain &= 3 }
    }
    pins {
        .vdd.pos  power   \"VDD\" \"1\"
        .vdd.neg  power   \"GND\" \"2\"
        .radio.rf passive \"RF\"  \"3\"
    }
}

board Tracker {
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= \"VDD\" }
    .u1: Module
    .ae1: ChipAntenna
    .supply ~ .u1.vdd
    .u1.radio.rf ~ .ae1.antenna.feed
    .ae1.gnd ~ .u1.vdd.neg
}
";

#[test]
fn the_fab_notes_ask_a_person_to_verify_the_grants_conditions() {
    let built = outcome_with("tracker.nsm", SOURCE, &Loader::std(), &Catalogs::jlcpcb(), None)
        .unwrap_or_else(|e| panic!("{e:?}"))
        .boards
        .remove(0);
    assert_eq!(
        netsmith_solve::radio::verify_lines(&built.trace.model),
        [
            "verify: radio `u1.radio` (FCC ID 2ABCD-EXAMPLE): a person must verify that the antenna is of the type the grant was tested with, Dipole (`ae1.antenna` is Chip)",
            "verify: radio `u1.radio` (FCC ID 2ABCD-EXAMPLE): a person must verify that the product keeps a person at least 200mm from the antenna (the grant's exposure condition)",
        ]
    );
}
