# b9 Plan

## Product Direction

Port `skout` from Go to Rust across multiple releases. Treat feature parity and readiness to supplant `skout` as verification outcomes rather than schedule assumptions.

See [`docs/skout-parity-checklist.md`](docs/skout-parity-checklist.md) for the active command parity tracker and historical reference map.

## Ideas To Explore

- Remove transitional `logout` after one released cleanup window, once users have had an opportunity to delete the retired Yahoo credential.
- Monitor the unofficial public Yahoo fantasy paths for denial or payload drift while preserving atomic replacement and last-complete-snapshot fallback.
