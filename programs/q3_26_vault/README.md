# q3_26_vault

A personal SOL vault. See the [root README](../../README.md#vault) for the design.

Instructions: `initialize`, `deposit(amount)`, `withdraw(amount)`, `close`.

```bash
cargo build-sbf --arch v0 --manifest-path programs/q3_26_vault/Cargo.toml
cargo test -p q3_26_vault
```
