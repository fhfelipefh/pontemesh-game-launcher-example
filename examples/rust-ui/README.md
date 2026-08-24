# Ponte Mesh - Visual Test Launcher

Visual interface built with Rust and `egui` to execute, monitor, loop, and measure Ponte Mesh hybrid downloads across machines in local networks or against remote servers.

## Features

- **Origin Connection (HTTP/HTTPS)**: Connects to local development instances (`http://127.0.0.1:8080`) or remote servers (`https://134.65.234.41:8080`).
- **Flexible Configuration**: Full support for `.env`, `launcher.toml`, and in-app UI settings.
- **Directory Customization**: Configure installation directory and fragment cache directory directly from `.env` or the UI.
- **Synchronized Multi-Machine Scheduling**: Set a target clock time (HH:MM:SS) to trigger simultaneous downloads across multiple machines.
- **Automated Loop Stress Testing**: Run $N$ repeated cycles with automated cooldown and cache cleanup.
- **Real-Time P2P vs Fallback Metrics**: Live visual distribution of LAN peer transfers vs Origin server fallback, offload percentage, and fragment logs.
- **CSV/JSON Metric Export**: Export structured metrics (`benchmark_metrics.csv`) with timestamps, durations, and byte totals.

## Configuration

Copy `.env.example` to `.env`:

```bash
cp .env.example .env
```

Or set the variables in `.env`:

```env
PONTEMESH_ORIGIN_URL=https://134.65.234.41:8080
PONTEMESH_APPLICATION_TOKEN=pm_app_example_token
PONTEMESH_RELEASE_BUCKET=game-updates
PONTEMESH_RELEASE_MANIFEST_KEY=releases/stable.json
PONTEMESH_INSTALL_DIRECTORY=runtime/installations/rust
PONTEMESH_CACHE_DIRECTORY=runtime/installations/rust.pontemesh-cache
PONTEMESH_P2P_LISTEN_ADDRESS=/ip4/0.0.0.0/tcp/9095
PONTEMESH_P2P_ANNOUNCE_ADDRESS=
PONTEMESH_SEED_SECONDS=0
```

## Running the Visual Launcher

From the repository root:

```bash
cargo run --release --manifest-path examples/rust-ui/Cargo.toml
```

## Building a Standalone Executable

To produce a lightweight, standalone native binary to distribute across test machines:

```bash
cargo build --release --manifest-path examples/rust-ui/Cargo.toml
```

The compiled binary will be located at:
- Windows: `target/release/pontemesh-game-launcher-ui.exe`
- Linux/macOS: `target/release/pontemesh-game-launcher-ui`
