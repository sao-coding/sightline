# Security policy

## Supported versions

Only the latest release gets security fixes.

## Reporting a vulnerability

Please report security problems **privately**, not in a public issue:

1. Open [Report a vulnerability](../../security/advisories/new) in the Security tab.
2. Say what you found, which version, and how to reproduce it.

You can write in English or Chinese. I will reply as soon as I can and credit you in the release
notes unless you prefer not to be named.

## Scope

sightline runs locally, makes no network connections, and only reads window pixels and process
statistics. Reports of interest include: a way to make `sight` capture a window that opted out of
capture, unexpected file writes outside the `--out` path or the temp folder, and problems in
`install.ps1` or the release assets (for example a checksum that does not match).
