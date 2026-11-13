The placed boards (`*.board.json`, a `Board` as JSON) that this crate's unit tests start from, so
that they need no placer (issue #608). The placer's `pcb_fixtures` test makes them from its own
test designs and fails when they are stale; write them again with

    NETSMITH_UPDATE_FIXTURES=1 cargo test -p netsmith-placer pcb_fixtures

and review the diff.
