# Threat review: Samurai Toolbox privilege boundary (2026-11-26)

Scope: `samurai-toolbox-gui` (`toolbox/gui/`). The CLI it calls has its own rules (spec 05 and
HANDOFF "Root hardening rules").

## Design
```
samurai-toolbox (user)  --read-->  /usr/local/bin/samurai ... --json          (no privilege)
                        --pkexec->  /usr/lib/samurai/gui-helper <verb> [args]  (root)
                                      └─ validate ─ execv /usr/local/bin/samurai <fixed argv>
```
- The GUI never runs as root (it refuses to start with uid 0).
- `gui-helper` is the only program the toolbox runs through pkexec. About 490 lines of C++ (`helper/` + `common/`), standard
  library only (no Qt), PIE + BIND_NOW from Arch's build flags.
- One table (`common/operations.cpp`) lists every operation: verb, argument validator, fixed argv.
- One polkit action per verb (`org.samurai.toolbox.<verb>`), selected by pkexec's
  `exec.path` + `exec.argv1` annotations ([pkexec(1)](https://polkit.pages.freedesktop.org/polkit/pkexec.1.html)).
  A first argument that matches no action falls back to `org.freedesktop.policykit.exec`
  (administrator password), and the helper then refuses it.

## Attacker
Any code already running as the desktop user (browser or game compromise), which may call pkexec
directly with any arguments, and untrusted text that reaches the screen (package metadata,
journal lines, snapshot labels, Arch news titles). Goal: root without the password, or a different
root action than the one the user approved.

## Threats and controls
| Threat | Control | Test |
|--------|---------|------|
| Run an arbitrary command or CLI sub-command as root | verbs are an allowlist; `rescue`, `--skip-interventions`, `--no-redact`, user caches and vendor scripts are unreachable | `test_everything_else_is_refused_before_the_cli_runs` |
| Shell injection | no shell anywhere; `execv` of an absolute path | `test_allowed_requests_become_fixed_argv` (shell metacharacters stay one argument) |
| Option injection through a value | every value is validated; none may start with `-`; `--` precedes app ids | same two tests, `test_cli_accepts_the_argument_vectors` |
| Environment injection (`PATH`, `LD_*`, `PYTHONPATH`, `BASH_ENV`, `SAMURAI_CATALOG_DIR`) | `clearenv`, fixed `PATH`/`LANG`/`HOME`; only a numeric `PKEXEC_UID` survives; cwd `/`, umask 022, stdin `/dev/null`, extra descriptors closed | `test_environment_is_rebuilt_and_cwd_is_root` |
| Root writes to a user-chosen path (symlink, overwrite) | no operation takes a path; the diagnostics bundle goes to stdout (`--out -`) and the **user's** process creates the file (new file only, 0600) | `diag-bundle` argv test |
| Silent privileged change | every mutating action is `auth_admin`; no `*_keep`, so one password never covers a second request; `allow_any`/`allow_inactive` = `no` (no SSH or background-session use) | `test_only_read_only_operations_skip_authentication` |
| Persistence through a kernel parameter | the helper refuses to **add** `init`, `rdinit`, `systemd.*`, `rd.*` (small harmless list excepted) and boot-mode words; the CLI refuses root/unlock parameters | refusal table |
| Rich-text injection / remote fetch from displayed text | dynamic text is `Text.PlainText` (or HTML-escaped for `InlineMessage`); the QML engine's network access manager rejects every request | smoke test feeds markup in `tests/gui/fake-samurai` |
| Toolbox crash or close kills pacman mid-transaction | the helper relays output, ignores SIGPIPE/HUP/TERM, puts the CLI in its own session and drains output nobody reads | `test_operation_finishes_when_the_reader_goes_away` |
| Helper hangs on daemons that keep the pipe | after the CLI exits only buffered output is read | `test_daemons_holding_the_pipes_do_not_hang_the_helper` |
| No trace of what ran | syslog (`authpriv`): caller uid, command, exit code; rejected argument text is not logged | checked in the VM test |

## Accepted risks
- **Reads without a password** (`health-read`, `kernel-list`, `boot-get`, `snap-list`), as spec 09
  asks for read-only actions. Any process in the active local session can learn the kernel command
  line, snapshot descriptions and root-only health findings. None takes an argument. They do run
  CLI code as root without a prompt, so a bug in those read paths would be reachable; drop them to
  `auth_admin_keep` if that trade is unwanted.
- **The password prompt shows what pkexec shows**: the action message and program. A malicious
  process can raise the same prompt as the toolbox. The message names the operation, not the
  argument (which app, which snapshot).
- `apps-install` installs anything in the signed catalog; `update` runs pacman with `--noconfirm`
  (default answers). Both need the password, and the toolbox shows the plan first. The plan is
  computed again at install time, so it can differ if repositories changed in between.
- `samurai` and its executors live in `/usr/local/bin` (root-owned). Anyone who can write there is
  already root.

## Deferred (the D-Bus helper, ADR-0004 / spec 01)
Not built: `samurai-helper` on D-Bus (`org.samurai.Helper1`) with a typed method per action,
polkit checks with per-request details (so the prompt can name the app or snapshot), a task queue
shared with the CLI, progress signals, journald task logs, cancellation of safe steps, and
surviving a logout as a system unit. The operation table maps one to one onto those methods.
Until then: one task at a time per toolbox window (pacman's own lock covers the rest).
