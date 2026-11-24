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

User-facing changes need a changeset. Run `knope document-change` and describe the change in one
or two lines; pick `patch` for a fix, `minor` for a feature. Do not edit `CHANGELOG.md` or the
version in `Cargo.toml` by hand.

Releases go through a pull request, because `main` does not accept direct pushes:

1. Merge your change with its changeset into `main`.
2. The `Prepare release` workflow runs automatically when `.changeset/` changes on `main` (or run
   it by hand from the Actions tab). It bumps the version and `CHANGELOG.md` on the `release`
   branch and opens or updates the PR `chore: release vX.Y.Z`.
3. Review the release PR and merge it (squash).
4. The `Release` workflow runs on that merge: it builds `sight.exe`, tags `vX.Y.Z` and publishes
   the GitHub release with the assets.

Checks do not run automatically on a PR opened by `GITHUB_TOKEN`; if a check becomes required,
add a `RELEASE_TOKEN` secret (a PAT or GitHub App token) so the release PR triggers CI.

## License

By contributing you agree that your work is licensed under [AGPL-3.0](LICENSE).
