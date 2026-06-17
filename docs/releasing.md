# Releasing

Use this checklist for every public `termviz` release. The product boundary for
the release remains a local terminal viewer: do not add or describe daemons,
browser dashboards, notebook runtimes, or background services as release
requirements.

## Release Rules

- Use semantic versioning.
- Keep the `Cargo.toml` version aligned with the release tag.
- Add a `CHANGELOG.md` entry before tagging or publishing.
- Never publish with placeholder release notes.
- Use Conventional Commits with explanatory bodies.
- Keep large-file tradeoffs explicit in README, CHANGELOG, or architecture docs.
- Keep redirected stdout scriptable. Terminal control sequences must require
  interactive TTY use or an explicit render/export request.

## Preflight

1. Confirm the release scope and version.
2. Update package metadata and lockfile if the version changed.
3. Update `CHANGELOG.md` with `## [X.Y.Z] - YYYY-MM-DD`.
4. Ensure the changelog entry is user-facing and suitable for GitHub Release
   notes. Remove phase labels, internal TODO language, and placeholders.
5. Confirm README examples and documented user-visible limitations match the
   release.
6. Confirm architecture or release docs describe any new boundary, export,
   terminal protocol, packaging, or large-file tradeoff.

## Local Verification

Run the standard checks:

```sh
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

For terminal UI changes, run the built binary under a real PTY and verify draw,
resize, scroll or pan, and quit behavior. Example:

```sh
script -q /tmp/termviz-pty.log -c 'target/release/termviz examples/sample.png'
```

For protocol changes, verify redirected stdout does not receive terminal control
sequences unless the user explicitly requested them with `--output-format`,
`--protocol`, `--output` to a supported export extension, or another documented
render/export flag. Redirected output without `--output-format` should remain a
scriptable PNG export, not terminal protocol output.

For image rendering changes, verify at least one protocol backend and the ANSI
block fallback. For plot parsing or rendering changes, verify deterministic
CLI output for CSV/TSV/JSONL fixtures.

For Kitty or pixel-protocol visual changes, include a real emulator recording
from `scripts/record-emulator-demo.sh`. For plot-rendering changes that can
affect geometry, color, grouping, or viewport behavior, use
`scripts/record-emulator-fixtures.sh` and inspect the contact sheets.

## CI

Before publishing:

1. Push the release branch.
2. Confirm GitHub Actions pass for formatting, tests, Clippy, and any release
   artifact jobs that exist.
3. For tagged releases or manual dispatches, confirm the release workflow builds
   `termviz-linux-x64.tar.gz` and `sha256sums.txt`.
4. If CI is missing or incomplete for the release, record the local verification
   commands and results in the release PR or release notes.

## Release Artifacts

Release artifacts are a Linux x64 static binary tarball plus SHA-256 checksum.
The release workflow verifies that the binary has no glibc runtime interpreter
before packaging, so Linux x64 users can download a self-contained CLI from the
GitHub Release page.

## Install From Git

The supported package-manager-free install path is Cargo from GitHub:

```sh
cargo install --git https://github.com/siriusctrl/termviz --tag vX.Y.Z
termviz --version
```

For local validation before tagging, run `scripts/release-verify.sh`. The script
also runs `cargo package --locked --allow-dirty` as a non-publish packaging
safety check.

## GitHub Release

1. Tag the exact commit that was published:

   ```sh
   git tag -a vX.Y.Z -m "termviz X.Y.Z"
   git push origin vX.Y.Z
   ```

2. Create a GitHub Release for `vX.Y.Z`.
3. Use the matching `CHANGELOG.md` section as the release notes.
4. Attach `termviz-linux-x64.tar.gz` and `sha256sums.txt` if the release
   workflow produced them.
5. Do not publish a GitHub Release with placeholder text such as "TBD",
   "initial release", or copied internal implementation phases.

## Post-Release

1. Confirm the GitHub Release and `cargo install --git --tag vX.Y.Z` install
   command point to the same version.
2. Confirm `CHANGELOG.md` still has an `Unreleased` section ready for future
   user-facing changes.
3. If anything failed after publishing, document the follow-up in the next
   changelog entry and decide whether a patch release is needed.
