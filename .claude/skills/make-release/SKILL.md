---
name: make-release
description: Build all 4 release artifacts via `make release` and publish them to GitHub as a new release with a title and notes. Use when the user invokes /make-release or asks to cut/publish a Guess-Up release.
---

# make-release

Cut a Guess-Up release: build the 4 cross-platform artifacts with `make release`, then publish them to GitHub with `gh release create`.

## Preconditions

1. Working tree is clean and the release-completing PR has been merged to `main`.
2. `crates/client/Cargo.toml` and `crates/relay/Cargo.toml` both carry the target version (they should match — enforced by `CLAUDE.md`).
3. `gh` is authenticated for the repo.

If any of these are off, stop and report — do not try to "fix" version numbers or commit on the user's behalf.

## Flow

### Step 1 — Detect version and tag

Read the version from `crates/client/Cargo.toml`:

```bash
awk -F'"' '/^version/{print $2; exit}' crates/client/Cargo.toml
```

The tag is `v<MAJOR>.<MINOR>` — drop the patch (e.g. `1.2.0` → `v1.2`). This matches the existing release history (`v1.0`, `v1.1`, `v1.2`). Confirm the detected tag with the user if it's ambiguous.

Also verify the relay crate version matches:

```bash
awk -F'"' '/^version/{print $2; exit}' crates/relay/Cargo.toml
```

If they diverge, stop and flag it — `CLAUDE.md` requires all workspace crates on the same version.

### Step 2 — Collect title and notes

If the user has not already provided a title and notes in the current conversation, ask for them. Style guide from prior releases:

- **Title** is a short thematic phrase (e.g. "Connection Improvements", "Rounds, Warnings, and Custom Lists") — not just the version number.
- **Notes** are a tight markdown bullet list. One line per user-visible change. Code spans (`` ` ``) for file names, formats, flags. No sub-bullets, no paragraphs.

Reference existing releases for tone:

```bash
gh release view v1.1
gh release view v1.2
```

### Step 3 — Build artifacts

Run the full release build:

```bash
make release
```

This takes a few minutes (cross-compiles for Linux and Windows). Allow ≥10 minutes timeout. The build produces 4 files in `dist/`:

- `guess_up-<version>-linux-x86_64.tar.gz`
- `guess_up-<version>-windows-x86_64.zip`
- `relay-<version>-linux-x86_64.tar.gz`
- `relay-<version>-windows-x86_64.zip`

Verify all 4 exist before continuing:

```bash
ls -lh dist/*.tar.gz dist/*.zip
```

If any are missing, stop and report the `make release` error rather than publishing a partial release.

### Step 4 — Publish

Create the release with all 4 artifacts attached. Use a heredoc for the notes so multi-line markdown is preserved:

```bash
gh release create <TAG> \
  --title "<TITLE>" \
  --notes "$(cat <<'EOF'
<NOTES>
EOF
)" \
  dist/guess_up-<VERSION>-linux-x86_64.tar.gz \
  dist/guess_up-<VERSION>-windows-x86_64.zip \
  dist/relay-<VERSION>-linux-x86_64.tar.gz \
  dist/relay-<VERSION>-windows-x86_64.zip
```

Report the release URL that `gh` prints.

## Rules

- **Never** push tags, bump versions, or commit on the user's behalf as part of this flow. Versioning is handled on the feature branch that closes the release (per `CLAUDE.md`) — the skill assumes it's already done.
- **Never** publish if any of the 4 artifacts failed to build. Partial releases confuse users who grab a platform that's missing.
- **Never** skip the `make release` step and try to reuse an older `dist/` — always rebuild so the artifacts match the current workspace version.
- If the user explicitly asks for a prerelease, pass `--prerelease` to `gh release create`. Otherwise ship as a normal release.
