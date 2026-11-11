# Publishing

`gdtidy` releases are tag-driven. Pushing a `v<version>` tag starts **Publish release** (`.github/workflows/release.yml`), which checks that the tag matches the version in `Cargo.toml`, builds the archives, publishes to crates.io (trusted publishing, no token), then creates the GitHub release. Tags with a prerelease suffix (`v1.3.0-rc.1`) become GitHub prereleases and are not sent to gdsuite.

The version source of truth is the committed repo state: `Cargo.toml` and `Cargo.lock`. `scripts/release.py bump` updates the version and `cargo update --workspace` the lockfile.

## Release from GitHub

**Actions → Cut release → Run workflow**, on `main`:

| Input | Values |
| --- | --- |
| bump | `patch`, `minor` or `major` |
| pre | `none` for a stable release, or `alpha`, `beta`, `rc` |
| version | an exact version instead, such as `1.3.0-rc.1` (overrides the two above) |
| dry run | pick the version and run the tests, push nothing |

It bumps the version, runs `cargo test --locked`, commits `release: v<version>` to `main`, pushes the tag and starts Publish release and `docs.yml`. The run summary shows the version it picked.

How the version is picked, from the current one:

| Current | bump | pre | Next |
| --- | --- | --- | --- |
| 1.2.3 | patch | none | 1.2.4 |
| 1.2.3 | minor | none | 1.3.0 |
| 1.2.3 | major | none | 2.0.0 |
| 1.2.3 | minor | beta | 1.3.0-beta.1 |
| 1.3.0-beta.1 | minor | beta | 1.3.0-beta.2 |
| 1.3.0-beta.2 | minor | rc | 1.3.0-rc.1 |
| 1.3.0-rc.1 | minor | none | 1.3.0 |
| 1.3.0-rc.1 | major | none | 2.0.0 |

From a prerelease, a bump that already leads to its version stays on it, so keep the same `bump` through the betas, the release candidates and the release. Going back (from `rc` to `beta`) is refused. Check a pick without releasing with `python scripts/release.py next-version minor rc`.

## Release from a checkout

From a clean `main` checkout:

```sh
python scripts/release.py bump 0.2.0
cargo update --workspace
cargo test --locked
git commit -am "release: v0.2.0"
git tag -a v0.2.0 -m "v0.2.0"
git push origin main v0.2.0
```

## Failed release recovery

If Publish release fails but the tag already points to the correct release commit, prefer fixing the workflow and re-running the job.

If the fix requires a new commit and the original release did not publish to crates.io, move the same tag to the new commit instead of inventing a patch version. Pushing the moved tag starts Publish release again:

```sh
git tag -f -a v0.2.0 -m v0.2.0
git push origin -f v0.2.0
```

Once crates.io has a version, it cannot be published again: release the next patch instead.

The docs site is published from release and prerelease tags; see [docs/README.md](docs/README.md#published-versions).
