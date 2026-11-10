# Security policy

**Owner:** Angryvibes LLC, the repository owner

## Supported versions

RROFLOW is pre-alpha. The `rroflow` SDK (`rroflow-core`, `rroflow-cli`) is
released from this repository with signed build provenance (GitHub artifact
attestations); only the latest release and the current `main` branch receive
fixes. The ingest server (`rroflow-ingest`) is deployed by the owner from
`deploy/` and is not a user download; reports against it are still in scope.

## Reporting a vulnerability

Report vulnerabilities privately through GitHub's private vulnerability
reporting for this repository (the repository's **Security** tab, then
**Report a vulnerability**). Do not open a public issue, pull request, or
discussion for a security report.

RROFLOW reads coding-agent session transcripts. Do not attach a real
transcript, a Shippin (Nomen) token, a Raindrop key, or any secret to a
report; describe the shape of the data instead, or attach a synthetic
transcript. A report should state the affected crate, the released version or
revision, exact steps to reproduce, and the observed impact.

Findings about data handling are explicitly in scope: prompt or response text
reaching a sink the user did not opt into, redaction failures in tool inputs
and outputs, secrets surviving into events, and signal evidence that leaks
content.

## Response

The repository owner acknowledges a report, confirms or declines the finding,
and lands a fix through a pull request against `main`; a fix to a released SDK
version ships as a new release with attestations. The reporter is credited in
the release notes unless they ask not to be.

## Scope notes

- Prompt and response text is sent only when the user opts in per run; by
  default only usage, termination, tool status, and signal evidence are sent.
- `rroflow doctor` exits `2` when it finds a secret stored at Raindrop; a
  report that reproduces that exit on real data is treated as confirmed.
