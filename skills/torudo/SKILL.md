---
name: torudo
description: Drive torudo (a todo.txt TUI) from natural language. Use whenever the user is capturing, organising, reviewing, or acting on their own todo list — in any wording or language, and whether or not they name torudo. That covers dropping something into the inbox (often a bare URL), showing or triaging a mode, tagging an item with time/energy or a priority, attaching a URL to an item that already exists, pushing it out to a later date or giving it a deadline, breaking one into a checklist, moving it between the GTD modes (inbox/todo/waiting/ref/someday), completing or reopening it, finding where an item lives and putting the running TUI's cursor on it, surveying what there is to pick up next, and starting work on an item in its own git worktree and Claude session.
---

# torudo

torudo keeps one todo.txt-format file per GTD mode under `~/todotxt/` (`inbox.txt`,
`todo.txt`, `waiting.txt`, `ref.txt`, `someday.txt`), plus per-item detail in
`todos/{id}.md`. The whole of it is reachable from the CLI:

```bash
# A mode is a place: put an item in it, or read what it holds.
torudo <mode> add <text>                              # append an item, print it as JSON
torudo <mode> list [--json] [--with-md]               # list the mode's items
torudo done list [--json] [--with-md]                 # what has been completed

# An item is a thing: its id says which one, so no mode is named.
torudo show <id>                                      # print it as JSON, detail md included
torudo set <id> [--time ...] [--energy ...] [--rec ...] [--priority ...] [--threshold ...] [--due ...]
torudo link <id> <url>... [--remove <url>]            # attach URLs to it
torudo complete <id>                                  # finish it: move to done.txt
torudo promote <id> --to <mode>                       # move it to another mode
torudo reopen <id> [--to <mode>]                      # pull it back out of done.txt (default: todo)
torudo focus <id>                                     # move the running TUI's cursor onto it

torudo search <query> [--json] [--with-md] [--mode <mode>] # find an item by title, tag or detail text
```

`<mode>` is one of `inbox` / `todo` / `waiting` / `ref` / `someday`. Everything works
whether or not the TUI is running — a running session picks the change up through its file
watcher. Pass `--todotxt-dir <path>` **before** the subcommand when the user works out of a
non-default directory.

An `id:` is unique across the files, so the commands above that take one find the item
themselves. Never run a `search` or `show` first just to work out which mode something is in
— the id has already answered that.

Never edit the `.txt` files by hand — always go through these commands. The detail md files
under `todos/` are the exception: their content is yours to author, so write them with
Write/Edit. torudo touches them only as a side effect — carrying one over when a `rec:` item
is completed, deleting it with its item — never as content. To find out what an item already
holds, read it back through the CLI: `torudo show <id>` for one item,
`<mode> list --json --with-md` for a whole mode.

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
  `torudo set <id> --priority` instead (see "Triaging" below).
- **`+project` / `@context`** — add only when the request makes it obvious (e.g. "torudoの
  リリース作業" → `+torudo`). When it is a guess, ask the user with AskUserQuestion instead
  of guessing; offering "タグなしで追加" as one of the options is usually right, since inbox
  items get sorted later anyway. Reuse existing tags — `torudo inbox list --json` and
  `torudo todo list --json` show what is already in use.
- **`t:` / `due:`** — only when the user gave a date. Convert relative dates ("明日", "来週
  月曜") to absolute `YYYY-MM-DD` using today's date. This is only for what to embed at capture
  time; to add, move or drop a date on an item that already exists, use
  `torudo set <id> --threshold` / `--due` instead (see "Triaging" below).
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
torudo inbox list --json             # structured: id, title, priority, projects, contexts, key_values
torudo todo list                     # one todo.txt line per item, for showing the user
torudo inbox list --json --with-md   # the same, plus each item's detail md in an `md` field
```

Use `--json` whenever you need the `id` of an existing item — every other command takes one.
A mode whose file does not exist yet lists as an empty array, which is normal.

`--with-md` folds each item's `todos/{id}.md` into the JSON as an `md` field, raw — frontmatter
and checkboxes included. It requires `--json` and is rejected at the argument boundary without
it. An item whose detail file is missing — or that has no `id:` — carries no `md` key at all
rather than a null one, so test for the key. Leave the flag off when an `id` is all you need: a
whole mode's worth of md bodies is a lot of output. For one item whose id you already have,
`torudo show <id>` is the cheaper read (see "Finding and focusing").

## Writing the detail md

Long-form detail lives in `$TODOTXT_DIR/todos/{id}.md` (default `~/todotxt/todos/{id}.md`).
The content is yours to author, so write these with Write/Edit — and to see what one already
holds, `torudo show <id>` reads it back without going near the path. Unchecked `- [ ]` lines
are what the TUI surfaces: the top 3 appear on the card, and the `{done}/{total}` badge counts
them.

```markdown
---
cwd: /home/maedana/src/github.com/maedana/torudo
branch: readme-dev-setup
plan: /home/maedana/.claude/plans/2026-08-20-readme-dev-setup.md
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
- **`plan` frontmatter** points at the plan file the worktree session confirmed for this
  item. That session writes the line itself once its plan is approved (see below), so — like
  `branch` — it is a record, never something to invent. Keep it when you rewrite the file:
  dropping it loses the only link back to the plan.
- **Preserve what is already there.** Append to the checklist rather than overwriting notes,
  and never flip `- [ ]` to `- [x]` unless the user says the step is done. Editing an existing
  file means opening it with Read first — Write and Edit both refuse one they have not read,
  and rebuilding it from what `show` printed is how notes get lost.
- **Record *what*, not *how*.** The checklist is the shape of the task and the background
  behind it, not its design. Working out an approach means reading the target repository, and
  that belongs to the worktree session — see "Starting work on a task".

## Triaging

```bash
torudo set i1 --time short --energy low  # time:short energy:low
torudo set t1 --rec 1w                   # rec:1w — recur weekly from completion
torudo set t1 --rec none                 # drop rec: — stop the item repeating
torudo set t1 --priority a               # (A) — bump it to top priority
torudo set t1 --priority none            # drop the priority prefix
torudo set t1 --threshold 2026-08-24     # t:2026-08-24 — hold it back until then
torudo set t1 --due 2026-08-31           # due:2026-08-31 — the deadline
torudo set t1 --threshold none           # drop t: — back in play right away
torudo promote i1 --to todo              # inbox.txt → todo.txt, source mode found for you
```

- `--time` is `short` / `medium` / `long`, `--energy` is `low` / `high`, `--rec` is a pattern
  like `1w` or `+1m` (see "Capturing" above for what `rec:` means), `--priority` is `a` through
  `e` (uppercase works too), and `--threshold` / `--due` take an absolute date, `YYYY-MM-DD` and
  nothing else. An existing value is replaced. Any subset of the six flags may be given at once;
  `--rec` with an unparseable pattern (e.g. a typo like `1z`) fails loudly and writes nothing,
  and so does a date that is not `YYYY-MM-DD` (e.g. "来週月曜") — resolve it yourself and retype
  it rather than guessing.
- `--rec none`, `--priority none`, `--threshold none` and `--due none` remove the `rec:` tag,
  the `(A)`–`(E)` prefix, the `t:` tag and the `due:` tag respectively — each is the only way to
  clear its own. The keyword is case-insensitive on all four, so `NONE` and `None` work as well
  as `none`. All four are fine to run on an item that has nothing to clear — nothing changes
  and it still succeeds. `--time` and `--energy` take no `none` yet, so a stale `time:` or
  `energy:` can only be overwritten, not cleared.
- `(A)` surfaces in its own column in the TUI's Pick view, so reserve it for things the user
  really means to do today rather than everything they call "priority".
- Postponing something is `--threshold`, not a move to another mode. "来週月曜まで寝かせて",
  "今日はやらない" and "月末まで置いといて" all point at `t:`, and a single
  `torudo set <id> --threshold <YYYY-MM-DD>` is the whole of it. Work the relative date
  out against today's date yourself — the CLI takes nothing but `YYYY-MM-DD`. Do not reach for
  `someday` here: that mode says "might never happen", which is not what "not yet" means. Say
  the date you arrived at back to the user ("8/24まで先送りしました"), so a bad conversion gets
  caught on the spot.
- `--due` is the deadline; `--threshold` is the day it becomes workable. "月末までにやらないと
  まずい" is `--due`, "来週までは触らない" is `--threshold`. An item with a future `t:` does not
  vanish from the list — it sinks to the bottom and renders dimmed in the TUI — so folding it
  away at review time is the survey's job, under the rule in "Reviewing what to do next".
- Set the tags **before** promoting, so the item lands in Todo ready to be picked. That is
  the usual flow: capture → write the checklist → set time/energy → promote to todo.
- Both commands need the item's `id` and fail loudly on an unknown one, but neither needs to
  know which mode holds it — that is what `set <id>` finds for itself. Take the `id` from
  `add`'s output, from `list --json`, or from `search`.
- Estimating time/energy is a judgment call about the user's own capacity — propose values
  with your reasoning and let the user correct them, unless they already said what they want.

## Attaching a URL to an item that already exists

```bash
torudo link t1 https://example.com/spec             # append one URL to the line
torudo link t1 https://a.example https://b.example  # append several at once
torudo link t1 --remove https://example.com/spec    # drop one from the line
torudo link t1 https://new --remove https://old     # swap one for the other in one write
```

- Only the **line** gets a link icon and opens with `o` in the TUI. A URL written into
  `todos/{id}.md` is a note and nothing more, so "このタスクにこのURL紐付けて" means `link`,
  never an edit to the detail md.
- URLs are matched as whole tokens, so `--remove` needs the URL exactly as it stands in the
  line. Read it back with `torudo show <id>` first rather than retyping it from memory.
- Appending a URL the item already carries changes nothing, and removing one it never had is
  not an error — both still succeed, so a repeated `link` is safe.
- `link` appends URLs and nothing else. Rewording an item is the user rewriting it, not a
  `link` call with prose in it.
- Attaching at capture time is different: build the line with the URL already in it, under
  "When the input is a URL" above. `link` is for the item that is already on file.

## Completing

```bash
torudo complete <id>      # → done.txt, stamped with today's date
torudo done list --json        # find what was completed, and its id
torudo reopen <id>        # done.txt → todo.txt, completion marker cleared
```

- Only **Todo** and **Waiting** can complete, matching the TUI's `x`. `complete <id>` finds
  the mode itself and names it when it refuses, so finishing something sitting in Inbox / Ref
  / Someday means reading that message and promoting it to Todo first.
- If the item carries `rec:`, completing it also appends a fresh next occurrence to the same
  file — nothing more to do. The JSON output's `recurrence` field carries the new item's `id`
  and dates; report that instead of `torudo <mode> add`-ing a replacement by hand.
- **Complete only what the user said is done.** `torudo show <id>` prints the item's detail
  md; when it still has unchecked `- [ ]` lines, say so and let them confirm rather than
  assuming the task is finished.
- `reopen` takes the item back out of `done.txt`, into Todo unless `--to` says otherwise —
  priority, creation date, and tags all survive the round trip, so a mistaken completion costs
  nothing. Nothing records which file the item left when it was completed, which is why the
  destination is named here and nowhere else. It reads ids from `done.txt`, so get them from
  `torudo done list --json`.
- `done` is an archive, not a GTD mode: it only lists. Items leave it through `reopen`.

## Finding and focusing

```bash
torudo search "release" --json           # search every mode's title, tags and detail md, case-insensitive substring
torudo search "release" --json --with-md # the same, plus the detail md body of every hit
torudo search "release" --mode todo      # narrow to one mode
torudo show <id>                         # print one item by id, from whatever mode holds it
torudo focus <id>                        # move the running TUI's cursor onto that item
```

- Use `search` when the user is not sure which mode holds something ("release作業のタスクどこだっけ").
  It never searches `done.txt`, so a completed item will not turn up — check `torudo done list` instead.
- A `title` hit means the query was in the item's title **or** in one of its `+project` / `@context` tags,
  so searching for a project name finds every item filed under it. `key:value` tags (`id:`, `time:`,
  `energy:`) are not searched — narrow by those with `--mode` plus `list --json` instead.
- `--with-md` hands back the detail md of every hit, so an `md` match no longer needs a second read
  to see what it matched on. It requires `--json`, and a hit with no detail file carries no `md` key.
- `show` is the id-shaped lookup: it sweeps every mode and then `done.txt`, and prints that one item
  as JSON with its detail md and a `mode` field naming where it turned up (`done` for something
  already completed). The md is always there — no flag to pass — and an unknown id fails loudly.
  Unlike `focus` it only reads files, so it needs no running TUI and honours `--todotxt-dir`.
- `focus` needs a running TUI; without one it fails loudly instead of doing nothing silently. Reach for it when
  the user wants the item on screen ("あれ画面に出して"), typically right after a `search` that resolved the id.
- Multiple hits are normal — prefer a `title` match over a `md` match, and ask the user when it is still
  ambiguous rather than guessing which one they meant.

## Reviewing what to do next

```bash
torudo inbox list --json --with-md
torudo todo list --json --with-md
torudo waiting list --json --with-md
```

**An ask with no item in it is a request to survey.** The user is not telling you to do something;
they are asking what there is, so they can decide later when they actually get a gap. The tell is the
missing object — they are pointing at the list, not at anything on it — and that holds in any
language. "次何やろう", "時間空いたら何やれるかな", "そろそろ棚卸ししたい" are the usual shapes.
Read the three modes, then report what you found.

**So this is not the trigger for "Starting work on a task".** That section needs an item already
picked out. Here there is none, so do not cut a worktree, and do not nudge the user toward one.

**Leave running work out of it.** Agents mid-task, sessions waiting on a plan, anything already
in flight — the user is tracking those. Reporting them back is noise.

Look for these five things, in this order:

- **Ball back in your court** — items in `waiting` whose condition has since been met. This is the
  most valuable thing to surface, because it is the easiest to miss: the item looks parked, but the
  thing it was parked on already happened. The `md` in the `--with-md` output says what it is
  waiting on — weigh that against what has come up since.
- **Undecided** — how long `inbox` items have been sitting. What is stuck there is a *decision*
  (todo? someday? drop it?), not work, so a long-stale inbox is cheap to clear.
- **Started, and not started** — split `todo` in two. An item is underway when the `md` from
  `--with-md` carries a `branch:` or already has some `- [x]` in its checklist; everything else has
  not been picked up yet. Do not editorialise about age here — just say which side each item is on.
- **Batchable** — items that share a project, a repository, or a kind of work and would be cheaper
  done together (two dependabot items, three small fixes in one repo).
- **By time and energy** — group what is left by `time:` and `energy:` so the user can match the
  list against whatever gap they get. "30分あるとき" and "半日とれるとき" are the useful buckets.

**Leave postponed items out.** An item whose `t:` is in the future has already been decided
on — the user parked it there deliberately, and listing it again every survey undoes that
decision. Drop those from every heading, count them, and close the survey with a single line:
`先送り中 3件（最短 8/22）`, where the date is the earliest `t:` among them. No titles, no
reasons, no heading of their own. An item whose `t:` has already passed is back in play and
gets surveyed normally — say `t:2026-07-25 超過` next to it as before.

**Put what has sat longest at the bottom of each heading.** Something untouched for months is, in
practice, not a priority — leading with it buries the things the user might actually pick up. Sort
by `creation_date`, newest first, and let the old ones fall to the end where they belong.

Report it as a survey and stop there. Do not rank the items, do not pick one, and do not ask
"which do you want to start?" — the user is deciding on their own schedule, and the answer may
well be "none of them today". If nothing turns up under a heading, drop the heading rather than
padding it.

### Number the items

Number them `1.`, `2.`, `3.` … straight through the whole survey, continuing across the headings
instead of restarting under each one. The number is the handle the user grabs: "1は完了で",
"3をtodoに上げて", "2と4は今日やる". Keep the number → id mapping for the rest of the conversation.

```
■ ボールが戻っているかも (waiting)
  1. Nodeのバージョン更新 [#123947] — 待っていた自動テストが8/17に落ちて報告済み

■ 判断待ち (inbox)
  2. スマホからtorudoへキャプチャ +torudo — 8/17から
  3. dependabot対応 +ga-pms — 8/13から5日
  4. dependabot対応 +aegis — 8/13から5日

■ 着手済み (todo)
  5. 脆弱性診断(API)の対応 +aegis — branch: remove-api-key-query-param、実装とテストまで済み
  6. メニューの見切れ +ga-pms — branch: fix-sidebar-menu-cutoff

■ 未着手 (todo)
  7. 登録番号のDRY化 +jfa-apps — short/low
  8. sentry移行 — short/low
  9. メンスケ機能 [#122088] +jfa-apps — long/high
 10. 保険検討 — short/low、5/22から
 11. 家族旅行検討 — long/high、4/19から、t:2026-07-25 超過

■ 空き時間別
  30分  : 7, 8
  半日  : 9

先送り中 3件（最短 8/22）

3 と 4 は同じ dependabot なのでまとめて処理できる。
```

- **One number per item.** When an item would fit under two headings, number it where it first
  appears and refer back to that number afterwards ("2 と 3 はまとめてやれる"), rather than handing
  the same item a second number further down.
- **The numbering lasts until the next survey.** Running a new one renumbers everything, so a number
  from an earlier survey no longer means what it did. If the user points at a stale number, say so
  and re-resolve it instead of acting on the old mapping.
- **Echo the title back before acting.** "1は完了で" is a one-word instruction aimed at a position,
  so name what you are about to touch — `完了にしました: <title> (id: …)` — and pass the **id** to the
  command. Never re-derive the item from its position in a list you rebuild later.

## Starting work on a task

A development task gets its own git worktree, its own terminal tab, and its own Claude
session. This session is the commander: it sets that up and hands the task over, it does not
do the work itself.

**This section fires the moment the user asks to move a particular item forward** — however they
phrase it, in any language. What makes it this request is that an item is named and they are
pointing at it; the verb they reach for barely matters, and it is usually nowhere near "着手して".
"実績出力の件進めたい", "〜やろうか", "次これやる" are all the same ask. Route first; do not start
digging here and cut the worktree later.

An ask with no item in it is the other thing entirely — nothing has been chosen, so it belongs to
"Reviewing what to do next" instead.

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
# 0. Reuse the branch the item's md records (torudo show <id>); otherwise name it yourself
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

- **Repository** — take it from the item's `cwd` frontmatter (`torudo show <id>` prints it)
  when it has one; ask otherwise.
- **Branch name** — name it yourself, do not ask. Slugify the task into three or four
  lowercase words in kebab-case, in English even when the task is not ("READMEに開発環境
  セットアップ手順を追記する" → `readme-dev-setup`). Then **write it into the item's md as
  `branch:` frontmatter** once the worktree exists. `git wt <branch> <base-ref>` creates the
  branch, so the name you pick becomes a real branch — check it is not already taken.
- **A recorded `branch:` wins.** When `torudo show <id>` shows the md already has one, reuse
  it instead of naming a new one: `git wt <branch>` alone switches to the existing worktree, so
  resuming an interrupted task lands back where it was. Only fall back to naming when the
  frontmatter has none.
- **`WT_FAIL` means stop.** A repo configured with `wt.hook` (`bundle install`, `pnpm
  install`) can fail after the worktree directory already exists. **Do not start Claude** in
  that case — a worktree without its dependencies is worse than no worktree at all. Report
  what failed and let the user decide.
- **The peer name is not the name passed to `agent start`.** It is derived from the worktree
  directory name plus a random suffix (`wt-probe` → `wt-probe-df`), so it cannot be predicted.
  Call ListAgents after starting and match on the directory-name prefix.
- **Plan first.** The session starts in plan mode, so the handover message asks for a plan,
  not for the work: describe the task, point at the relevant files if you know them, and ask
  for a plan. Include the item's detail md (`torudo show <id>` prints it) when it has one — the
  `- [ ]` checklist is the shape of the task as the user sees it.
- **The plan is the user's to approve.** Tell them it is ready and stop there. Do not approve
  it from this session, and do not drive the child's UI with `agent send-keys` to get past
  the prompt — that turns a deliberate checkpoint into a rubber stamp.
- **Ask it to record its plan.** The handover message is also where you tell the spawned
  session to write the path of its confirmed plan into the item's md as `plan:` frontmatter.
  Claude Code names a plan file after the prompt that opened it, so without that line nothing
  ties the plan back to the task. Give the md's full path in the message — the session is
  working in the worktree and has no reason to know where `todos/{id}.md` lives.
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
| これのやることリストを書いて | `torudo inbox list --json` で id を得る → `torudo show <id>` で既存mdを確認 → `todos/{id}.md` に `- [ ]` を書く |
| 短時間・低エネルギーにしてtodoに移して | `torudo set <id> --time short --energy low` → `torudo promote <id> --to todo` |
| ゴミ出しは毎週繰り返しにして | `torudo set <id> --rec 1w` |
| もう繰り返さなくていい | `torudo set <id> --rec none` |
| これ最優先にして | `torudo set <id> --priority a` |
| 優先度下げていい | `torudo set <id> --priority c` |
| 優先度外して | `torudo set <id> --priority none` |
| 来週月曜まで先送りして | 今日の日付から日付を算出 → `torudo set <id> --threshold 2026-08-24` |
| 今日はやらない / しばらく寝かせたい | 同上。何日まで寝かせるか決まらなければ確認してから `--threshold` |
| 月末までにやらないとまずい | `torudo set <id> --due 2026-08-31` |
| もう着手できる / 先送りやめる | `torudo set <id> --threshold none` |
| このタスクにこのURL紐付けて | `torudo search` などで id を得る → `torudo link <id> https://…` |
| さっきのissueのリンクも足しといて | `torudo link <id> https://github.com/owner/repo/issues/12` |
| このURLもう関係ないから外して | `torudo show <id>` で正確なURLを確認 → `torudo link <id> --remove https://…` |
| これはいつかやるやつ | `torudo promote <id> --to someday` |
| Astro7対応終わった | `torudo complete <id>` |
| さっき完了にしたのやっぱ戻して | `torudo done list --json` で id を得る → `torudo reopen <id>` |
| 今週何が終わった? | `torudo done list` |
| Astro7対応に着手して | `git wt --nocd` でworktree作成 → `herdr tab create` → `herdr agent start` → ListAgentsで名前解決 → SendMessageで依頼 |
| worktree切ってclaude立てて | 同上。ブランチ名とベースブランチを確認してから |
| 実績出力の件進めたい | 同上。「進めたい」「やろうか」も着手の依頼。ここで調べ始めない |
| 本番でスクリプト流したい | 同上。コード変更を伴わない作業もworktreeとセッションを切る |
| 次何やろう / 時間空いたら何やれる? | inbox/todo/waiting を `list --json --with-md` で読んで棚卸しする。着手依頼ではないのでworktreeは切らない |
| 経費精算のタスクどこだっけ | `torudo search "経費精算"` |
| このタスクの詳細見せて | `torudo show <id>`（モードを知らなくても引ける。完了済みも出る） |
| あれ画面に出して | `torudo focus <id>`（直前の `search` で得た id） |
