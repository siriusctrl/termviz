# AGENTS.md

Principles for agents contributing to this repository.

## Core Principles

1. **Viewer-first, not server-first**
   - `termviz` is a local terminal viewer.
   - Do not add background daemons, browser dashboards, plugin systems, or
     notebook runtimes unless the product boundary is deliberately changed.

2. **Large visual inputs are a product requirement**
   - Do not decode or render whole large assets just to draw the first screen.
   - Prefer metadata-first loading, tile readback, bounded plot windows,
     temporary files, and explicit preloading.
   - If a feature requires whole-file parsing or whole-image decoding, make
     that tradeoff explicit in the CLI help or docs.

3. **Stdout stays scriptable**
   - Keep terminal escape sequences behind TTY detection or explicit flags.
   - Redirected stdout should produce metadata, export output, or clear errors,
     not accidental inline-image control codes.

4. **Conventional Commits with real bodies**
   - Use Conventional Commits for every commit.
   - Include a body that explains what changed and why.

5. **Release notes are part of the release**
   - Maintain `CHANGELOG.md` for user-facing changes.
   - Every release version must have a `CHANGELOG.md` entry before tagging.
   - Do not publish a release with placeholder notes.

## Navigation

Use README for user-facing behavior. Use docs for maintainer workflows and
durable project decisions.

Keep this file coarse-grained. Do not mirror every implementation detail here.
Use `docs/INDEX.md` as the navigation entry point when you need code layout or
workflow-specific docs.

### Read these docs first

- `README.md`
- `docs/INDEX.md`
- `CHANGELOG.md`

### Read these docs when the task matches

- Architecture, module boundaries, terminal protocols, plot model, or asset
  loading:
  - Read `docs/architecture.md`
- Release, packaging, crates.io, npm, GitHub Releases, or version tags:
  - Read `docs/releasing.md` once it exists.
- TUI rendering, block fallback visuals, screenshots, recordings, or product
  effect demos:
  - Read `docs/visual-verification.md`.
- Test organization, protocol coverage, selector behavior, or PTY smoke:
  - Read `docs/testing.md`.

## Engineering Rules

- Keep stdout output valid and scriptable.
- Keep interactive behavior behind TTY detection.
- Keep image decoding separate from terminal rendering.
- Keep plot data parsing separate from plot scene/raster rendering.
- Add terminal protocols as backend implementations, not as product branches.
- Update `README.md` when CLI flags, install steps, or user-visible behavior
  changes.
- Update `CHANGELOG.md` when user-facing behavior, packaging, or release process
  changes.
- Update docs when architecture, release, packaging, or artifact policy changes.
- Prefer Linux-first behavior, but avoid unnecessary non-portable code when
  portable Rust is simple.

## Verification Requirements

- Run `cargo fmt`.
- Run `cargo test`.
- Run `cargo clippy --all-targets -- -D warnings` when Clippy is available.
- After implementing a user-facing command or viewer behavior, run the exact
  command path, or the closest faithful fixture command, yourself before
  reporting completion.
- For TUI changes, run the built CLI under a real PTY, for example with
  `script`, and verify draw/resize/scroll/quit behavior.
- For visual TUI or block-rendering changes, create PTY recording artifacts with
  `scripts/record-pty-demo.sh`, inspect `keyframes/` or `contact-sheet.png`
  yourself, and keep the recording path in the handoff or final summary.
- For terminal protocol changes, verify the protocol output does not appear on
  redirected stdout unless explicitly requested.
- For image rendering, run fixtures through at least one protocol backend and
  the block fallback.
- For protocol renderer or viewer changes, cover every explicit protocol
  (`blocks`, `kitty`, `sixel`, and `iterm`) at the appropriate test layer.
  Keep `auto` tests focused on selector behavior rather than renderer output.
- For plot parsing or rendering, add deterministic CLI tests and fixture data.
- For large-file behavior, add or update benchmark scripts before claiming the
  implementation is bounded.
