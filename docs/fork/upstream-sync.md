# Taking upstream's commits into oos-plus

oos-plus follows the main branch of [OOS](https://github.com/dekajoule/oos). This page is the procedure of one sync,
for a person and for the agent that runs it every day. Nothing here touches a scope. (The page
[sync-checklist.md](sync-checklist.md) next to it is upstream's, about another fork; it is not this procedure.)

## Why a plain merge does not work

A pull request lands in the fork's main squashed, because the rules of main want a linear history. After a sync git
therefore does not know that upstream's commits are in: a plain `git merge` of upstream's main counts every commit
since the fork began and conflicts in dozens of files that are already merged.

`tools/upstream-base.txt` holds the commit of upstream's main that the last sync reached. `tools/upstream-sync.sh
start` first merges that commit with the strategy `ours`, which changes no file and only records the ancestry on the
branch, and then merges upstream's main, which now brings the new commits alone. It writes the new base into the
file, so the sync that lands carries it.

## The steps

1. `git fetch origin`, and a branch `claude/upstream-sync-<date>` from `origin/main`. If a pull request of an earlier
   sync is still open, go on with that one; do not open a second.
2. `tools/upstream-sync.sh status`. With `behind 0` there is nothing to do.
3. `tools/upstream-sync.sh start`. It leaves the merge open, with or without conflicts.
4. Resolve the conflicts by the rules below, and read what git merged by itself in the files the next section names.
5. Add the section of this sync to `CHANGELOG.md`, above the section of the sync before it: `### From upstream OOS
   <old base>..<new base> (upstream #<first> to #<last>, merged <date>)`, written from upstream's new files in
   `changelog.d/`, which come along. Say there that nothing of the merge ran on a unit in oos-plus.
6. `git commit -s` (the merge commit; its message names the range, the conflicts and what was tested). Fixes that the
   checks ask for are commits of their own after it.
7. The checks, below. Then `tools/privacy-scan.py origin/main..HEAD`.
8. Push, and open the pull request against `wasimlhr/oos-plus` with the title `Upstream: merge dekajoule/oos up to
   <new base> (<date>)`. Its text lists upstream's pull requests by kind, every conflict with how it was resolved, what
   the merge needed beyond the conflicts, the hardware paths that changed, what was run and where, and `Behaviour:
   changes` with what changes.
9. It is merged squashed when its checks are green.

## Rules for the conflicts

| Where | Rule |
|---|---|
| `README.md` | The fork's README is its own: keep it. A line upstream changed in a block the fork does not have is dropped |
| a fix the fork made first and upstream took or reworked | upstream's version; the fork's copy goes, so nothing is done twice |
| menus, settings, keys of system.conf, lists of modules (`tools/layers.py`) | both: the fork's rows next to upstream's new ones |
| tests both sides added at the same place | both |
| `docs/adr/README.md` | both sides, then `tools/adr-index.py` writes the index. A number upstream took that a fork record has: both stay, and the fork's record is named in `FORK_RECORDS` of `tools/adr-index.py` |
| counts that only go down (`tools/ratchet.txt`, `tools/scpi-coverage.txt`) | what the merged tree counts (`tools/ratchet.sh`, `tools/scpi-coverage.py --enforce`); a number above both sides is said in the pull request |
| generated files (`docs/manual/commands.json`, the index above) | run their tool again, never merge them by hand |
| the hardware paths (`hal/src/dho/`, `hal/src/update/`, `ui/src/board/`, `fpgad/`, `boot/`, `kernel/`, `rootfs/`) | upstream's side where the fork did not change the lines. Where both changed the same lines, do not guess: see "When to stop" |

## What git merges by itself and still needs a look

- **The Windows desktop build.** Upstream is Unix-only (ADR 0033), oos-plus keeps a desktop build for Windows. New
  upstream code that uses `std::os::fd`, `std::os::unix` or constants of `libc` that Windows lacks goes behind
  `#[cfg(unix)]`, with a line that starts `// oos-plus:` and says why. `cargo check --all-targets` in `ui` on Windows
  finds every such place; CI does not, it builds on Linux.
- **A new row of a menu** gets its place on the sheet of Minimal and Studio (`sheet_plan` in `ui/src/ui/sheet.rs`,
  oos-plus ADR 0030). A tab holds seven rows; rows that fall on a full tab make the menu no sheet at all, and a walk
  test says so.
- **A new module** of `ui/src`, `ui/system/src` or `ui/remote/src` has its place in `tools/layers.py`.
- **A new download** of a build script has its line in `rootfs/deps.lock` (`tools/deps-check.sh`).

## The checks

- On Linux (WSL, or CI for the pull request): `tools/check.sh`, every enforced layer. `fmt` and `clippy` are not
  enforced in oos-plus, and `cargo fmt` is never run.
- On Windows: in `ui`, `cargo check --locked --all-targets` without a warning (the desktop build), and
  `cargo check --locked --all-targets --target aarch64-unknown-linux-musl` without a warning (what the scope gets).
- Not run: `tools/compare-base.sh`, because a sync changes behaviour.
- A check that fails is fixed in the code or the test it names. It is never made to pass by weakening it.

## When to stop

The sync is left open, as a draft pull request that says what is in the way, and the owner decides:

- both sides changed the same lines in a hardware path;
- a check fails for a reason that is more than a follow-up of the merge (a rule above, a number, a moved row);
- upstream's main no longer holds the base (`tools/upstream-sync.sh` says so): its history was rewritten;
- upstream removed or replaced something the fork builds on, so keeping the fork's side means a decision.

## The scope

A sync is tested on the host. Installing its build on a scope is not a step of it: a scope is written only when its
owner asked for it (AGENTS.md). The pull request names the hardware paths that changed, so the owner knows what the
first run on the unit is for.
