use std::io::Write;
use std::sync::Arc;

use chunkstore::{ChunkStore, FsBackend, MemoryBackend, Stats};
use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

struct PyBackend {
    obj: Py<PyAny>,
}

impl PyBackend {
    fn call_get(&self, py: Python<'_>, key: &str) -> PyResult<Option<Vec<u8>>> {
        let value = self.obj.call_method1(py, "get", (key,))?;
        if value.is_none(py) {
            return Ok(None);
        }
        Ok(Some(extract_bytes(value.bind(py))?))
    }

    fn call_put(&self, py: Python<'_>, key: &str, data: &[u8]) -> PyResult<()> {
        let bytes = PyBytes::new(py, data);
        self.obj.call_method1(py, "put", (key, bytes))?;
        Ok(())
    }

    fn call_exists(&self, py: Python<'_>, key: &str) -> PyResult<bool> {
        let value = self.obj.call_method1(py, "exists", (key,))?;
        value.extract(py)
    }

    fn call_delete(&self, py: Python<'_>, key: &str) -> PyResult<()> {
        self.obj.call_method1(py, "delete", (key,))?;
        Ok(())
    }
}

impl chunkstore::ChunkBackend for PyBackend {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, chunkstore::ChunkStoreError> {
        Python::attach(|py| {
            self.call_get(py, key)
                .map_err(|e| chunkstore::ChunkStoreError::backend(e.to_string()))
        })
    }

    fn put(&self, key: &str, data: &[u8]) -> Result<(), chunkstore::ChunkStoreError> {
        Python::attach(|py| {
            self.call_put(py, key, data)
                .map_err(|e| chunkstore::ChunkStoreError::backend(e.to_string()))
        })
    }

    fn exists(&self, key: &str) -> Result<bool, chunkstore::ChunkStoreError> {
        Python::attach(|py| {
            self.call_exists(py, key)
                .map_err(|e| chunkstore::ChunkStoreError::backend(e.to_string()))
        })
    }

    fn delete(&self, key: &str) -> Result<(), chunkstore::ChunkStoreError> {
        Python::attach(|py| {
            self.call_delete(py, key)
                .map_err(|e| chunkstore::ChunkStoreError::backend(e.to_string()))
        })
    }
}

fn extract_bytes(value: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(buffer) = value.extract::<&[u8]>() {
        return Ok(buffer.to_vec());
    }
    let bytes = value.getattr("encode")?.call0()?;
    bytes
        .extract::<Vec<u8>>()
        .map_err(|_| PyValueError::new_err("backend get() must return bytes or None"))
}

fn map_err(err: chunkstore::ChunkStoreError) -> PyErr {
    PyIOError::new_err(err.to_string())
}

struct PyWriter<'py> {
    py: Python<'py>,
    writer: Bound<'py, PyAny>,
}

impl Write for PyWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let bytes = PyBytes::new(self.py, buf);
        let written = self
            .writer
            .call_method1("write", (bytes,))?
            .extract::<usize>()
            .unwrap_or(buf.len());
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if self.writer.hasattr("flush").unwrap_or(false) {
            self.writer
                .call_method0("flush")
                .map_err(|e| std::io::Error::other(e.to_string()))?;
        }
        Ok(())
    }
}

#[pyclass(name = "Stats", from_py_object)]
#[derive(Clone)]
struct PyStats {
    total_bytes: u64,
    stored_bytes: u64,
    savings_pct: f64,
}

#[pymethods]
impl PyStats {
    #[getter]
    fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    #[getter]
    fn stored_bytes(&self) -> u64 {
        self.stored_bytes
    }

    #[getter]
    fn savings_pct(&self) -> f64 {
        self.savings_pct
    }

    fn __repr__(&self) -> String {
        format!(
            "Stats(total_bytes={}, stored_bytes={}, savings_pct={:.2})",
            self.total_bytes, self.stored_bytes, self.savings_pct
        )
    }
}

impl From<Stats> for PyStats {
    fn from(value: Stats) -> Self {
        Self {
            total_bytes: value.total_bytes,
            stored_bytes: value.stored_bytes,
            savings_pct: value.savings_pct(),
        }
    }
}

enum StoreInner {
    Memory(ChunkStore<Arc<MemoryBackend>>),
    Python(ChunkStore<Arc<PyBackend>>),
    Fs(ChunkStore<FsBackend>),
}

#[pyclass]
struct ChunkStoreHandle {
    inner: StoreInner,
}

#[pymethods]
impl ChunkStoreHandle {
    #[staticmethod]
    fn memory() -> PyResult<Self> {
        let store = ChunkStore::open(Arc::new(MemoryBackend::new())).map_err(map_err)?;
        Ok(Self {
            inner: StoreInner::Memory(store),
        })
    }

    #[staticmethod]
    fn open(backend: Bound<'_, PyAny>) -> PyResult<Self> {
        if backend.hasattr("root")? {
            let root: std::path::PathBuf = backend.getattr("root")?.extract()?;
            let fs = FsBackend::new(root).map_err(map_err)?;
            let store = ChunkStore::open(fs).map_err(map_err)?;
            return Ok(Self {
                inner: StoreInner::Fs(store),
            });
        }

        let py_backend = Arc::new(PyBackend {
            obj: backend.unbind(),
        });
        let store = ChunkStore::open(py_backend).map_err(map_err)?;
        Ok(Self {
            inner: StoreInner::Python(store),
        })
    }

    fn ingest(&self, file_id: &str, data: &[u8]) -> PyResult<Vec<String>> {
        match &self.inner {
            StoreInner::Memory(store) => store.ingest(file_id, data).map_err(map_err),
            StoreInner::Python(store) => store.ingest(file_id, data).map_err(map_err),
            StoreInner::Fs(store) => store.ingest(file_id, data).map_err(map_err),
        }
    }

    fn ingest_cdc(&self, file_id: &str, data: &[u8]) -> PyResult<Vec<String>> {
        match &self.inner {
            StoreInner::Memory(store) => store.ingest_cdc(file_id, data).map_err(map_err),
            StoreInner::Python(store) => store.ingest_cdc(file_id, data).map_err(map_err),
            StoreInner::Fs(store) => store.ingest_cdc(file_id, data).map_err(map_err),
        }
    }

    fn ingest_fixed(&self, file_id: &str, data: &[u8], chunk_size: usize) -> PyResult<Vec<String>> {
        match &self.inner {
            StoreInner::Memory(store) => store
                .ingest_fixed(file_id, data, chunk_size)
                .map_err(map_err),
            StoreInner::Python(store) => store
                .ingest_fixed(file_id, data, chunk_size)
                .map_err(map_err),
            StoreInner::Fs(store) => store
                .ingest_fixed(file_id, data, chunk_size)
                .map_err(map_err),
        }
    }

    fn read(&self, file_id: &str) -> PyResult<Vec<u8>> {
        match &self.inner {
            StoreInner::Memory(store) => store.read(file_id).map_err(map_err),
            StoreInner::Python(store) => store.read(file_id).map_err(map_err),
            StoreInner::Fs(store) => store.read(file_id).map_err(map_err),
        }
    }

    fn read_to_writer(
        &self,
        py: Python<'_>,
        file_id: &str,
        writer: Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let mut target = PyWriter {
            py,
            writer: writer.clone(),
        };
        match &self.inner {
            StoreInner::Memory(store) => {
                store.read_to_writer(file_id, &mut target).map_err(map_err)
            }
            StoreInner::Python(store) => {
                store.read_to_writer(file_id, &mut target).map_err(map_err)
            }
            StoreInner::Fs(store) => store.read_to_writer(file_id, &mut target).map_err(map_err),
        }
    }

    fn file_digests(&self, file_id: &str) -> PyResult<Vec<String>> {
        match &self.inner {
            StoreInner::Memory(store) => store.file_digests(file_id).map_err(map_err),
            StoreInner::Python(store) => store.file_digests(file_id).map_err(map_err),
            StoreInner::Fs(store) => store.file_digests(file_id).map_err(map_err),
        }
    }

    fn read_chunk(&self, digest: &str) -> PyResult<Vec<u8>> {
        match &self.inner {
            StoreInner::Memory(store) => store.read_chunk(digest).map_err(map_err),
            StoreInner::Python(store) => store.read_chunk(digest).map_err(map_err),
            StoreInner::Fs(store) => store.read_chunk(digest).map_err(map_err),
        }
    }

    fn delete(&self, file_id: &str) -> PyResult<()> {
        match &self.inner {
            StoreInner::Memory(store) => store.delete(file_id).map_err(map_err),
            StoreInner::Python(store) => store.delete(file_id).map_err(map_err),
            StoreInner::Fs(store) => store.delete(file_id).map_err(map_err),
        }
    }

    fn stats(&self) -> PyResult<PyStats> {
        let stats = match &self.inner {
            StoreInner::Memory(store) => store.stats().map_err(map_err)?,
            StoreInner::Python(store) => store.stats().map_err(map_err)?,
            StoreInner::Fs(store) => store.stats().map_err(map_err)?,
        };
        Ok(stats.into())
    }
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ChunkStoreHandle>()?;
    m.add_class::<PyStats>()?;
    Ok(())
}
