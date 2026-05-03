<h1 align="center">git-router</h1>

<p align="center">
<a href="https://github.com/misfitdev/git-router/actions/workflows/ci.yml"><img src="https://github.com/misfitdev/git-router/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
<a href="https://github.com/misfitdev/git-router/actions/workflows/release.yml"><img src="https://github.com/misfitdev/git-router/actions/workflows/release.yml/badge.svg" alt="Release"></a>
<a href="https://crates.io/crates/git-router"><img src="https://img.shields.io/crates/v/git-router" alt="Crates.io"></a>
<a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
<a href="https://github.com/misfitdev/git-router/attestations"><img src="https://slsa.dev/images/gh-badge-level1.svg" alt="SLSA 1"></a>
</p>

<p align="center">
Route SSH keys and HTTPS credentials by matching the org/namespace in
git remote URLs. Replaces SSH host aliases and <code>url.&lt;base&gt;.insteadOf</code>
rules.
</p>

<p align="center">
<a href="#install">Install</a> &middot;
<a href="#quick-start">Quick start</a> &middot;
<a href="#how-it-works">How it works</a> &middot;
<a href="#config">Config</a> &middot;
<a href="#commands">Commands</a> &middot;
<a href="#security">Security</a>
</p>

---

## Install

<details>
<summary>Linux</summary>

> | Repository      | Instructions                            |
> | --------------- | --------------------------------------- |
> | **crates.io**   | `cargo install git-router`              |
> | Homebrew        | `brew install misfitdev/tap/git-router` |
>
> Or download a prebuilt binary from [GitHub Releases].

</details>

<details>
<summary>macOS</summary>

> | Repository      | Instructions                            |
> | --------------- | --------------------------------------- |
> | **Homebrew**    | `brew install misfitdev/tap/git-router` |
> | crates.io       | `cargo install git-router`              |
>
> Or download a prebuilt binary from [GitHub Releases].

</details>

Every release includes SHA256 checksums and
[SLSA build provenance][attestations] attestations, verifiable with:

```sh
gh attestation verify git-router-*.tar.gz --owner misfitdev
```

[GitHub Releases]: https://github.com/misfitdev/git-router/releases
[attestations]: https://github.com/misfitdev/git-router/attestations

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
