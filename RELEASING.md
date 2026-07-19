# Releasing

How to cut a release of OpenTimsTDF (the `opentimstdf` crate on crates.io and
the `opentimstdf` package on PyPI, both built from this one repo and tag).

## 1. Confirm CI and audit are green

Before doing anything else, confirm the commit you're about to release is
actually clean:

```
scripts/check-release-ready.sh
```

This checks the most recent `ci.yml` and `audit.yml` runs for `HEAD` (pass a
ref/SHA to check something other than `HEAD`) and exits non-zero if either
hasn't run, is still in progress, or failed. `publish.yml` triggers on any
`v*` tag push with no dependency on CI or audit passing - GitHub Actions has
no way for one workflow file to `needs:` a job in another - so this check is
the only thing standing between a red commit and a live publish. Don't tag
until it passes.

## 2. Bump the version

The crate version lives in one place, `[workspace.package].version` in the
root `Cargo.toml` (both `crates/opentimstdf` and `crates/opentimstdf-py` use
`version.workspace = true`). The PyPI package version is derived from the
same Cargo workspace version by maturin (`pyproject.toml` declares
`dynamic = ["version"]`), so there's nothing to bump there separately.

1. Edit `version` in `Cargo.toml`.
2. Run `cargo build` (or `cargo check`) so `Cargo.lock` picks up the new
   `opentimstdf`/`opentimstdf-py` version entries.

## 3. Update the changelog

In `CHANGELOG.md`, turn the `## [Unreleased]` section into a dated release
section (`## [X.Y.Z] - YYYY-MM-DD`), and leave a fresh empty `## [Unreleased]`
heading above it for future entries.

## 4. Commit

Commit the version bump and changelog together:

```
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "release: vX.Y.Z"
```

(See recent `release: vX.Y.Z` commits in `git log` for the style - a short
body summarizing what's bundled is welcome but not required.)

## 5. Tag and push

```
git tag -a vX.Y.Z -m vX.Y.Z
git push origin main
git push origin vX.Y.Z
```

Pushing the tag is what triggers `.github/workflows/publish.yml`
(`cargo publish` to crates.io, plus building and publishing wheels/sdist to
PyPI).

## 6. Verify

- Check the `Publish` workflow run for the new tag went green:
  `gh run list -w publish.yml -c $(git rev-parse vX.Y.Z)`
- Confirm the new version shows up on
  [crates.io](https://crates.io/crates/opentimstdf) and
  [PyPI](https://pypi.org/project/opentimstdf/).
