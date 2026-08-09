# Security Policy

## Reporting a vulnerability

Please use GitHub private vulnerability reporting when it is available. Otherwise, contact the repository owner privately through their GitHub profile. Do not publish credentials, exploit details, or sensitive logs in a public issue.

## Example credentials

`launcher.toml` is ignored by Git because it contains the local downloader token used by the launcher. The token is intentionally limited to read and access-package scopes. The values in `compose.yaml` are local demonstration database credentials and must not be reused for an internet-facing deployment.

A distributed executable cannot protect a reusable bearer token. Public, protected downloads require user authentication through a mature identity provider and a backend exchange for short-lived credentials. Do not compile the demonstration token into a launcher.

This repository is an educational local-network example. Review the production security guidance in the [Ponte Mesh Server security documentation](https://github.com/fhfelipefh/pontemesh-server/blob/main/docs/SECURITY.md) before exposing an Origin outside a trusted development network.
