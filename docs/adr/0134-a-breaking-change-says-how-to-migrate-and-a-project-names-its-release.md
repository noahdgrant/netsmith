---
status: proposed
---

# A breaking change says how to migrate, and a project names the release it is written for

Issue #740. ADR 0001 keeps one spelling for each thing, with no grace period, so a release that changes a spelling makes the old one an error at once. Between 0.1.0 and the next release nine changes did that, and a project of about 100 parts that built on 0.1.0 had 139 errors five days later. Each error named the new spelling, but nothing said why there were so many, or what each change was, short of reading ADRs 0115 to 0129. The owner was away; this was decided by the implementing agent with the smallest mechanism that answers the issue, and waits for the owner.

**Decision.**

- `docs/MIGRATING.md` lists, release by release, every change that makes source (or a file beside it, or a command line) that built on the release before an error: the error it gives, the old spelling and the new one side by side, and the ADR. A pull request that makes such a change has `!` in its title (Conventional Commits) and adds its entry under _Unreleased_ in the same pull request. The `PR title` workflow fails a `!` title whose pull request does not change `docs/MIGRATING.md`. Releasing renames _Unreleased_ to the release (`docs/RELEASING.md`). A change that only changes a result (a pick, a via, a new check) is listed under _What changed with no error_.
- `netsmith.toml` names the release a project is written for, `netsmith = "0.2.0"` in `[project]`. `netsmith init` writes the running release. The key is optional; a manifest without it was made before the key, with 0.1.0 or earlier. A value that is not `major.minor.patch`, or `major.minor` for any patch of it, is refused when the manifest is read.
- Every command that builds or checks a project warns, once in a run, when the project names another minor release (the owner, issue #1657): an older one points to `docs/MIGRATING.md` and says to set the key when the project builds, a newer one says to install that netsmith, and a patch release is silent. A project that names none gets no warning; when its `build`, `check` or `fab` stops because a board failed its check, the run ends with a note that it was made before the key. The release never refuses a build.

**Rejected.**

- *A `netsmith migrate` that rewrites the mechanical spellings.* Each breaking change would need a rewriter kept for as long as projects of that release exist, tested against sources nobody has; the errors already name every place and the new text. It may come later for a change that is too large to make by hand.
- *Keeping the old spelling for one release with a warning.* It is the second spelling ADR 0001 refuses, for a release, in every stage that reads it.
- *Editions, where a project of an older release keeps building with the old language.* The checker would carry two languages.
- *Refusing a project of another release.* Most releases change nothing a given project uses.
- *Nothing, because the project is before 1.0.* The issue is what that cost.

**Cost.** Every breaking pull request writes an entry, and its author must see that a change breaks source (a change that only adds is not one). The key goes stale in a project that moves without setting it; every run then warns until it is set. A project made before the key gets the note on any failed check, a typo too, until it adds the key.

**Left open.** Whether `init --refresh` should offer to set the key (it touches no source today, ADR 0131); a rewriter for a large change.
