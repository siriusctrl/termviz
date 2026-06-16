# Releasing

Use this checklist for every public `termviz` release. The product boundary for
the release remains a local terminal viewer: do not add or describe daemons,
browser dashboards, notebook runtimes, or background services as release
requirements.

## Release Rules

- Use semantic versioning.
- Keep version numbers aligned across all package manifests.
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
5. Confirm README examples and Current State/TODO sections match the release.
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
render/export flag.

For image rendering changes, verify at least one protocol backend and the ANSI
block fallback. For plot parsing or rendering changes, verify deterministic
CLI output for CSV/TSV/JSONL fixtures.

## CI

Before publishing:

1. Push the release branch.
2. Confirm GitHub Actions pass for formatting, tests, Clippy, and any release
   artifact jobs that exist.
3. For tagged releases or manual dispatches, confirm the `release-artifact` job
   uploads `termviz-linux-x86_64.tar.gz` and
   `termviz-linux-x86_64.tar.gz.sha256`.
4. If CI is missing or incomplete for the release, record the local verification
   commands and results in the release PR or release notes.

## Release Artifacts

The 0.1.0 artifact scope is a Linux x86_64 tarball from CI plus a SHA-256
checksum. Attach both files to the GitHub Release if the workflow completed.

Do not describe npm prebuilt binaries for 0.1.0. npm distribution is deferred
until package scaffolding, binary installation behavior, and CI publishing are
implemented.

## crates.io

Run a dry-run before publishing:

```sh
cargo publish --dry-run --locked
```

Inspect the packaged contents before publishing if the dry-run output is
surprising:

```sh
cargo package --list --locked
```

Publish only after the changelog entry, local checks, and CI are complete:

```sh
cargo publish --locked
```

After publishing, verify the crate page and install path:

```sh
cargo install termviz --version X.Y.Z
termviz --version
```

## GitHub Release

1. Tag the exact commit that was published:

   ```sh
   git tag -a vX.Y.Z -m "termviz X.Y.Z"
   git push origin vX.Y.Z
   ```

2. Create a GitHub Release for `vX.Y.Z`.
3. Use the matching `CHANGELOG.md` section as the release notes.
4. Attach `termviz-linux-x86_64.tar.gz` and its `.sha256` file if the release
   artifact workflow produced them.
5. Do not publish a GitHub Release with placeholder text such as "TBD",
   "initial release", or copied internal implementation phases.

## Post-Release

1. Confirm the GitHub Release, crates.io page, and install command point to the
   same version.
2. Confirm `CHANGELOG.md` still has an `Unreleased` section ready for future
   user-facing changes.
3. If anything failed after publishing, document the follow-up in the next
   changelog entry and decide whether a patch release is needed.
