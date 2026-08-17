---
name: torudo
description: Drive torudo (a todo.txt TUI) from natural language — capture into the inbox, break an item down into a checklist, tag it with time/energy, move it between GTD modes, complete it, find it by keyword and jump the running TUI's cursor to it, and start work on a task in its own git worktree and Claude session. Use when the user says things like "inboxに〜を追加して", "inboxに<URL>を追加して", "inboxを見せて", "これをtodoに移して", "エネルギー低めで短時間にして", "これ最優先にして", "やることリストを書いて", "これ着手して", "〜の件進めたい", "worktree切ってclaude立てて", "終わったから完了にして", "〜のタスクどこだっけ", "あれ画面に出して", or "add this to my inbox".
---

# torudo

torudo keeps one todo.txt-format file per GTD mode under `~/todotxt/` (`inbox.txt`,
`todo.txt`, `waiting.txt`, `ref.txt`, `someday.txt`), plus per-item detail in
`todos/{id}.md`. Every mode file is reachable from the CLI:

```bash
torudo <mode> add <text>                             # append an item, print it as JSON
torudo <mode> list [--json]                          # list the mode's items
torudo <mode> set <id> [--time ...] [--energy ...] [--rec ...] [--priority ...]  # set GTD tags / recurrence / priority
torudo <mode> promote <id> --to <mode>               # move to another mode
torudo <mode> complete <id>                          # finish it: move to done.txt
torudo <mode> reopen <id>                            # pull it back out of done.txt
torudo done list [--json]                            # what has been completed
torudo search <query> [--json] [--mode <mode>]       # find an item by title, tag or detail text, across every mode
torudo focus <id>                                    # move the running TUI's cursor onto an item
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
- **Priority** — `(A)` through `(E)` only when the user explicitly signals urgency
  ("最優先で", "急ぎ", "(A)で"). Never invent one. This is only for what to embed at capture
  time; to change or clear the priority on an item that already exists, use
  `torudo <mode> set <id> --priority` instead (see "Triaging" below).
- **`+project` / `@context`** — add only when the request makes it obvious (e.g. "torudoの
  リリース作業" → `+torudo`). When it is a guess, ask the user with AskUserQuestion instead
  of guessing; offering "タグなしで追加" as one of the options is usually right, since inbox
  items get sorted later anyway. Reuse existing tags — `torudo inbox list --json` and
  `torudo todo list --json` show what is already in use.
- **`t:` / `due:`** — only when the user gave a date. Convert relative dates ("明日", "来週
  月曜") to absolute `YYYY-MM-DD` using today's date.
- **`rec:<count><unit>`** — only when the user says the task repeats ("毎週", "毎月",
  "定期で"). Unit is `d` / `w` / `m` / `y`. No `+` (e.g. `rec:1w`) counts from the day it gets
  completed — use it for chores like taking out the trash. A leading `+` (e.g. `rec:+1m`)
  counts from the item's own `due:` date instead, keeping a fixed schedule even if completed
  late — use it for things like rent. A `+` pattern that is months overdue is advanced
  repeatedly until it clears the completion date, so the next occurrence is always in the
  future. Completing a `rec:` item creates its next occurrence automatically, so never
  manually recreate a recurring item after completing it.
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
- **Record *what*, not *how*.** The checklist is the shape of the task and the background
  behind it, not its design. Working out an approach means reading the target repository, and
  that belongs to the worktree session — see "Starting work on a task".

## Triaging

```bash
torudo inbox set i1 --time short --energy low   # time:short energy:low
torudo todo set t1 --rec 1w                     # rec:1w — recur weekly from completion
torudo todo set t1 --rec none                   # drop rec: — stop the item repeating
torudo todo set t1 --priority a                 # (A) — bump it to top priority
torudo todo set t1 --priority none              # drop the priority prefix
torudo inbox promote i1 --to todo               # move inbox.txt → todo.txt
```

- `--time` is `short` / `medium` / `long`, `--energy` is `low` / `high`, `--rec` is a pattern
  like `1w` or `+1m` (see "Capturing" above for what `rec:` means), and `--priority` is `a`
  through `e` (uppercase works too). An existing value is replaced. Any subset of the four
  flags may be given at once; `--rec` with an unparseable pattern (e.g. a typo like `1z`) fails
  loudly and writes nothing, so retype it rather than guessing.
- `--rec none` and `--priority none` remove the `rec:` tag and the `(A)`–`(E)` prefix
  respectively — the only way to clear either. Both are fine to run on an item that has
  nothing to clear — nothing changes and it still succeeds. `--time` and `--energy` take no
  `none` yet, so a stale `time:` or `energy:` can only be overwritten, not cleared.
- `(A)` surfaces in its own column in the TUI's Pick view, so reserve it for things the user
  really means to do today rather than everything they call "priority".
- Set the tags **before** promoting, so the item lands in Todo ready to be picked. That is
  the usual flow: capture → write the checklist → set time/energy → promote to todo.
- Both commands need the item's `id` and fail loudly on an unknown one. Take the `id` from
  `add`'s output or from `list --json`.
- Estimating time/energy is a judgment call about the user's own capacity — propose values
  with your reasoning and let the user correct them, unless they already said what they want.

## Completing

```bash
torudo todo complete <id>      # → done.txt, stamped with today's date
torudo done list --json        # find what was completed, and its id
torudo todo reopen <id>        # done.txt → todo.txt, completion marker cleared
```

- Only **Todo** and **Waiting** can complete, matching the TUI's `x`. Finishing something
  sitting in Inbox / Ref / Someday means promoting it to Todo first.
- If the item carries `rec:`, completing it also appends a fresh next occurrence to the same
  file — nothing more to do. The JSON output's `recurrence` field carries the new item's `id`
  and dates; report that instead of `torudo <mode> add`-ing a replacement by hand.
- **Complete only what the user said is done.** When the detail md still has unchecked
  `- [ ]` lines, say so and let them confirm rather than assuming the task is finished.
- `reopen` takes the item back out of `done.txt` and into the mode you name — priority,
  creation date, and tags all survive the round trip, so a mistaken completion costs nothing.
  It reads ids from `done.txt`, so get them from `torudo done list --json`.
- `done` is an archive, not a GTD mode: it only lists. Items leave it through `reopen`.

## Finding and focusing

```bash
torudo search "release" --json          # search every mode's title, tags and detail md, case-insensitive substring
torudo search "release" --mode todo     # narrow to one mode
torudo focus <id>                       # move the running TUI's cursor onto that item
```

- Use `search` when the user is not sure which mode holds something ("release作業のタスクどこだっけ").
  It never searches `done.txt`, so a completed item will not turn up — check `torudo done list` instead.
- A `title` hit means the query was in the item's title **or** in one of its `+project` / `@context` tags,
  so searching for a project name finds every item filed under it. `key:value` tags (`id:`, `time:`,
  `energy:`) are not searched — narrow by those with `--mode` plus `list --json` instead.
- `focus` needs a running TUI; without one it fails loudly instead of doing nothing silently. Reach for it when
  the user wants the item on screen ("あれ画面に出して"), typically right after a `search` that resolved the id.
- Multiple hits are normal — prefer a `title` match over a `md` match, and ask the user when it is still
  ambiguous rather than guessing which one they meant.

## Starting work on a task

A development task gets its own git worktree, its own terminal tab, and its own Claude
session. This session is the commander: it sets that up and hands the task over, it does not
do the work itself.

**This section fires the moment the user asks to move an item forward** — however they phrase
it. "Astro7対応に着手して" and "worktree切ってclaude立てて" are the obvious ones, but "実績出力の
件進めたい", "〜やろうか", "次これやる" are the same request. Route first; do not start digging
here and cut the worktree later.

The trigger is the item, not the kind of change. A task that touches no code — a production
operation, a data fix, an investigation that needs the repository's history — still gets its
own worktree and session. Anything that needs the target repo's context goes through here.

**What this session may do, and may not:**

- **May** — read the item's source of truth (the board entry, the issue, the ticket) and
  summarise it into `todos/{id}.md`. Writing a useful checklist needs that much.
- **May not** — read the target repository's code, settle on an implementation approach, or
  write code or scripts. Those are the worktree session's job. The md records *what* to do
  and why; it does not record *how*.

That boundary is easy to slide past, because investigating for the md is legitimately this
session's work and designing feels like a continuation of it. **When a question can only be
answered by reading the target repo, that is the signal to stop and cut the worktree** — not
a reason to open one more file. Being told you may act now ("exited plan mode", an approval,
a go-ahead) says nothing about *where* to act; it is never a reason to skip this section.

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

# 2. Open a tab on the repository, in the workspace this session is running in.
#    The JSON reply carries the pane_id
herdr tab create --cwd <repo> --label "<repo>/<branch>" --no-focus --workspace <workspace_id>

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

- **Workspace** — `--workspace` is what keeps the new tab next to the session that asked for
  it; without it herdr drops the tab in the default workspace, away from the user's work.
  Take the id from this session's `HERDR_WORKSPACE_ID` environment variable and substitute it
  literally (`--workspace w1`), rather than letting the shell expand `$HERDR_WORKSPACE_ID`
  inside the command — `${HERDR_WORKSPACE_ID:+--workspace "$HERDR_WORKSPACE_ID"}` is passed as
  a single argument under zsh and herdr rejects it. If the variable is unset, this session is
  not running inside herdr: drop the flag.

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
| ゴミ出しは毎週繰り返しにして | `torudo todo set <id> --rec 1w` |
| もう繰り返さなくていい | `torudo todo set <id> --rec none` |
| これ最優先にして | `torudo todo set <id> --priority a` |
| 優先度下げていい | `torudo todo set <id> --priority c` |
| 優先度外して | `torudo todo set <id> --priority none` |
| これはいつかやるやつ | `torudo inbox promote <id> --to someday` |
| Astro7対応終わった | `torudo todo complete <id>` |
| さっき完了にしたのやっぱ戻して | `torudo done list --json` で id を得る → `torudo todo reopen <id>` |
| 今週何が終わった? | `torudo done list` |
| Astro7対応に着手して | `git wt --nocd` でworktree作成 → `herdr tab create` → `herdr agent start` → ListAgentsで名前解決 → SendMessageで依頼 |
| worktree切ってclaude立てて | 同上。ブランチ名とベースブランチを確認してから |
| 実績出力の件進めたい | 同上。「進めたい」「やろうか」も着手の依頼。ここで調べ始めない |
| 本番でスクリプト流したい | 同上。コード変更を伴わない作業もworktreeとセッションを切る |
| 経費精算のタスクどこだっけ | `torudo search "経費精算"` |
| あれ画面に出して | `torudo focus <id>`（直前の `search` で得た id） |
