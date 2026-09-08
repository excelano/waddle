# Releasing waddle

The release loop lives in `~/notes/releasing.md` — the ordered steps, the apt
step, crates.io, the winget submission, the spent-tag rule, and the standing
facts about tokens and secrets. Failure recipes are in
`~/notes/build_release_gotchas.md`. This file carries what is true of waddle and
not of its siblings.

| | |
|---|---|
| Loop | cargo-dist |
| Version lives in | `workspace.package.version` in the root `Cargo.toml`, once, for both crates |
| `apt-ship` argument | `waddle` |
| crates | `waddle-core`, then `waddle` |
| winget package | `Excelano.waddle` |
| Windows asset | `waddle-x86_64-pc-windows-msvc.zip` |

**Two crates, one version, one tag.** The workspace versions both crates
together and `publish-crate.yml` publishes `waddle-core` before `waddle`.
Bumping one without the other is not a state this repository has.

**The command, the Homebrew formula, and the apt package are all `waddle`.**
cargo-dist's tarballs and installer are named after it: `waddle-installer.sh`,
`waddle-<target>.tar.xz`. The `.deb` is built from the `waddle` crate alone
(`cargo deb -p waddle`); the library reaches people through crates.io.

**A docling-core release can be the reason for a release here.** When
`docling-latest` in `ci.yml` goes red, the fix is a change to a writer and
the bump of the pin in the root `Cargo.toml` goes in the same commit. Duckling
takes the new version by bumping `waddle-core`.
