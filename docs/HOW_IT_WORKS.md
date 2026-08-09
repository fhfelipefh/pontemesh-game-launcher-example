# How the example works

The launcher knows the Origin URL, a runtime-injected downloader token, the release
bucket, the stable descriptor key, and the installation directory. Ponte Mesh owns
authorization, source selection, fragment validation, resume, and fallback.

## Update sequence

```mermaid
sequenceDiagram
    actor Player
    participant Launcher
    participant SDK as Ponte Mesh SDK
    participant Origin
    participant Source as Authorized source
    participant Disk

    Player->>Launcher: Check for update
    Launcher->>SDK: Sync releases/stable.json
    SDK->>Origin: Request access package
    Origin-->>SDK: Manifest, policy, authorized sources
    SDK->>Source: Request missing fragments
    Source-->>SDK: Untrusted bytes
    SDK->>SDK: Validate every fragment hash
    SDK-->>Launcher: Release descriptor
    loop Files in declared order
        Launcher->>SDK: Sync release object into staging
        SDK->>Source: Missing fragments only
        SDK->>SDK: Validate object and release hashes
    end
    Launcher->>Disk: Check space and swap staging directory
    Disk-->>Launcher: Installed or rolled back
    Launcher-->>Player: Ready to play
```

## Code map

```text
src/main.rs            starts a normal update
src/config.rs          loads local and environment configuration
src/launcher.rs        stages, verifies, installs, and rolls back releases
src/bin/bootstrap.rs   prepares a fresh local Origin automatically
compose.yaml           starts the Origin and PostgreSQL
sample-content/        contains small release metadata files
runtime/               ignored downloads, fragment cache, and installed game
```

The bootstrap creates the 8 MiB simulated game package in a temporary file. It is
uploaded to the local Origin and removed automatically instead of being committed to
the repository. It also creates a random administrative password and downloader token
inside ignored local files; no reusable application or Server credential is tracked.

## Trust boundaries

```mermaid
flowchart LR
    L[Public launcher] -->|runtime downloader token| O[Origin control plane]
    O -->|short-lived access package| S[SDK]
    P[Peer or Replica/Edge] -->|untrusted fragments| S
    O -->|fallback fragments| S
    S -->|hash-validated files| T[Staging directory]
    T -->|atomic swap| G[Installed game]
```

The downloader preset has no object-write scope. It limits a leaked demonstration
token but does not turn an embedded secret into a safe authentication mechanism.
Public protected applications need a real user identity and short-lived token
exchange outside the executable.

## Resume and rollback

Validated fragments are cached by manifest identity. A later process reads and
revalidates them before downloading anything. Completed files are written through a
temporary file. A release is assembled in a sibling staging directory, then the old
installation is renamed to a rollback directory before the new one is installed. If
the final rename fails, the previous directory is restored. Before downloading, the
launcher rejects releases above 20 GiB or 10,000 files and reserves disk space for
both the fragment cache and staging directory.

## LAN peer flow

```mermaid
flowchart LR
    A[Launcher A] -->|announce validated fragments| O[Origin]
    B[Launcher B] -->|request access package| O
    O -->|A is an authorized source| B
    B -->|fragment request| A
    A -->|fragment bytes| B
    B -->|validate against Origin manifest| D[Local cache]
```

Launcher A must remain running and advertise a reachable LAN address. The Origin
continues to control discovery and authorization; a peer never becomes the authority
for hashes or access.

The default Compose Origin is loopback-only. For launchers on separate computers,
publish the Origin through HTTPS and configure that URL on each launcher. Remote HTTP
URLs are rejected because they would expose the bearer token and trusted control-plane
responses to the network.
