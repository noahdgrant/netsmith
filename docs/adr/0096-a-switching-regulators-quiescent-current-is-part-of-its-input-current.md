# A switching regulator's quiescent current is part of its input current
status: folded
Now in `docs/LANGUAGE.md` ("Switching regulators", "What a part is at DC") and the doc comment of `Lt8641` (`std/parts/regulator.nsm`). Known limitation, not built (issue #63): an unloaded std stage is refused by the check of ADR 0076, amendment 15, so a sleep budget cannot be shown to pass on one. Full text: `git show 2a2a5b2:docs/adr/0096-a-switching-regulators-quiescent-current-is-part-of-its-input-current.md`.
