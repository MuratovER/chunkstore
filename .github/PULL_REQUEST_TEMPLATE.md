## Summary

-

## Test plan

- [ ] `cargo test -p chunkstore-core`
- [ ] `cargo clippy -p chunkstore-core --all-targets -- -D warnings`
- [ ] Python tests if wrappers changed (`maturin develop` + `pytest`)
- [ ] Go tests if C-API changed (`./scripts/build-core.sh` + `go test`)
- [ ] On-disk format or C-API change called out below (or N/A)

## Notes

Format / C-API impact:
