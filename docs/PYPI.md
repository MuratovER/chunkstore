# PyPI publishing

Automated via [`.github/workflows/pypi.yml`](../.github/workflows/pypi.yml).

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

```bash
# example for 0.1.1
# edit both files, then:
git add Cargo.toml python/pyproject.toml
git commit -m "chore: release v0.1.1"
git tag v0.1.1
git push origin main --tags
```

### Publish to PyPI

1. Open **GitHub → Releases → Draft a new release**.
2. Choose tag `v0.1.x` (create from `main` if needed).
3. Title: `v0.1.x` — paste short changelog.
4. Click **Publish release**.

The `pypi.yml` workflow builds sdist + Linux wheels (`x86_64`, `aarch64`) and uploads to PyPI.

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
- **wheels** — `manylinux` abi3 for Python ≥ 3.10 (`cp310-abi3`)

macOS / Windows wheels are not built in CI yet; users on those platforms can install from sdist if they have a Rust toolchain, or we can extend the matrix later.
