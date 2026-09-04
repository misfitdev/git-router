# AGENTS.md

`git-router` routes SSH keys, HTTPS credentials, and committer identity by
org/namespace in git remote URLs. One Rust binary, invoked by git through
`core.sshCommand`, a credential helper, and generated gitconfig. No daemon, no
network, no background process. Read `README.md` first; this file covers only
what an agent needs on top of it.

## Tooling

Toolchain and tool versions are pinned in `.mise.toml`. Prefix commands with
`mise x --` when the shell has not activated mise.

| Task | Command |
|---|---|
| First-time setup | `mise trust && mise install && lefthook install` |
| Format | `cargo fmt` |
| Format check | `cargo fmt -- --check` |
| Lint | `cargo clippy --all-targets -- -D warnings` |
| Test | `cargo test` |
| Full gate | the three above, in that order |
| Release build | `cargo build --release` |
| Regenerate completions | `cargo run -- completions <shell>` |

`lefthook.yml` defines the gate and runs it on pre-commit; CI runs the same three
commands on Linux and macOS. There is no `justfile` — `lefthook.yml` is the
single definition of what must pass.

All commands are offline. Nothing in this repo needs credentials, a network, or a
remote forge.

## Rules of engagement

- **Never touch the operator's real git configuration.** `git router init` writes
  to the global gitconfig, and `add`/`remove` write to the OS config directory.
  Every test and every manual probe sets `HOME`, `XDG_CONFIG_HOME`,
  `GIT_CONFIG_GLOBAL`, and `GIT_CONFIG_SYSTEM=/dev/null` to a temporary
  directory first. `HOME` alone does not isolate anything on Linux, where
  `dirs::config_dir()` prefers `XDG_CONFIG_HOME`. A bare
  `git router init` or `git config --global` from an agent is a defect.
- **Never push a tag.** `.github/workflows/release.yml` fires on `v*` and creates
  a GitHub release, publishes to crates.io, and commits to the Homebrew tap.
  Releases are human-driven.
- Never push. Keep commits small and scoped to one change.
- **Never print a credential.** Running `git credential fill` against a real host
  makes the OS keychain answer and prints a live token to the transcript. Scope
  credential probes to `127.0.0.1` or a host that cannot resolve, and to a
  temporary `HOME`.
- Do not add a dependency without saying why the standard library or an existing
  dependency will not do. The tree is deliberately small.

## Definition of done

A change is not done when it compiles, and not done when the gate is green on
code you only asserted against itself. Work through this every time:

1. **Tests.** Anything that changes generated gitconfig, the credential protocol,
   or ssh argv gets a test that makes real git resolve the result. See Testing.
2. **Docs.** Update `README.md` and `SECURITY.md` in the same commit as the code.
   A new subcommand or flag updates the Commands section and the clap help text.
3. **Staleness sweep** (below). A doc that quietly became wrong is worse than no
   doc.
4. `cargo fmt -- --check && cargo clippy --all-targets -- -D warnings && cargo test`
   before committing.

### Staleness sweep

Before opening a PR, run `rg -l '<thing-you-changed>' README.md SECURITY.md src/`
and check:

- `README.md` — Commands, Config, Route matching, How it works, Design
  constraints. It is the first thing anyone reads, and its claims about how git
  is wired go stale the moment the generator changes.
- `SECURITY.md` — the Security Design list makes specific claims about token
  storage, transport, and what `init` will and will not modify. Any change to
  credential handling invalidates one of them.
- Clap `about`/`help` strings in `src/main.rs` — they are the CLI's own
  documentation and ship in the generated completions.
- `.github/workflows/` — a new build target, test file, or required tool lands
  untested if CI does not know about it.
- Comments near your change that described the old behavior.

Fix stale prose by rewriting it to describe what is true now. Do not annotate it
with what changed — the diff is the history.

## Testing

**Asserting on the strings git-router writes proves only that it is consistent
with itself.** The generated config is an input to git, and git is the only thing
that can say whether it works. A full suite of green unit tests coexisted with a
credential helper git could not invoke, because every test checked the output
against the same wrong expectation that produced it.

- `tests/git_integration.rs` — drives real `git`. Credential behavior through
  `git credential fill`, identity through `git config user.email` in a throwaway
  repo, and `init`'s effect on user config through `git config --get-all`. Every
  test isolates `HOME`, `XDG_CONFIG_HOME`, `GIT_CONFIG_GLOBAL`, and
  `GIT_CONFIG_SYSTEM`. Verify cross-platform test changes on Linux before
  pushing: `docker run --rm -v "$PWD":/w -w /w rust:1.95-slim bash -c
  'apt-get update -qq && apt-get install -y -qq git && CARGO_TARGET_DIR=/tmp/t cargo test'`.
- `tests/cli_roundtrip.rs` — CLI surface: argument validation, add/list/remove,
  completions.
- Unit tests in `src/` — pure logic: parsing, validation, path handling. A unit
  test calls the function under test. A test that reimplements the function's
  logic and then asserts on its own output tests nothing.
- New remote URL forms go in the `identity_applies_across_remote_url_forms`
  table. Git's glob matching is unforgiving about shapes that look equivalent.

## Git configuration gotchas

The semantics this tool is built on. Each of these produced a silent no-op that
looked like working configuration. Verify against local git rather than memory —
a temp `HOME` and `git credential fill` settle any of them in seconds.

- A `credential.helper` value that is not an absolute path and does not start
  with `!` is expanded to `git credential-<value>` and run through the shell.
  `git-router credential-helper` becomes `git credential-git-router
  credential-helper`, which is not a git command. Always `!`-prefix.
- Git omits `path` from credential requests unless `credential.useHttpPath` is
  true, and routes on the same host are indistinguishable without it. Set it in a
  URL-scoped section, never globally: globally it changes credential lookup for
  every host and every other helper.
- Git consults credential helpers in declaration order and stops at the first
  that returns a credential. A helper reached through `include.path` loses to one
  declared earlier in the global file. `helper = ""` resets the inherited list,
  and inside a `credential.<url>` section that reset is scoped to matching URLs.
- `credential.<url>` matches paths by component: `acme` does not match
  `acme-other`, but does match `acme/foo/bar.git`. Helpers accumulate across
  every matching section rather than most-specific-wins.
- Git applies the last value it reads. A `user.name` or `user.email` written
  below `include.path` in the global config overrides every routed identity.
- `git config --replace-all <key> <value>` with no value_regex deletes every
  existing value of that key. For multi-valued keys like `credential.helper` and
  `include.path` that silently destroys unrelated user configuration. Always pass
  an anchored, escaped value_regex.
- `includeIf "hasconfig:remote.*.url:<pattern>"` glob-matches the whole remote
  URL. Covering a host means covering scp-style, `ssh://`, `https://`, userinfo
  (`user@`), and non-default-port forms — they are separate patterns.

## Layout

- `src/main.rs` — clap definitions and dispatch. No logic.
- `src/cli.rs` — subcommands, global gitconfig mutation, `doctor`.
- `src/config.rs` — `config.toml` load/save, validation, generated gitconfig and
  identity fragments.
- `src/credential.rs` — the git credential helper protocol.
- `src/ssh_wrap.rs` — ssh argv parsing and rewriting.
- `tests/` — see Testing.
- `.github/workflows/ci.yml` — the gate. `release.yml` — tag-driven, human-only.

Generated files live in the OS config directory next to `config.toml`:
`git-router.gitconfig` (the single file `include.path` points at) and one
`identity-<host>-<org>.gitconfig` fragment per route with an identity. They are
rewritten wholesale on every `add`, `remove`, and `init`; nothing else may edit
them.

## Conventions

- Edition 2024, `rust-version = "1.85"`; the toolchain is pinned in `.mise.toml`.
  CI sets `RUSTFLAGS=-Dwarnings`.
- No `unwrap`/`expect` outside tests, except on an invariant the surrounding code
  establishes — and then say which one.
- Errors are `io::Result` with `io::ErrorKind::InvalidInput` for bad user input
  and `io::Error::other` for everything else. Message text is lowercase without a
  trailing period; `main` adds the `git-router: ` prefix.
- Config and identity files are written `0600` inside a `0700` directory via
  `write_private`/`create_dir_private`. Config writes are write-then-rename.
- Never log, print, or include a token in an error. `Display for Route` masks it.
- **Fail open.** No matching route means git behaves as if git-router were not
  installed. The single exception is an org with a token configured: git-router
  alone answers for those URLs, because the helper reset that makes it reachable
  also suppresses the fallback.
- Write `config.toml` last, after the generated gitconfig succeeds, so a failed
  write cannot leave a recorded route that was never wired.
- Validation is split by shape: a host is one path segment, an org may nest.
  Anything reaching a filename or a gitconfig pattern is validated at both the
  CLI boundary and at generation time, because `config.toml` is hand-editable.
- Commit subjects: imperative, sentence case, no type prefix, no attribution
  lines.

## Writing

Applies to code comments, `README.md`, `SECURITY.md`, clap help text, and PR
descriptions alike.

**Necessity test — the governing rule.** Every sentence must answer a question an
engineer will actually have while operating or changing this system. True and
interesting is not the bar; needed is. Before adding a sentence, name the reader
and the moment they need it. If you cannot, delete it.

Do not add:

- Color, asides, or framing ("worth noting", "interestingly", "it is tempting
  to").
- Observations about the repo that change no decision.
- Background an expert already has, or that the code states plainly.
- Speculation about what someone might do on another machine, in another repo, or
  at some future point, unless it is a live constraint on this one.

**Tense.** Present or future only: what a thing *is*, what it is *intended* to
become. Past tense only when the history is still an active constraint — a live
workaround, a migration in flight — and then state the constraint, not the story.

**Delete on sight.** These are always wrong here; rewrite the sentence without
them or drop it: `Previously`, `Used to`, `Formerly`, `Replaces`, `Changed from`,
`Historically`, `We decided`, `Note that`, `cleaner than`, `avoids drift`,
`source of truth`, `for clarity`, `load-bearing`.

**No stock intensifiers.** `load-bearing`, `critical`, `crucial`, `essential`,
`important to note` assert that something matters instead of saying what breaks.
Name the failure: not "the empty helper is load-bearing" but "without the empty
helper, a credential helper declared earlier in the global config answers first
and git-router is never invoked". If no failure can be named, the sentence fails
the necessity test.

**No process commentary.** Docs describe the system, not the work of building it.
Never write about team discussions, reviews, approvals, or who needs to agree to
what: "that conversation belongs before the merge", "pending sign-off", "we
should discuss". Merge-time consequences and open questions go in the PR
description or an issue.

**Audience is expert.** The reader is a git power user. Do not define a
credential helper, `includeIf`, gitconfig include, or SSH agent. Do not caveat
the obvious. Do document git's non-obvious semantics — the Gotchas above are
external constraints on the code's shape, which is exactly what is worth writing
down.

**Register: dry, not cute.** Declarative sentences. No jokes, metaphors,
rhetorical questions, exclamation marks, or emoji. Dry is not cryptic — write
grammatical sentences a human reads once and understands.

**Comments explain hard blocks. That is the entire job.**

- Write one when: an expression or control flow is genuinely hard to read; an
  external constraint forces the shape (a git semantic, an ssh argv rule, a
  filesystem permission requirement); or there is a gotcha that will bite whoever
  edits next.
- Do not write one when it restates the code, labels the function, teaches a
  general Rust or git concept, or argues for the design choice.
- One to three lines. If it needs a paragraph, it belongs in `README.md` or a
  `docs/<topic>.md`.
- Self-check before committing: delete the comment and reread the code. If
  nothing was lost, leave it deleted.

**Diagrams.** Architecture, flow, and sequence diagrams are Mermaid in a
` ```mermaid ` block — GitHub renders it. Do not hand-draw box diagrams in ASCII:
they misalign in proportional-font renderers and every edit is manual. Directory
trees stay as plain ` ```text ` blocks; Mermaid is the wrong shape for them.

**Docs.** State current behavior and intent. Prefer tables and lists wherever the
content enumerates; reserve prose for the reasoning that cannot be tabulated. No
narrative walkthroughs of how the design was reached. When a doc goes stale,
rewrite the claim — never append a correction to the end.
