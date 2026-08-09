# Security Policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's private vulnerability reporting feature for this repository. Do not publish credentials, exploit details, or sensitive logs in a public issue.

## Example credentials

`launcher.toml` is ignored by Git because it contains the local application token used by the launcher. The values in `compose.yaml` are local demonstration database credentials and must not be reused for an internet-facing deployment.

This repository is an educational local-network example. Review the production security guidance in the [Ponte Mesh Server security documentation](https://github.com/fhfelipefh/pontemesh-server/blob/main/docs/SECURITY.md) before exposing an Origin outside a trusted development network.
