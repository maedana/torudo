---
name: torudo
description: Drive torudo (a todo.txt TUI) from natural language — capture into the inbox, break an item down into a checklist, tag it with time/energy, move it between GTD modes, and start work on a task in its own git worktree and Claude session. Use when the user says things like "inboxに〜を追加して", "inboxに<URL>を追加して", "inboxを見せて", "これをtodoに移して", "エネルギー低めで短時間にして", "やることリストを書いて", "これ着手して", "worktree切ってclaude立てて", or "add this to my inbox".
---

# torudo

torudo keeps one todo.txt-format file per GTD mode under `~/todotxt/` (`inbox.txt`,
`todo.txt`, `waiting.txt`, `ref.txt`, `someday.txt`), plus per-item detail in
`todos/{id}.md`. Every mode file is reachable from the CLI:

```bash
torudo <mode> add <text>                             # append an item, print it as JSON
torudo <mode> list [--json]                          # list the mode's items
torudo <mode> set <id> [--time ...] [--energy ...]   # set GTD tags
torudo <mode> promote <id> --to <mode>               # move to another mode
```

`<mode>` is one of `inbox` / `todo` / `waiting` / `ref` / `someday`. Everything works
whether or not the TUI is running — a running session picks the change up through its file
watcher. Pass `--todotxt-dir <path>` **before** the mode when the user works out of a
non-default directory.

Never edit the `.txt` files by hand — always go through these commands. The detail md files
under `todos/` are the exception: torudo only reads those, so write them directly.

## Capturing

```bash
torudo inbox add "(A) Buy milk +grocery @home"
```

The text follows the todo.txt format, assembled in this order:

```
(A) <summary> +project @context key:value
```

- **Summary** — short, in the language the user used. No trailing period.
- **Priority** — `(A)` / `(B)` / `(C)` only when the user explicitly signals urgency
  ("最優先で", "急ぎ", "(A)で"). Never invent one.
- **`+project` / `@context`** — add only when the request makes it obvious (e.g. "torudoの
  リリース作業" → `+torudo`). When it is a guess, ask the user with AskUserQuestion instead
  of guessing; offering "タグなしで追加" as one of the options is usually right, since inbox
  items get sorted later anyway. Reuse existing tags — `torudo inbox list --json` and
  `torudo todo list --json` show what is already in use.
- **`t:` / `due:`** — only when the user gave a date. Convert relative dates ("明日", "来週
  月曜") to absolute `YYYY-MM-DD` using today's date.
- **Never add** a creation date or `id:` — torudo fills both in automatically.

The command prints the added item as JSON; keep the `id` from it for any follow-up.

### When the input is a URL

A bare URL is a useless inbox item, so resolve a title first and add it as
`<title> <URL>` — keeping the URL in the line lets torudo open it later with `o`.

**Pick the most specific tool available, in this order:**

1. **A dedicated CLI or MCP tool for that host.** These see content behind login that
   WebFetch cannot — e.g. `github.com/<owner>/<repo>/(issues|pull)/<n>` →
   `gh issue view` / `gh pr view`. When the host looks like an internal service, check the
   available skills and MCP tools for one that covers it before giving up on step 1.
2. **WebFetch** for ordinary public pages. Ask it for the page title.
3. **The URL itself** when both fail (private page, network error, auth required). Derive a
   provisional title from the host and path, and say plainly that the title is a guess.

Trim the resolved title: drop site-name suffixes (` | GitHub`, ` - Qiita`), collapse
whitespace, and keep it to roughly 60 characters. If the user's request carried its own
intent ("この記事あとで読む"), prefer their wording over the raw title.

**Always show the assembled line and get approval before running the command for URL
inputs** — the title was chosen by you, not by the user. For non-URL inputs, add it
immediately without asking.

## Listing

```bash
torudo inbox list --json     # structured: id, title, priority, projects, contexts, key_values
torudo todo list             # one todo.txt line per item, for showing the user
```

Use `--json` whenever you need the `id` of an existing item — every other command takes one.
A mode whose file does not exist yet lists as an empty array, which is normal.

## Writing the detail md

Long-form detail lives in `$TODOTXT_DIR/todos/{id}.md` (default `~/todotxt/todos/{id}.md`).
torudo only reads these files, so write them with Write/Edit. Unchecked `- [ ]` lines are
what the TUI surfaces: the top 3 appear on the card, and the `{done}/{total}` badge counts
them.

```markdown
---
cwd: /home/maedana/src/github.com/maedana/torudo
branch: readme-dev-setup
---
# やること

- [ ] CLIにlistサブコマンドを足す
- [ ] SKILL.mdに手順を追記
- [ ] CHANGELOGを更新
```

- **Break the task into checkable steps**, one per `- [ ]` line, ordered so the first three
  are the ones to start with — those are what shows on the card.
- **`cwd` frontmatter** names the repository the item belongs to. Set it when you can
  identify one; omit it otherwise.
- **`branch` frontmatter** is written when work starts (see below) and ties the item to its
  worktree. Treat it as a record, not something to invent up front — and never change it on
  an item that already has one, or the task loses track of its own worktree.
- **Preserve what is already there.** Read the file first when it exists; append to the
  checklist rather than overwriting notes, and never flip `- [ ]` to `- [x]` unless the user
  says the step is done.

## Triaging

```bash
torudo inbox set i1 --time short --energy low   # time:short energy:low
torudo inbox promote i1 --to todo               # move inbox.txt → todo.txt
```

- `--time` is `short` / `medium` / `long`, `--energy` is `low` / `high`. An existing value is
  replaced. Either flag may be given alone.
- Set the tags **before** promoting, so the item lands in Todo ready to be picked. That is
  the usual flow: capture → write the checklist → set time/energy → promote to todo.
- Both commands need the item's `id` and fail loudly on an unknown one. Take the `id` from
  `add`'s output or from `list --json`.
- Estimating time/energy is a judgment call about the user's own capacity — propose values
  with your reasoning and let the user correct them, unless they already said what they want.

## Starting work on a task

A development task gets its own git worktree, its own terminal tab, and its own Claude
session. This session is the commander: it sets that up and hands the task over, it does not
do the work itself.

This flow leans on two tools besides torudo — [git-wt](https://github.com/k1LoW/git-wt) for
the worktree and [herdr](https://herdr.dev) for the tab. Check for both before starting, so a
missing one is reported as itself rather than as a confusing failure later:

```bash
command -v git-wt herdr
```

- **No `git-wt`** — fall back to plain git, choosing the path yourself:
  `git -C <repo> worktree add -b <branch> <repo>/.worktrees/<branch> <base-ref>`. Tell the
  user, because git-wt's `wt.copy` / `wt.hook` conveniences (copying `.env` and credentials,
  running `bundle install`) do not happen — the worktree will need setting up by hand.
- **No `herdr`** — this flow does not apply. Print the commands for the user to run in their
  own terminal rather than substituting another multiplexer.

```bash
# 0. Reuse the branch recorded in todos/{id}.md if there is one; otherwise name it yourself
# 1. Base ref — the repo's default branch unless the user says otherwise
git -C <repo> symbolic-ref --short refs/remotes/origin/HEAD        # → origin/main

# 2. Open a tab on the repository. The JSON reply carries the pane_id
herdr tab create --cwd <repo> --label "<repo>/<branch>" --no-focus

# 3. Create the worktree in that pane, marking the outcome so it can be matched on.
#    The shell integration cd's the pane into the new worktree.
herdr pane run <pane_id> "git wt <branch> <base-ref> && echo WT_OK || echo WT_FAIL"

# 4. Wait for it — wt.hook repos run bundle/pnpm install here and take minutes
herdr pane wait-output <pane_id> --regex "WT_(OK|FAIL)" --timeout 600000

# 5. Confirm the pane actually landed in the worktree before going on
herdr pane get <pane_id>        # foreground_cwd should be the worktree path

# 5b. Record the branch in the item's md, so the task and the worktree stay tied together

# 6. Start Claude in that pane, in plan mode. Returns once it is ready for input
herdr agent start <name> --kind claude --pane <pane_id> -- --permission-mode plan

# 7. Resolve the peer name with ListAgents, then hand the task over with SendMessage
```

The worktree is created **in the pane**, not from here, so the user can watch `bundle
install` scroll by instead of staring at a silent terminal. The `WT_OK` / `WT_FAIL` marker is
what makes that safe — without it the only signal is screen text, which is easy to misread.

- **Repository** — take it from the item's `cwd` frontmatter when it has one; ask otherwise.
- **Branch name** — name it yourself, do not ask. Slugify the task into three or four
  lowercase words in kebab-case, in English even when the task is not ("READMEに開発環境
  セットアップ手順を追記する" → `readme-dev-setup`). Then **write it into the item's md as
  `branch:` frontmatter** once the worktree exists. `git wt <branch> <base-ref>` creates the
  branch, so the name you pick becomes a real branch — check it is not already taken.
- **A recorded `branch:` wins.** When the item's md already has one, reuse it instead of
  naming a new one: `git wt <branch>` alone switches to the existing worktree, so resuming an
  interrupted task lands back where it was. Only fall back to naming when the frontmatter has
  none.
- **`WT_FAIL` means stop.** A repo configured with `wt.hook` (`bundle install`, `pnpm
  install`) can fail after the worktree directory already exists. **Do not start Claude** in
  that case — a worktree without its dependencies is worse than no worktree at all. Report
  what failed and let the user decide.
- **The peer name is not the name passed to `agent start`.** It is derived from the worktree
  directory name plus a random suffix (`wt-probe` → `wt-probe-df`), so it cannot be predicted.
  Call ListAgents after starting and match on the directory-name prefix.
- **Plan first.** The session starts in plan mode, so the handover message asks for a plan,
  not for the work: describe the task, point at the relevant files if you know them, and ask
  for a plan. Include the item's detail md (`todos/{id}.md`) content when it has one — the
  `- [ ]` checklist is the shape of the task as the user sees it.
- **The plan is the user's to approve.** Tell them it is ready and stop there. Do not approve
  it from this session, and do not drive the child's UI with `agent send-keys` to get past
  the prompt — that turns a deliberate checkpoint into a rubber stamp.
- **Handing over** — send the task with SendMessage. A spawned session answers on its own
  screen, not back to you, unless you explicitly ask it to reply with SendMessage.
- **Waiting for the plan** — `herdr agent wait <name> --until blocked --until idle`. Which of
  the two a pending plan approval reports as has not been verified, so match on both and
  confirm with `herdr agent read <name>`.
- **Watching it** — `herdr agent wait <name> --until idle` blocks until it settles, and
  `herdr agent read <name>` dumps its screen. Neither needs the peer to cooperate.
- **Permission boundaries** — never hand a spawned session work that was blocked in this
  session. Route that back to the user instead.

Report the worktree path, the branch, and the peer name so the user can jump to the tab.

## After acting

Report the resulting line and the `id`, e.g.

```
追加しました: Buy milk +grocery @home (id: 4f0c…)
todoに移しました: リリース作業 +torudo time:short energy:low (id: 8a21…)
```

## Examples

| 依頼 | 実行するコマンド |
| --- | --- |
| inboxに牛乳を買うを追加して | `torudo inbox add "牛乳を買う"` |
| inboxに(A)でリリース作業を追加して | `torudo inbox add "(A) リリース作業"` |
| inboxにhttps://example.com/blog/rust-tui を追加して | タイトル解決 → 確認 → `torudo inbox add "Rust TUI入門 https://example.com/blog/rust-tui"` |
| inboxにhttps://github.com/owner/repo/issues/12 を追加して | `gh issue view 12 -R owner/repo` でタイトル取得 → 確認 → `torudo inbox add "<issueタイトル> https://github.com/owner/repo/issues/12"` |
| inboxの中身を見せて | `torudo inbox list` |
| これのやることリストを書いて | `torudo inbox list --json` で id を得る → `todos/{id}.md` に `- [ ]` を書く |
| 短時間・低エネルギーにしてtodoに移して | `torudo inbox set <id> --time short --energy low` → `torudo inbox promote <id> --to todo` |
| これはいつかやるやつ | `torudo inbox promote <id> --to someday` |
| Astro7対応に着手して | `git wt --nocd` でworktree作成 → `herdr tab create` → `herdr agent start` → ListAgentsで名前解決 → SendMessageで依頼 |
| worktree切ってclaude立てて | 同上。ブランチ名とベースブランチを確認してから |
