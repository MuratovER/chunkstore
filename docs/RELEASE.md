# Release flow (PyPI + crates.io)

One push to `main` with a version bump publishes **Python (PyPI)**, **Rust (crates.io)**, and creates a **GitHub Release**.

```mermaid
flowchart LR
  push[Push version bump to main] --> releaseYml[release.yml]
  releaseYml --> ghRelease[GitHub Release vX.Y.Z]
  releaseYml --> pypi[pypi.yml via workflow_run]
  releaseYml --> crates[crates-io.yml via workflow_run]
  pypi --> PyPI[PyPI chunkstore]
  crates --> Crates[crates.io chunkstore-core]
```

GitHub does not deliver `release: published` to other workflows when the release is
created with the default `GITHUB_TOKEN`. `pypi.yml` and `crates-io.yml` therefore
also listen for **`workflow_run`** after **Release** completes and publish only when
the new tag points at that commit.

## 1. Bump version (maintainer)

Keep in sync:

| File | Field |
|------|--------|
| [`Cargo.toml`](../Cargo.toml) | `[workspace.package] version` |
| [`python/pyproject.toml`](../python/pyproject.toml) | `[project] version` |
| [`python/python_src/chunkstore/__init__.py`](../python/python_src/chunkstore/__init__.py) | `__version__` |

Update [`CHANGELOG.md`](../CHANGELOG.md), then:

```bash
git add Cargo.toml python/pyproject.toml python/python_src/chunkstore/__init__.py CHANGELOG.md
git commit -m "Release vX.Y.Z: …"
git push origin main
```

**Do not** create the tag manually — [`release.yml`](../.github/workflows/release.yml) creates `vX.Y.Z` when the tag does not exist yet.

## 2. Automatic publish

| Trigger | Workflow | Destination |
|---------|----------|-------------|
| **Release** workflow completed (new tag) | [`pypi.yml`](../.github/workflows/pypi.yml) | [pypi.org/project/chunkstore](https://pypi.org/project/chunkstore/) |
| **Release** workflow completed (new tag) | [`crates-io.yml`](../.github/workflows/crates-io.yml) | [crates.io/crates/chunkstore-core](https://crates.io/crates/chunkstore-core) |

Check **Actions** after push: `Release` → `Publish to PyPI` → `Publish to crates.io`.

Both publish jobs appear under **Deployments** (`pypi` and `crates-io` environments).

## One-time setup

### PyPI (trusted publishing — no API token in secrets)

See [docs/PYPI.md](PYPI.md). Summary:

| Field | Value |
|-------|--------|
| PyPI project | `chunkstore` |
| GitHub owner / repo | `MuratovER` / `chunkstore` |
| Workflow | `pypi.yml` |
| Environment | `pypi` |

GitHub **Settings → Environments → `pypi`** (optional approval gate).

### crates.io

1. Create API token at [crates.io/settings/tokens](https://crates.io/settings/tokens) (scope: publish `chunkstore-core`).
2. Verify your email at [crates.io/settings/profile](https://crates.io/settings/profile) (required to publish).
3. Add secret **`CARGO_REGISTRY_TOKEN`** — repository secret or under environment **`crates-io`** (Settings → Environments → `crates-io`; optional approval gate).
4. First publish may require `cargo owner --add github:MuratovER:chunkstore-core` from a maintainer machine.

GitHub creates the **`crates-io`** environment on first workflow run if it does not exist yet.

## Manual re-publish

If a workflow failed after the release was published, or a release predates the
`workflow_run` chain (e.g. v0.2.0 tag exists but PyPI/crates.io never ran):

- **PyPI:** Actions → **Publish to PyPI** → **Run workflow** (builds from current `main`).
- **crates.io:** Actions → **Publish to crates.io** → **Run workflow** (requires
  `CARGO_REGISTRY_TOKEN`). crates.io does not allow republishing the same version —
  if `0.2.0` is already published, bump a patch release instead.

## Go module

Go consumers use git tags (no separate registry):

```bash
go get github.com/MuratovER/chunkstore/go@vX.Y.Z
./scripts/build-core.sh
```

See [go/README.md](../go/README.md) and [docs/CRATES.md](CRATES.md).
