//! The only time limit a test of the router passes: a hang guard, not a speed. A routing that is
//! not done in it has hung, however slow the machine (which only makes the routing longer, never
//! ten times longer than the 2 minutes a board of these takes on a runner to itself): so a test
//! never fails by the speed of the machine, and a hang ends in a message that says so. A test
//! that wants less of the router's work asks for less of it (`RouteOptions::retry_orders`,
//! `rip_up`, `negotiate`, `most_tries`) and not for a shorter time.

use std::time::Duration;

use netsmith_router::Routing;

/// The `time_limit` of a routing in a test.
pub const GUARD: Duration = Duration::from_secs(600);

/// Fails, saying the router hung, when the routing stopped at [`GUARD`].
#[track_caller]
pub fn not_hung(routing: &Routing) {
    assert!(
        !routing.stopped,
        "the router was still working at its hang guard of {GUARD:?}: it hung"
    );
}
