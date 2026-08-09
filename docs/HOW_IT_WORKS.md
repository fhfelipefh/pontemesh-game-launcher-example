# How the examples work

Every language consumes the same `launcher.toml`, Origin, downloader credential, and
release descriptor. The only difference is how application code reaches the SDK.

```mermaid
sequenceDiagram
    actor Player
    participant Launcher as Language launcher
    participant SDK as Ponte Mesh SDK
    participant Origin
    participant Source as Authorized source
    participant Disk

    Player->>Launcher: Check for update
    Launcher->>SDK: Sync releases/stable.json
    SDK->>Origin: Request access package
    Origin-->>SDK: Manifest, policy, and authorized sources
    SDK->>Source: Request missing fragments
    Source-->>SDK: Untrusted bytes
    SDK->>SDK: Validate fragment and object hashes
    SDK-->>Launcher: Verified release descriptor
    Launcher->>Launcher: Validate paths, sizes, hashes, and order
    loop Files in release order
        Launcher->>SDK: Sync file into staging
        SDK->>Source: Request verified fragments
        Launcher->>Launcher: Verify declared release file
    end
    Launcher->>Disk: Replace installation atomically
    Disk-->>Player: Ready to play or previous version restored
```

## Repository map

```text
examples/rust/        direct Rust SDK and full cache/P2P features
examples/python/      ctypes wrapper over the stable C ABI
examples/javascript/  Koffi wrapper over the stable C ABI
examples/cpp/         C++17 dynamic loader and RAII client
docker/               shared local Origin image
sample-content/       small metadata files uploaded by bootstrap
runtime/              ignored credentials, downloads, and installations
native/               ignored extracted native SDK package
```

## Trust boundaries

```mermaid
flowchart LR
    A[Application language] -->|local calls| B[Rust SDK or C ABI]
    B -->|bearer token| O[Origin control plane]
    O -->|short-lived access package| B
    U[Origin, Replica/Edge, or peer] -->|untrusted fragments| B
    B -->|hash-validated object| S[Language staging directory]
    S -->|validated release path and atomic swap| G[Installed game]
```

Python `ctypes`, JavaScript Koffi, and the C++ dynamic loader are thin bridges. They
do not decide which source is trusted and do not validate network fragments; those
security controls remain inside the native SDK.

## Release validation

All implementations require schema version 1, a non-empty release, safe portable
relative paths, unique destinations, valid sizes, SHA-256-shaped digests, at most
10,000 files, and at most 20 GiB. Python and JavaScript additionally compare the
release-level SHA-256 after the SDK completes. C++ relies on the SDK's complete-object
hash validation and checks the descriptor size before installation.

## Installation isolation

Each stack installs into its own directory under `runtime/installations/`. A failed
final rename restores the previous directory. This lets all examples run against the
same release without overwriting one another.
