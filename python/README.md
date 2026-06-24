# Python wrapper

```bash
python -m venv .venv
source .venv/bin/activate
pip install maturin pytest
export PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1  # if using Python 3.14+
maturin develop --release
pytest
```

Optional extras:

```bash
pip install ".[dev,s3,fastapi]"
```

**PyPI:** `pip install chunkstore` — [chunkstore 0.1.0](https://pypi.org/project/chunkstore/0.1.0/). Maintainer release flow: [docs/PYPI.md](../docs/PYPI.md).
