# Releasing

Release automation is not implemented yet. Until it exists, use this file as
the policy stub and update it before the first public release.

## Policy

- Use semantic versioning.
- Keep version numbers aligned across all package manifests.
- Add a `CHANGELOG.md` entry before tagging.
- Use Conventional Commits with explanatory bodies.
- Run `cargo fmt`, `cargo test`, and `cargo clippy --all-targets -- -D warnings`.
- Build the release binary before tagging.
- For terminal rendering changes, verify the built binary in a real PTY.
- For protocol output changes, verify redirected stdout stays scriptable.

## TODO

- [ ] Add GitHub Actions for checks.
- [ ] Add release artifact builds.
- [ ] Add crates.io release steps.
- [ ] Decide whether npm prebuilt binaries are in scope.
- [ ] Document post-release registry verification.
