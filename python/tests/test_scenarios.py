from __future__ import annotations

import tempfile
from pathlib import Path

import pytest

from chunkstore import ChunkClient, ChunkStore, FilesystemBackend


def byte_at(i: int) -> int:
    x = (i * 0x9E3779B9) & 0xFFFFFFFFFFFFFFFF
    return ((x >> 24) ^ (x >> 16) ^ (x >> 8)) & 0xFF


@pytest.fixture
def memory_store() -> ChunkStore:
    return ChunkStore.memory()


@pytest.fixture
def fs_store(tmp_path: Path) -> ChunkStore:
    backend = FilesystemBackend(tmp_path / "chunks")
    return ChunkStore.open(backend)


def test_scenario_01_unique_file_download(memory_store: ChunkStore) -> None:
    digests = memory_store.ingest("doc", b"unique-payload")
    assert len(digests) == 1
    assert memory_store.read("doc") == b"unique-payload"


def test_scenario_02_duplicate_file_dedups(memory_store: ChunkStore) -> None:
    memory_store.ingest("a", b"same-payload")
    memory_store.ingest("b", b"same-payload")
    stats = memory_store.stats()
    assert stats.savings_pct > 0.0
    assert stats.stored_bytes * 2 == stats.total_bytes


def test_scenario_03_partial_overlap_reuses_prefix(memory_store: ChunkStore) -> None:
    chunk_size = 64
    prefix = b"\x01" * (chunk_size * 2)
    file_a = prefix + b"AAA"
    file_b = prefix + b"BBBBB"
    da = memory_store.ingest_fixed("a", file_a, chunk_size)
    db = memory_store.ingest_fixed("b", file_b, chunk_size)
    assert da[:2] == db[:2]
    assert da[2] != db[2]


def test_scenario_04_delete_one_of_two_keeps_shared(fs_store: ChunkStore) -> None:
    fs_store.ingest("a", b"shared-prefix")
    digests = fs_store.ingest("b", b"shared-prefix")
    fs_store.delete("a")
    assert fs_store.read("b") == b"shared-prefix"
    assert digests


def test_scenario_05_delete_last_file_gcs(fs_store: ChunkStore, tmp_path: Path) -> None:
    digests = fs_store.ingest("only", b"gc-me")
    digest = digests[0]
    fs_store.delete("only")
    root = tmp_path / "chunks"
    assert not (root / digest).exists()


def test_scenario_07_streaming_upload_from_disk(memory_store: ChunkStore) -> None:
    data = bytes([5]) * (256 * 1024 + 17)
    with tempfile.NamedTemporaryFile() as tmp:
        tmp.write(data)
        tmp.flush()
        with open(tmp.name, "rb") as reader:
            memory_store.ingest_reader("streamed", reader, 64 * 1024)
    assert memory_store.read("streamed") == data


def test_scenario_08_cdc_beats_fixed_on_prefix_insert(memory_store: ChunkStore) -> None:
    size = 20 * 1024 * 1024
    base = bytes(byte_at(i) for i in range(size))
    edited = b"\xab" + base

    fixed = ChunkStore.memory()
    fixed.ingest("base", base)
    fixed.ingest("edited", edited)
    fixed_savings = fixed.stats().savings_pct

    cdc = ChunkStore.memory()
    cdc.ingest_cdc("base", base)
    cdc.ingest_cdc("edited", edited)
    cdc_savings = cdc.stats().savings_pct

    assert fixed_savings < 5.0
    assert cdc_savings > 30.0


def test_scenario_09_shared_binary_block_savings(memory_store: ChunkStore) -> None:
    block = b"\xbb" * (4 * 1024 * 1024)
    a = block + b"tail-a"
    b = block + b"tail-b-longer"
    memory_store.ingest("a", a)
    memory_store.ingest("b", b)
    assert memory_store.stats().savings_pct >= 40.0


def test_client_roundtrip(memory_store: ChunkStore) -> None:
    client = ChunkClient(memory_store)
    client.upload_file("doc", b"hello")
    assert client.download_file("doc") == b"hello"
    client.delete_file("doc")
