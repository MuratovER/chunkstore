# Security policy

## Supported versions

| Version | Supported |
|---------|-----------|
| 0.3.x   | Yes |
| 0.2.x   | Security fixes only |
| < 0.2   | No |

## Reporting a vulnerability

Please **do not** open a public GitHub issue for security reports.

Email **cooperative.entr@gmail.com** with:

- Affected crate / package version (`chunkstore-core`, PyPI `chunkstore`, Go module)
- Description and impact (integrity of digests, GC, backend path traversal, FFI memory, etc.)
- Reproduction if you have one

You should receive an acknowledgement within a few days. We will coordinate a fix and a disclosure date.

## Scope

In scope: the Rust core, C-API, Python and Go wrappers, on-disk metadata format, and official examples **as libraries**.

Out of scope: misconfigured S3 buckets, Postgres exposed to the internet, or application-level access control around `file_id` (chunkstore does not authenticate callers).
