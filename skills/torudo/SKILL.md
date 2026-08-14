---
name: torudo
description: Drive torudo (a todo.txt TUI) from natural language — capture into the inbox, break an item down into a checklist, tag it with time/energy, and move it between GTD modes. Use when the user says things like "inboxに〜を追加して", "inboxに<URL>を追加して", "inboxを見せて", "これをtodoに移して", "エネルギー低めで短時間にして", "やることリストを書いて", or "add this to my inbox".
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
---
# やること

- [ ] CLIにlistサブコマンドを足す
- [ ] SKILL.mdに手順を追記
- [ ] CHANGELOGを更新
```

- **Break the task into checkable steps**, one per `- [ ]` line, ordered so the first three
  are the ones to start with — those are what shows on the card.
- **`cwd` frontmatter** is required for torudo's `clp` / `cli` claude launch. Set it when
  the item belongs to a repository you can identify; omit it otherwise.
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
