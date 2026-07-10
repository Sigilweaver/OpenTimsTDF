# Security Policy

## Supported Versions

| Version | Supported |
| ------- | --------- |
| latest  | Yes       |
| older   | No        |

Only the latest published release receives security updates.

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Report privately via [GitHub Security Advisories](https://github.com/Sigilweaver/OpenTimsTDF/security/advisories/new).

Include:

- A description of the vulnerability and its potential impact.
- Steps to reproduce or a proof of concept (a small `.d/` bundle is
  ideal; synthetic SQLite + frame bytes are even better).
- The crate version (Rust or Python wheel) and OS / toolchain.

Expect an initial acknowledgment within 7 days.

## Scope

In scope:

- **Parser correctness on malicious `.d/` (TDF) input.** OpenTimsTDF
  reads the SQLite `analysis.tdf` metadata file and the
  `analysis.tdf_bin` frame stream. Panics, out-of-bounds reads,
  undefined behavior, infinite loops, or memory exhaustion triggered
  by a crafted bundle are in scope. SQL-injection style attacks via
  the SQLite metadata are in scope.
- **Memory safety**: the crate forbids `unsafe_code`. A demonstrated
  unsafe-code violation reachable from safe API is a security bug.
- **Path-traversal or arbitrary-file-write bugs** in any helper that
  derives output paths from input filenames.
- **Supply-chain integrity** of published artifacts on crates.io and
  PyPI.

Out of scope:

- Denial of service via legitimately large `.d/` bundles. timsTOF
  acquisitions can be hundreds of GB by design.
- Inaccurate decoding of specific timsTOF acquisition modes. Those
  are correctness bugs - file them as regular issues.
- Vulnerabilities in third-party crates with no demonstrated exploit
  path through OpenTimsTDF.

## Disclosure

We follow coordinated disclosure. Reporters are credited in the
release notes unless they prefer to remain anonymous. We aim to ship
a fix within 30 days of confirming a high or critical issue.

## Note on reverse engineering

OpenTimsTDF was developed by clean-room reverse engineering of public
artifacts (PRIDE deposits, published specifications, format
documentation in the public domain). It does not depend on any
Bruker SDK or binary blob, and contains no Bruker proprietary code.
Bug reports about parser accuracy or coverage are welcome but are
not security issues unless they involve one of the categories above.

## Stack context

OpenTimsTDF is one of three vendor readers in the
[OpenMassSpec](https://github.com/Sigilweaver/OpenMassSpec) stack.
Sibling readers:
[OpenTFRaw](https://github.com/Sigilweaver/OpenTFRaw) (Thermo),
[OpenWRaw](https://github.com/Sigilweaver/OpenWRaw) (Waters).
Shared foundation:
[openmassspec-core](https://github.com/Sigilweaver/OpenMassSpecCore).
