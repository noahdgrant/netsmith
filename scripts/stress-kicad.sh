#!/usr/bin/env bash
# Runs the fab tests that start Windows KiCad many times, several copies at once, to find
# a flaky launch. Usage: scripts/stress-kicad.sh [runs] [processes] [test filter...]
#
# Defaults: 50 runs split over 3 processes, of the three tests that agree with KiCad's own
# plot or check. Every test thread of a copy and every copy take turns at KiCad through
# `netsmith_fab::exclusive`, so 0 failures is the expected result; a failure prints the
# test's full output (which has KiCad's stdout, stderr, exit status and earlier attempts).
set -u
runs=${1:-50}
procs=${2:-3}
shift 2 2>/dev/null || shift $# 
filters=("$@")
if [ ${#filters[@]} -eq 0 ]; then
    filters=(
        the_outline_and_pours_example_passes_kicads_check_with_its_layout_file
        the_four_layer_example_agrees_with_kicads_own_plot
        the_two_board_example_agrees_with_kicads_own_plot
    )
fi
cd "$(dirname "$0")/.." || exit 2
bin=$(cargo test -p netsmith --test fab --no-run 2>&1 | sed -n 's/^ *Executable tests\/fab.rs (\(.*\))$/\1/p')
[ -x "$bin" ] || { echo "cannot find the fab test binary" >&2; exit 2; }
out=$(mktemp -d /tmp/stress-kicad.XXXXXX)
cd crates/netsmith || exit 2
per=$(( (runs + procs - 1) / procs ))
for p in $(seq 1 "$procs"); do
    (
        for r in $(seq 1 "$per"); do
            n=$(( (p - 1) * per + r ))
            [ "$n" -gt "$runs" ] && break
            if ! "$bin" --test-threads 4 "${filters[@]}" > "$out/run-$n.log" 2>&1; then
                echo "run $n FAILED, log: $out/run-$n.log"
            fi
        done
    ) &
done
wait
failed=$(grep -l "^test result: FAILED\|panicked" "$out"/run-*.log 2>/dev/null | wc -l)
echo "$runs runs of ${#filters[@]} tests in $procs processes: $failed failed (logs in $out)"
[ "$failed" -eq 0 ]
