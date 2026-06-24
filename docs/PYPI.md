# PyPI publishing

**Live:** [chunkstore on PyPI](https://pypi.org/project/chunkstore/) — published automatically on GitHub Release via [`pypi.yml`](../.github/workflows/pypi.yml).

See [docs/RELEASE.md](RELEASE.md) for the full PyPI + crates.io release pipeline.

## One-time setup

### 1. PyPI project

1. Register at [pypi.org](https://pypi.org/account/register/) (and optionally [test.pypi.org](https://test.pypi.org/account/register/)).
2. Reserve the package name **`chunkstore`** on PyPI (first upload claims the name).

### 2. Trusted publishing (recommended — no API token in GitHub Secrets)

Configure on **PyPI → Account settings → Publishing**:

| Field | Value |
|-------|--------|
| PyPI project name | `chunkstore` |
| Owner | `MuratovER` |
| Repository name | `chunkstore` |
| Workflow name | `pypi.yml` |
| Environment name | `pypi` |

Repeat for **TestPyPI** with environment name `testpypi` if you use manual TestPyPI runs.

### 3. GitHub environments (optional but recommended)

**Settings → Environments**:

| Environment | Purpose |
|-------------|---------|
| `pypi` | Production uploads; add required reviewers if you want approval before publish |
| `testpypi` | Dry-run uploads from `workflow_dispatch` |

No secrets needed when using trusted publishing (OIDC).

## Release flow

### Version bump

Keep these in sync:

| File | Field |
|------|--------|
| [`Cargo.toml`](../Cargo.toml) | `[workspace.package] version` |
| [`python/pyproject.toml`](../python/pyproject.toml) | `[project] version` |
| [`python/python_src/chunkstore/__init__.py`](../python/python_src/chunkstore/__init__.py) | `__version__` |

```bash
# example for 0.2.1
# edit version files + CHANGELOG.md, then:
git add Cargo.toml python/pyproject.toml python/python_src/chunkstore/__init__.py CHANGELOG.md
git commit -m "Release v0.2.1: …"
git push origin main
```

### Automatic publish (PyPI + crates.io)

Push to `main` with a new version → [`release.yml`](../.github/workflows/release.yml) creates GitHub Release `vX.Y.Z` → both publish workflows run:

| Workflow | Registry |
|----------|----------|
| [`pypi.yml`](../.github/workflows/pypi.yml) | [PyPI `chunkstore`](https://pypi.org/project/chunkstore/) |
| [`crates-io.yml`](../.github/workflows/crates-io.yml) | [crates.io `chunkstore-core`](https://crates.io/crates/chunkstore-core) |

Full maintainer checklist: [docs/RELEASE.md](RELEASE.md).

**One-time:** PyPI trusted publishing ([PYPI.md](PYPI.md)); crates.io secret `CARGO_REGISTRY_TOKEN` in GitHub Actions.

**Manual** (re-trigger without version bump): GitHub → Releases → Publish release for an existing tag, or re-run failed workflow jobs in Actions.

| Platform | Wheel |
|----------|--------|
| Linux | `manylinux` x86_64, aarch64 |
| macOS | universal2 (Intel + Apple Silicon) |
| Windows | x64 |

All wheels are abi3 (`cp310-abi3`, Python ≥ 3.10).

### TestPyPI (manual dry run)

**Actions → Publish to PyPI → Run workflow** → check **Publish to TestPyPI** → Run.

Install from TestPyPI:

```bash
pip install --index-url https://test.pypi.org/simple/ --extra-index-url https://pypi.org/simple/ chunkstore
```

(`--extra-index-url` pulls build deps like `maturin` metadata deps from real PyPI.)

## Local build (debug)

```bash
cd python
python -m venv .venv && source .venv/bin/activate
pip install maturin
maturin build --release --sdist --out dist
ls dist/
```

## What gets published

- **sdist** — source distribution (Rust + Python, builds via maturin)
- **wheels** — abi3 for Python ≥ 3.10 (`cp310-abi3`):
  - Linux: `manylinux_2_17` x86_64 + aarch64
  - macOS: universal2 (x86_64 + arm64)
  - Windows: AMD64
