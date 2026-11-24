# Contributing

Thanks for helping. English or Chinese are both fine in issues and pull requests.

## Before you start

- For a bug, open an [issue](../../issues/new/choose) with the steps to reproduce it.
- For a new feature, open an issue first so we can agree it fits. sightline only **observes and
  measures**: it never clicks, types or moves the mouse, and it does no OCR or image analysis.
- For a security problem, see [SECURITY.md](SECURITY.md). Do not open a public issue.

## Build and check

You need Windows 10/11 and a recent stable Rust (`rust-toolchain.toml` selects it).

```powershell
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

CI runs the same four commands on Windows. Please run them before you push.

## Pull requests

- Keep a change focused. Add a unit test for new pure logic, and a test that fails without the fix
  for a bug fix.
- If behaviour or flags change, update `README.md`, the translations (`README.zh-CN.md`,
  `README.zh-TW.md`), `docs/REFERENCE*.md`, `AGENTS.md` and `skills/sightline/SKILL.md`.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/):
  `feat: ...`, `fix: ...`, `docs: ...`, `ci: ...`, `chore: ...`.

## Changelog and releases

The version and `CHANGELOG.md` come from **changesets only**. Commit messages are never read for
them (`knope.toml` sets `ignore_conventional_commits`), so keep writing them in Conventional
Commits style for readability, but they do not change the version or the changelog.

- Every user-visible change needs a changeset: a Markdown file in `.changeset/`. Run
  `knope document-change`, or write it by hand (`.changeset/short_name.md`):

  ```markdown
  ---
  default: minor
  ---

  #### Short title of the change

  - What changed, in one or two lines a user can act on.
  ```

  Use `patch` for a fix, `minor` for a feature, `major` for a breaking change.
- CI-only, docs-only and other changes users never see need no changeset.
- Do not edit `CHANGELOG.md` or the version in `Cargo.toml` by hand.

Releases go through a pull request, because `main` does not accept direct pushes:

1. Merge your change with its changeset into `main`.
2. The `Prepare release` workflow runs on every push to `main` (or run it by hand from the
   Actions tab). If changesets are pending, it bumps the version and `CHANGELOG.md` on the
   `release` branch and opens or updates the PR `chore: release vX.Y.Z`. With none pending it does
   nothing.
3. Review the release PR and merge it (squash). It deletes the changesets, so the push that merges
   it finds nothing pending and opens no new PR.
4. The `Release` workflow runs on that merge: it builds `sight.exe`, tags `vX.Y.Z` and publishes
   the GitHub release with the assets.

Checks do not run automatically on a PR opened by `GITHUB_TOKEN`; if a check becomes required,
add a `RELEASE_TOKEN` secret (a PAT or GitHub App token) so the release PR triggers CI.

## License

By contributing you agree that your work is licensed under [AGPL-3.0](LICENSE).
