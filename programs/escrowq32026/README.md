# escrowq32026

A timed token escrow. See the [root README](../../README.md#escrow) for the design.

Instructions: `make`, `take`, `refund`, `update`.

```bash
cargo build-sbf --arch v0 --manifest-path programs/escrowq32026/Cargo.toml
cargo test -p escrowq32026
```
