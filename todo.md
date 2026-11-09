# mrs — TODO

## Command palette: next commands

### Batch 1 — expose existing actions (trivial)

- [x] New File (`Ctrl+N`)
- [x] Save As… (`Ctrl+Shift+S`)
- [x] Find… (`Ctrl+F`)
- [x] Find Next / Previous (`F3` / `Shift+F3`)
- [x] Go to Line… (`Ctrl+L`)
- [x] Zoom In / Out / Reset (`Ctrl++` / `Ctrl+-` / `Ctrl+0`)
- [x] Copy File Path (`Ctrl+Shift+C`)
- [x] Delete Line (`Ctrl+Shift+K`)
- [x] Keyboard Shortcuts (`F1`)

### Batch 2 — small new features

- [ ] Duplicate Line — reuse the event-injection approach from Delete Line so undo works.
- [ ] Move Line Up / Down — common editor behavior; needs caret handling.
- [ ] Trim Trailing Whitespace — document or selection.
- [ ] Toggle Line Endings (LF/CRLF) — `Eol` is already tracked.
- [ ] Toggle Read-Only — flag passed to `.interactive()`; show it in the status bar.
- [ ] Toggle Fullscreen — one `ViewportCommand::Fullscreen` call.
- [ ] Selection → Uppercase / Lowercase — needs undo integration.
- [ ] Reload from Disk — ask for confirmation when modified.

### Deferred / probably skip

- [ ] Word wrap — conflicts with the current layout and scroll setup; more work than it looks.
- [ ] Open Recent — needs persistent config storage; better as its own feature.
- [ ] Toggle comment — language-dependent, awkward in a plain-text editor.

### Suggested order

See the sequential execution plan at the end of this file.

## GitHub repository and CI/CD

- [ ] Make sure the repository is ready to publish: commit all files, confirm `target/` is ignored, add a `LICENSE`, and pick a branch name (`main`).
- [ ] Create the GitHub repository (e.g. `gh repo create mrs --source=. --public --push`) and push the initial commit.
- [ ] Add `.github/workflows/ci.yml` running on pushes and pull requests:
  - `cargo fmt --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test`
  - `cargo build --release`
  - Cache Cargo builds (`Swatinem/rust-cache`) and use a stable Rust toolchain.
- [ ] Add `shellcheck install.sh` to CI (or a separate workflow).
- [ ] Add a release workflow triggered by version tags (`v*`):
  - Build release binaries (at least Linux; optionally macOS/Windows).
  - Package the binary, desktop entry, icons, `README.md`, and `LICENSE` as a tarball.
  - Generate checksums (`sha256sum`).
  - Publish the artifacts with `softprops/action-gh-release`.
- [ ] Add Dependabot for Cargo and GitHub Actions updates.
- [ ] Optionally protect `main` with required CI checks and add a CI status badge to `README.md`.
- [ ] Optionally add `cargo audit` and `cargo deny` for dependency/license checks.

## Execution plan (sequential)

One agent at a time. Start a batch only after the previous batch has been
reviewed and committed.

### Batching rule of thumb

- Mechanical / wiring existing actions: **5–8 tasks** per agent.
- Small features (state + UI + undo): **2–3 tasks** per agent.
- Medium features: **1–2 tasks** per agent.
- Config / CI / docs: **3–5 tasks** per agent.
- Batch by cohesion, not by count: one concern, one test story, roughly
  150–400 lines of diff.
- Checkpoint after every batch: `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test`, a manual smoke
  test of the new commands, then commit.

### Batches in order

1. **Repository bootstrap (1 task):** add `LICENSE`, pick `main`, make the
   initial commit, create the GitHub repository, and push.
2. **CI batch (3 tasks):** add `.github/workflows/ci.yml`, `shellcheck` for
   `install.sh`, and Dependabot. Push and confirm CI is green.
3. **Palette batch 1a (4 tasks):** New File, Save As…, Find…, Go to Line….
4. **Palette batch 1b (5 tasks):** Find Next/Previous, Zoom In/Out/Reset,
   Copy File Path, Delete Line, Keyboard Shortcuts.
5. **Palette batch 2a (3 tasks):** Duplicate Line, Move Line Up/Down,
   Trim Trailing Whitespace.
6. **Palette batch 2b (3 tasks):** Toggle Line Endings, Toggle Read-Only,
   Toggle Fullscreen.
7. **Palette batch 2c (2 tasks):** Selection → Uppercase/Lowercase,
   Reload from Disk.
8. **Release batch (3–4 tasks):** tag-triggered release workflow, packaging
   and checksums, CI badge/branch protection, optional `cargo audit` and
   `cargo deny`.

Each batch is one agent run and one commit.
