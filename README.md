[![CI](https://github.com/misfitdev/git-router/actions/workflows/ci.yml/badge.svg)](https://github.com/misfitdev/git-router/actions/workflows/ci.yml)
[![Release](https://github.com/misfitdev/git-router/actions/workflows/release.yml/badge.svg)](https://github.com/misfitdev/git-router/actions/workflows/release.yml)
[![Crates.io](https://img.shields.io/crates/v/git-router)](https://crates.io/crates/git-router)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![SLSA 1](https://slsa.dev/images/gh-badge-level1.svg)](https://slsa.dev)

# git-router

Route SSH keys and HTTPS credentials by matching the org/namespace in
git remote URLs. Replaces SSH host aliases and `url.<base>.insteadOf`
rules.

## Install

```sh
cargo install git-router
```

Or download a binary from [Releases](https://github.com/misfitdev/git-router/releases).
Every release includes SHA256 checksums and
[SLSA build provenance](https://slsa.dev) attestations, verifiable with:

```sh
gh attestation verify git-router-*.tar.gz --owner misfitdev
```

## Quick start

```sh
# Wire git-router into global gitconfig
git router init

# Add routes
git router add github.com personal --ssh-key ~/.ssh/personal.pub
git router add github.com work-org --ssh-key ~/.ssh/work.pub --token ghp_xxxx
git router add gitlab.com my-group --ssh-key ~/.ssh/gitlab.pub

# Verify setup
git router doctor
```

Now `git clone`, `git fetch`, and `git push` automatically use the right
SSH key and HTTPS token based on the org in the remote URL.

## How it works

`git-router` is a single binary with three modes, detected by how git
invokes it:

**SSH wrapper** (`core.sshCommand = git-router ssh-wrap`) -- git calls
this for SSH remotes. It parses the host and org from the SSH arguments,
looks up the matching route, and execs `ssh` with the correct
`IdentityFile`.

**Credential helper** (`credential.helper = git-router credential-helper`)
-- git calls this for HTTPS remotes. It reads the host and path from
stdin, matches a route, and returns the token.

**CLI** (`git router <subcommand>`) -- manages routes and verifies
configuration.

## Config

Routes live in `~/.config/git-router/config.toml`:

```toml
[[route]]
host = "github.com"
org = "personal"
ssh_key = "~/.ssh/personal.pub"

[[route]]
host = "github.com"
org = "work-org"
ssh_key = "~/.ssh/work.pub"
token = "ghp_xxxx"

[[route]]
host = "gitlab.com"
org = "my-group"
ssh_key = "~/.ssh/gitlab.pub"
```

For GitLab nested groups, routes match progressively shorter path
prefixes. A route for `my-group/infra` matches before a broader
`my-group` route.

## Commands

| Command | Description |
|---------|-------------|
| `git router init` | Write `core.sshCommand` and `credential.helper` to global gitconfig |
| `git router add <host> <org> [--ssh-key PATH] [--token TOKEN]` | Add or update a route |
| `git router list` | Print the route table |
| `git router remove <host> <org>` | Remove a route |
| `git router doctor` | Verify config, keys, gitconfig wiring, and SSH agent |

## Design constraints

- Never modifies `~/.ssh/config`
- Passes through on no match -- never blocks a git operation
- No daemons, no background processes, no temp files
- Atomic config writes (write-then-rename)
- Exit codes pass through from `ssh` or credential operations

## Security

See [SECURITY.md](SECURITY.md) for the vulnerability reporting policy.

## License

MIT
