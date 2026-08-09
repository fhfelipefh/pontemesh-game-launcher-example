# Security Policy

## Reporting a vulnerability

Please use GitHub private vulnerability reporting when it is available. Otherwise, contact the repository owner privately through their GitHub profile. Do not publish credentials, exploit details, or sensitive logs in a public issue.

## Example credentials

`launcher.toml` and `runtime/` are ignored by Git because they contain the local downloader token, generated administrative password, downloads, and installation state. The token is intentionally limited to read and access-package scopes. On Unix, the bootstrap creates secret files with owner-only permissions; on Windows, they inherit the current user's filesystem access control. The values in `compose.yaml` are isolated demonstration database credentials and must not be reused for an internet-facing deployment.

The Compose ports bind only to `127.0.0.1`. The launcher permits plaintext HTTP only for loopback hosts and requires HTTPS for every remote Origin. Use a TLS reverse proxy and replace the demonstration identity flow before making an Origin reachable from another computer.

Release descriptors are limited to 10,000 files and 20 GiB. Disk preflight reserves room for both the persistent fragment cache and staged installation, plus operational overhead. These limits are example defaults and should be adjusted deliberately for a real game.

A distributed executable cannot protect a reusable bearer token. Public, protected downloads require user authentication through a mature identity provider and a backend exchange for short-lived credentials. Do not compile the demonstration token into a launcher.

This repository is an educational local-network example. Review the production security guidance in the [Ponte Mesh Server security documentation](https://github.com/fhfelipefh/pontemesh-server/blob/main/docs/SECURITY.md) before exposing an Origin outside a trusted development network.
