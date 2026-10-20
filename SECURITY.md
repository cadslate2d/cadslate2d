# Security

## Reporting a vulnerability

Please do not open a public issue for a vulnerability. Report it privately through GitHub's private vulnerability reporting for this repository: **Security › Report a vulnerability** on <https://github.com/cadslate2d/cadslate2d>. You will hear back within a week; a fix and its release are coordinated with you, and you are credited unless you prefer not to be.

## What is in scope

CadSlate2D runs entirely in the browser, local-first: there is no account, no server, and no drawing leaves the page (DESIGN §43). The program's attack surface is therefore what it reads:

- **Imported files are untrusted.** The DXF reader must never panic, hang, or allocate without bound on any input; it is fuzzed (`scripts/fuzz.sh`) and bounded by `ImportLimits`. A file that crashes or stalls the program is a vulnerability.
- **Command-line input** is parsed by `cad-input`, which must never panic on any text.
- **The static build** must not fetch anything at run time and must work under a strict content-security policy.

Out of scope: the hosted AI service, which has its own repository and its own reporting path, and third-party browsers.

## Supported versions

The latest commit on `main` is the supported version; there are no releases yet.
