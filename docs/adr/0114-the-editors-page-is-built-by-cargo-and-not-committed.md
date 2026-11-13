---
status: accepted
---

# The editor's page is built by cargo and is not committed

Meets tests 1 (the page, the build script and the release rules depend on it) and 3 (committing the built page looks like the obvious way to keep Node out of a build, and was proposed again as a bot commit). The owner decided it on 2026-11-13 (#616). It replaces the paragraph "The page is a committed bundle" of ADR 0019; the rest of ADR 0019 stands.

**Decision.** The built page (Preact and Vite, from `crates/netsmith-editor/web`) is not in the repository: `crates/netsmith-editor/web-dist` is in `.gitignore`. `crates/netsmith-editor/build.rs` builds the page with Node 20 or newer (`npm ci`, `npm run build`) whenever Node is on `PATH` or in `~/.nvm`, and embeds it in the binary. With no Node, or when the build fails, the binary embeds a small placeholder page instead: it says the editor's page was not built and to use a release binary, or install Node and build again. `cargo build` still succeeds and prints a cargo warning with the reason. `netsmith editor` still starts and warns, and `/` answers the placeholder with a 503.

**Releases and CI always have Node.** The release workflow installs it, and `scripts/release-build.sh` fails on the warning (and on a placeholder at its smoke test) rather than ship it. The `build` job of CI installs it, so the test archive and the `netsmith` binary the other jobs run (the recipes among them) carry the real page, and the page tests fail on a placeholder when `NETSMITH_REQUIRE_BUNDLE_CHECK` is set, as `test` sets it. Without that variable the page tests skip with a message, so a Rust-only contributor needs no Node.

**Why.** Two pull requests that changed the page both changed `web-dist/SOURCE-HASH` and `index.html`, and every one conflicted with the other (#616). A pull request now carries sources only. A bot that commits the bundle to main needs a ruleset bypass and races with merges; it was drafted and dropped.

**Rejected.** A bot commit of the bundle on main: a bypass of the branch rules, and a window in which main holds an older page. Keeping the committed bundle as a fallback: the conflict stays for as long as any pull request may update it.

**The cost.** A build from a checkout or `cargo install` with no Node gets no editor, only the placeholder, so the editor needs Node at build time or a release binary. The first build with Node needs the npm registry once.

**Left open.** Publishing the crate to crates.io would need the built page in the package, or an install without the editor.
