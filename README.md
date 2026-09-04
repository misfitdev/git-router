<h1 align="center">git-router</h1>

<p align="center">
<a href="https://github.com/misfitdev/git-router/actions/workflows/ci.yml"><img src="https://github.com/misfitdev/git-router/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
<a href="https://github.com/misfitdev/git-router/actions/workflows/release.yml"><img src="https://github.com/misfitdev/git-router/actions/workflows/release.yml/badge.svg" alt="Release"></a>
<a href="https://crates.io/crates/git-router"><img src="https://img.shields.io/crates/v/git-router" alt="Crates.io"></a>
<a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
<a href="https://github.com/misfitdev/git-router/attestations"><img src="https://slsa.dev/images/gh-badge-level3.svg" alt="SLSA 3"></a>
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
| `git router completions <shell>` | Print a shell completion script |

Options for `add`: `--ssh-key PATH`, `--token TOKEN`, `--user-name NAME`, `--user-email EMAIL`

`doctor` exits non-zero when any check fails.

## Config

Routes live in the OS config directory:

| Platform | Path |
|----------|------|
| Linux    | `~/.config/git-router/config.toml` |
| macOS    | `~/Library/Application Support/git-router/config.toml` |

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

> **Include ordering.** Git applies the last value it reads. `git router init`
> appends its `include.path` to the end of your global gitconfig, so a
> `user.name`/`user.email` written *below* that include overrides every routed
> identity. `git router doctor` warns when it detects this.

Two kinds of file sit next to `config.toml`, both generated:

| File | Contents |
|------|----------|
| `git-router.gitconfig` | The single file `include.path` points at: credential sections and identity `includeIf` rules |
| `identity-<host>-<org>.gitconfig` | One committer identity per route that sets one |

They are rewritten in full on every `init`, `add`, and `remove`. Edit
`config.toml` instead; hand edits to the generated files are discarded.

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

**Credential helper** -- for HTTPS remotes. `git router init` writes a
per-route section into its generated gitconfig rather than registering a
global helper:

```
[credential "https://github.com/work-org"]
    useHttpPath = true
    helper = ""
    helper = "!git-router credential-helper"
```

Without `useHttpPath`, git omits the repo path and routes on the same host are
indistinguishable; setting it per route rather than globally leaves credential
lookup unchanged for every other host. Without the empty `helper`, a helper
declared earlier in your global config answers first and git-router is never
invoked. Sections are emitted only for routes that carry a token.

**Identity** -- `git router init` generates gitconfig `includeIf` fragments
that set `user.name` and `user.email` based on the remote URL. This is
static config, not a runtime hook -- git evaluates it directly. Requires
git 2.36+.

### Design constraints

- Never modifies `~/.ssh/config`
- Never removes or reorders credential helpers you configured yourself
- Passes through on no match -- never blocks a git operation. The one
  exception is an org with a token configured: those URLs are handled by
  git-router alone, so a corrupt `config.toml` prompts rather than falling
  back to another helper.
- No daemons, no background processes, no temp files
- Atomic config writes (write-then-rename)
- Exit codes pass through from `ssh` or credential operations
- `git router doctor` exits non-zero when a check fails

## Security

See [SECURITY.md](SECURITY.md) for the vulnerability reporting policy.

## License

MIT
