# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| latest  | Yes       |

## Reporting a Vulnerability

**Do not open a public issue for security vulnerabilities.**

Please report security issues by emailing:

&#115;&#101;&#99;&#117;&#114;&#105;&#116;&#121;&#64;&#109;&#105;&#115;&#102;&#105;&#116;&#46;&#100;&#101;&#118;

Include:

- Description of the vulnerability
- Steps to reproduce
- Impact assessment
- Suggested fix (if any)

You should receive an acknowledgment within 48 hours. We aim to release
a fix within 7 days for critical issues.

## Scope

The following are in scope:

- Credential or token leakage through config handling, logging, or error messages
- SSH key path traversal or injection via crafted remote URLs
- Arbitrary command execution through malformed git arguments
- Config file tampering leading to credential misdirection

## Security Design

- Tokens are stored as plaintext in `~/.config/git-router/config.toml`.
  Users are responsible for filesystem permissions on this file.
- Config writes use atomic rename to prevent partial-write corruption.
- The SSH wrapper passes through to `ssh` via `exec`; it never interprets
  shell metacharacters in arguments.
- No network access, no daemons, no temp files beyond atomic config writes.
