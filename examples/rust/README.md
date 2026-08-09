# Rust example

This is the most complete launcher in the repository. It uses the native Rust SDK
directly and demonstrates release discovery, persistent fragment cache, progress,
hash validation, disk preflight, staging, rollback, and optional peer seeding.

From the repository root:

```bash
cargo run --release --locked --manifest-path examples/rust/Cargo.toml
```

The game is installed under `runtime/installations/rust/`.
