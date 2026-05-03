<h1 align="center">git-router</h1>

<p align="center">
<a href="https://github.com/misfitdev/git-router/actions/workflows/ci.yml"><img src="https://github.com/misfitdev/git-router/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
<a href="https://github.com/misfitdev/git-router/actions/workflows/release.yml"><img src="https://github.com/misfitdev/git-router/actions/workflows/release.yml/badge.svg" alt="Release"></a>
<a href="https://crates.io/crates/git-router"><img src="https://img.shields.io/crates/v/git-router" alt="Crates.io"></a>
<a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
<a href="https://github.com/misfitdev/git-router/attestations"><img src="https://slsa.dev/images/gh-badge-level1.svg" alt="SLSA 1"></a>
</p>

<p align="center">
Route SSH keys, HTTPS credentials, and committer identity by matching
the org/namespace in git remote URLs. Replaces SSH host aliases,
<code>url.&lt;base&gt;.insteadOf</code> rules, and per-directory gitconfig includes.
</p>

<p align="center">
<a href="#install">Install</a> &middot;
<a href="#quick-start">Quick start</a> &middot;
<a href="#commands">Commands</a> &middot;
<a href="#config">Config</a> &middot;
<a href="#how-it-works">How it works</a> &middot;
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

# Add routes (with optional per-route identity)
git router add github.com personal --ssh-key ~/.ssh/personal.pub \
  --user-name "Tucker" --user-email "tucker@personal.dev"
git router add github.com work-org --ssh-key ~/.ssh/work.pub --token ghp_xxxx \
  --user-name "Tucker DeWitt" --user-email "tucker@work.com"
git router add gitlab.com my-group --ssh-key ~/.ssh/gitlab.pub

# Verify setup
git router doctor
```

Now `git clone`, `git fetch`, and `git push` automatically use the right
SSH key, HTTPS token, and committer identity based on the org in the
remote URL.

## Commands

| Command | Description |
|---------|-------------|
| `git router init` | Wire git-router into global gitconfig (idempotent) |
| `git router add <host> <org> [OPTIONS]` | Add or update a route |
| `git router show` | Print all routes with full details |
| `git router list` | Print routes as one-liners |
| `git router remove <host> <org>` | Remove a route |
| `git router doctor` | Verify config, keys, and gitconfig wiring |

Options for `add`: `--ssh-key PATH`, `--token TOKEN`, `--user-name NAME`, `--user-email EMAIL`

## Config

Routes live in `~/.config/git-router/config.toml`:

```toml
[[route]]
host = "github.com"
org = "personal"
ssh_key = "~/.ssh/personal.pub"
user_name = "Tucker"
user_email = "tucker@personal.dev"

[[route]]
host = "github.com"
org = "work-org"
ssh_key = "~/.ssh/work.pub"
token = "ghp_xxxx"
user_name = "Tucker DeWitt"
user_email = "tucker@work.com"

[[route]]
host = "gitlab.com"
org = "my-group"
ssh_key = "~/.ssh/gitlab.pub"
```

`user_name` and `user_email` are optional. Routes without them fall back
to whatever `user.name`/`user.email` is set in your global gitconfig.

### Route matching

The `<org>` argument is separate from `<host>` and can contain slashes
for nested namespaces:

```sh
git router add gitlab.com acme --ssh-key ~/.ssh/gl-acme.pub
git router add gitlab.com acme/foo --ssh-key ~/.ssh/gl-foo.pub
```

When resolving a remote like `gitlab.com:acme/foo/bar.git`,
git-router tries progressively shorter path prefixes until it finds a
match: `acme/foo` matches before falling back to `acme`.

## How it works

`git-router` is a single binary that git invokes at two points during
remote operations:

**SSH wrapper** (`core.sshCommand = git-router ssh-wrap`) -- for SSH
remotes. Parses the host and org from the SSH arguments, looks up the
matching route, and execs `ssh` with the correct `IdentityFile`.

**Credential helper** (`credential.helper = git-router credential-helper`)
-- for HTTPS remotes. Reads the host and path from stdin, matches a
route, and returns the token.

**Identity** -- `git router init` generates gitconfig `includeIf` fragments
that set `user.name` and `user.email` based on the remote URL. This is
static config, not a runtime hook -- git evaluates it directly. Requires
git 2.36+.

### Design constraints

- Never modifies `~/.ssh/config`
- Passes through on no match -- never blocks a git operation
- No daemons, no background processes, no temp files
- Atomic config writes (write-then-rename)
- Exit codes pass through from `ssh` or credential operations

## Security

See [SECURITY.md](SECURITY.md) for the vulnerability reporting policy.

## License

MIT
