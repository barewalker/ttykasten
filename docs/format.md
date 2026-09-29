# ttykasten: on-disk format specification

This document specifies the note collection that the Neovim plugin
**fzfkasten.nvim** reads and writes. ttykasten, a standalone Rust CLI, must
read and write the same collection **byte-compatibly**, so that both tools can
be used on it side by side.

It describes the plugin's defaults together with an **example
configuration**, a typical setup of the kind ttykasten is tested with (§1).
Where the example overrides a plugin default, both values are given. The plugin
itself is the reference. When this document and the code disagree, the code
is right and this document has a bug.

## 0. Conventions

### 0.1. Citations

Citations are `file:line` into the fzfkasten.nvim repository
(<https://github.com/barewalker/fzfkasten.nvim>), with paths relative to its
root. Unqualified names are files under `lua/fzfkasten/`. The other sources
are:

| Short name | Path |
|---|---|
| `plugin:N` | `plugin/fzfkasten.lua` (the command definitions) |
| `tests/…:N` | `tests/` (the specs, which pin edge cases) |

### 0.2. Lua patterns, stated as rules

The plugin matches text with Lua patterns. Lua patterns work on **bytes**, not
characters, and their character classes are ASCII-only in the plugin's locale:

| Lua | Meaning here |
|---|---|
| `%s` | ASCII whitespace `[ \t\n\v\f\r]`. The ideographic space U+3000 is **not** whitespace. |
| `%w` | `[A-Za-z0-9]` |
| `%W` | Any byte that is not `%w`. This includes `-`, `_`, and every byte ≥ 0x80, so every byte of a Japanese character counts as `%W`. |
| `%u` | `[A-Z]` |
| `%d` | `[0-9]` |
| `%c` | A control byte, `[\x00-\x1f\x7f]` |
| `.-` | A lazy (shortest-match) `.*` |
| `%f[%W]` | A frontier: the previous byte is `%w` and the next byte is `%W` or end-of-string |

Any regex in this document written as `/…/` is a restatement for a Rust
`regex` implementation. It is byte-oriented, and ASCII classes are meant
unless stated otherwise.

### 0.3. Time, dates and locale

All dates and times are **local time**. They are produced by `os.date`
(C `strftime`).

The example configuration runs under `LANG=en_US.UTF-8`, so the names come
out in English:

- `%A` is `Tuesday`
- `%a` is `Tue`
- `%B` is `September`

`%d`, `%m`, `%H`, `%M` and `%V` are zero-padded to two digits. For example,
`%A, %B %d, %Y` on 2026-09-01 gives `Tuesday, September 01, 2026`.

"N calendar days from `t`" is computed in calendar days, not in 86 400-second
steps (`utils.lua:22`): take the date of `t`, add N to the day field, set the
time to **12:00:00**, and let the day overflow into the next month or year.
Any date computed this way therefore carries the time 12:00. This matters for
`{{time}}` (§3).

### 0.4. How the plugin reads and writes a file

Every operation that writes a note **file** reads it with `vim.fn.readfile`
and writes it back with `vim.fn.writefile`. The byte effect is as follows
(verified with `nvim --headless`):

- **Reading:** the file is split on `\n`. A `\r` immediately before a `\n` is
  dropped. A leading UTF-8 BOM is dropped. A final line with no `\n` is still
  a line. A trailing `\n` does not produce an empty last line.
- **Writing:** each line is written followed by `\n`.

A file-writing operation therefore normalises the **whole file**:

- CRLF line endings become LF.
- A leading BOM is removed.
- A trailing `\n` is guaranteed.
- Every other byte is preserved exactly.

ttykasten MUST reproduce this exactly for every write. It MUST NOT, for
example, keep a missing final newline missing.

Some commands edit the **buffer** instead of the file (TaskToggle,
TaskCancel, TaskTag, TaskDue on the current line, YankLink, and LinkToDaily
when the daily note is loaded). Those edits reach the disk on `:w`, under
Vim's own write rules: `fileformat` preserved, `fixeol`. ttykasten has no
buffer, so it performs these as file writes with the rules above. The
**line text** it produces must be exactly what the buffer operation would
produce.

## 1. Defaults and example configuration

Defaults come from `config.lua:3-579`. The "Example configuration" column is a
typical `setup()` of the kind ttykasten is tested with; values in bold differ
from the default. The rest of this document assumes the example configuration
unless it says otherwise. Only options with a file-level meaning are listed.

| Option | Plugin default | **Example configuration** |
|---|---|---|
| `home` | `$ZETTELKASTEN_HOME` or `~/notes` (`config.lua:6`) | **`~/zettelkasten`**. The environment variable is then ignored because `setup()` overrides it. |
| `extension` | `md` (`config.lua:7`) | `md` |
| `hdate_format` | `%B %d, %Y` (`config.lua:8`) | **`%A, %B %d, %Y`** |
| `new_note_template` | nil (`config.lua:9`) | **`templates/template_new_note.md`** |
| `follow_link.create_nonexisting` | false (`config.lua:14`) | **true** |
| `follow_link.new_note_template` | nil | nil, so `new_note_template` is used |
| `notes.daily.dir` / `.format` / `.template` | `daily` / `%Y-%m-%d` / `daily.md` (`config.lua:109-111`) | **`lognote`** / `%Y-%m-%d` / **`templates/template_daily_note.md`** |
| `notes.daily.use_external_cmd` | false (`config.lua:112`) | false. No `## External Data` section is ever appended. |
| `notes.daily.lookback_days` | 30 (`config.lua:116`) | 30 |
| `notes.weekly.dir` / `.format` / `.template` | `weekly` / `%G-W%V` / `weekly.md` (`config.lua:119-122`) | **`lognote`** / `%G-W%V` / **`templates/template_weekly_note.md`** |
| `notes.weekly.lookback_weeks` | 8 (`config.lua:125`) | 8 |
| `patterns.tag` | `#([%w_-]+)` (`config.lua:105`) | the same, set explicitly |
| `patterns.link` | `%[%[(.-)%]%]` (`config.lua:106`) | the same |
| `transform.new_file_name` | identity (`config.lua:480-482`) | identity |
| `transform.sanitize_filename` | see §4 (`config.lua:488-495`) | default |
| `transform.insert_link` | `[[%s]]` (`config.lua:477-479`) | the same, set explicitly |
| `template_placeholders` | `{}` (`config.lua:501`) | `{}` |
| `block_id.length` / `.alphabet` | 6 / `a-z0-9` (`config.lua:76,79`) | the same |
| `block_id.alias` / `.alias_max` | false / 40 (`config.lua:85,102`) | the same (no alias) |
| `block_id.on_tag` / `.on_capture` | false / false (`config.lua:93-94`) | **true / true** |
| `block_id.daily_bullet` | `"- "` (`config.lua:97`) | `"- "` |
| `tasks.scope` | `all` (`config.lua:295`) | `all`, set explicitly |
| `tasks.require_tag` | nil (`config.lua:436`) | **`todo`** |
| `tasks.since_days` | nil (`config.lua:311`) | **60** |
| `tasks.always` | `{}` (`config.lua:322`) | **`{ "tasks/active.md" }`** |
| `tasks.capture_note` | nil (`config.lua:360`) | nil, so capture goes to `always[1]` = `tasks/active.md` |
| `tasks.date` | nil (`config.lua:319`) | **an example hook reading `**作成日**`** (§8.4) |
| `tasks.date_keys` | `{ "date", "created" }` (`config.lua:315`) | the same |
| `tasks.ignore.frontmatter_key` / `.dirs` | `tasks` / `{ "templates" }` (`config.lua:302,304`) | the same |
| `tasks.patterns.*`, `marks`, `new_checkbox`, `done_stamp`, `cancel_stamp`, `cancel_strike` | `config.lua:323-465` | the same (§8) |
| `week.ignore_dirs` | `{ "templates" }` (`config.lua:233`) | the same |
| `week.ignore_patterns` | `{}` (`config.lua:237`) | `{}` |
| `week.digest.sources` | `{}` (`config.lua:269`) | **`Messages`**, from a hypothetical external command `mail-week FROM TO` |
| `calendar.enabled` / `.name` | false / nil (`config.lua:187,191`) | **true / `Personal`** |
| `graph.ignore_dirs`, `recent.ignore_dirs` | `{ "templates" }` | the same |

`notes.daily.fzf_opts.cmd` / `notes.weekly.fzf_opts.cmd`, if set, only
affect the fzf listings of the old FindDaily and FindWeekly pickers. The log picker (§6) passes a fixed list, and fzf-lua then
ignores `cmd`. These options have no file-level meaning.

## 2. Collection layout

### 2.1. Where things live

- **Home:** `~/zettelkasten` in the example configuration.
- **Notes:** files named `*.md`. Some live in the home root and some in
  subdirectories (`lognote/`, `tasks/`, `notes/`, and so on). A note is named
  by its basename without the extension (§9.2), wherever it is filed.
- **Daily notes:** `lognote/<strftime("%Y-%m-%d")>.md`, for example
  `lognote/2026-09-29.md`.
- **Weekly notes:** `lognote/<strftime("%G-W%V")>.md`, for example
  `lognote/2026-W40.md`. Daily and weekly notes share a directory.
  - `%G` is the **ISO** year, so every day of an ISO week gives the same
    name: 2025-12-29 is `2026-W01`, and 2027-01-01 is `2026-W53` (Q1).
  - The filename is computed as `transform.new_file_name(strftime(format, t)) .. ".md"`
    (`core.lua:18-21`). No sanitising is applied.
- **Templates:** `templates/`. The example configuration uses the templates
  in the table below.

| Template | Content |
|---|---|
| `template_new_note.md` | frontmatter `title: {{title}}` / `date: {{date}}`, then a few section headings such as `# Related` |
| `template_daily_note.md` | frontmatter `title: {{hdate}}` / `date: {{date}}`, then `# Log` |
| `template_weekly_note.md` | frontmatter `title: {{isoyear}}-W{{week}}` / `date: {{date}}`, then section headings such as `# Tasks` |
| any other template | picked by hand only (§4) |

- **Standing task list:** `tasks/active.md`. It is the `always` entry and the
  capture target (§8.9).

### 2.2. Which files each operation sees

Each operation enumerates the collection its own way, and ttykasten must
reproduce the set each one sees.

| Operation | Walker | Hidden (dot) files and dirs | `.gitignore` | `.ignore` / `.rgignore` | Extra exclusions |
|---|---|---|---|---|---|
| Task scan (§8) | `rg --files-with-matches --no-messages --glob '*.md' -e SCAN HOME` (`tasks.lua:124-129`) | skipped | **honoured** (home is a git repo) | honoured | `tasks.ignore.dirs` = `templates` (`tasks.lua:223-230`); notes opted out by frontmatter |
| Note finder (§7), week notes (§11) | `rg --files --no-messages --no-ignore-vcs --glob '*.md'`, run in home (`pickers.lua:51-54`) | skipped | **not** honoured | honoured | finder: none. Week: `templates/`, `week.ignore_patterns` (none in the example), and the week's own weekly note. |
| Backlinks, link resolution, rename, SearchByTag | `vim.fn.glob(home.."/**/*.md")` (`pickers.lua:670,944`; `core.lua:229`) | skipped (verified) | not honoured | not honoured | none. Templates **are** included. |
| Task scan fallback (no `rg`, or `patterns.scan = false`) | the same Vim glob (`tasks.lua:99-102`) | skipped | no | no | `templates/` |

Notes:

- A note matched by `.gitignore` is therefore invisible to the task scan, but
  visible to the finder and to week views (unless `week.ignore_patterns`
  drops it).
- In every case "is ignored by directory" means that the relative path `rel`
  either equals `dir` or begins with `dir .. "/"` (`tasks.lua:225`,
  `week.lua:107`).

## 3. Templates and placeholders

`load_template(rel_path, title, time)` (`core.lua:52-102`) works as follows.

1. **Locate the file.** Strip one leading `templates/` from `rel_path`
   (`core.lua:55`). The file is `home/templates/<rest>`. So
   `templates/template_daily_note.md` and `template_daily_note.md` name the
   same file.
2. **Missing template.** If the file is not readable, the result is exactly
   `"# " .. title` (`core.lua:58-60`).
3. **Read.** Otherwise read the file per §0.4 and join its lines with `\n`.
   The result therefore has **no trailing newline**, even when the file had
   one.
4. **Substitute.** Replace every `{{KEY}}` using the Lua pattern `{{(.-)}}`
   (`core.lua:82`). `KEY` is the raw text between the braces, which is lazy
   and **not trimmed**, so `{{ title }}` is an unknown key. Replacement values
   are inserted literally, with no `%` interpretation.

| Key | Value (`core.lua:63-77`) |
|---|---|
| `title` | the `title` argument |
| `date` | `strftime("%Y-%m-%d", time)` |
| `hdate` | `strftime(hdate_format, time)`, which in the example configuration gives `Tuesday, September 29, 2026` |
| `year` / `month` / `day` | `%Y` / `%m` / `%d` |
| `isoyear` | `%G`, the ISO week-based year. It pairs with `week`: on 2025-12-29, `{{isoyear}}-W{{week}} {{year}}` is `2026-W01 2025`. |
| `week` | `%V`, the ISO week number, two digits |
| `time` | `%H:%M` |
| `agenda` | the day's calendar events as bulleted lines (§12) |
| `agenda_week` | the calendar for the ISO week containing `time` (§12) |

Keys are case-sensitive. An **unknown key is left intact**, braces and all
(`core.lua:84-86`). A placeholder function that raises is also left intact.
`template_placeholders` would add or override keys, but the example
configuration sets none. None of the example templates uses `agenda` or
`agenda_week`.

Two quirks of the lazy pattern:

- `{{a}}}` yields `A}`.
- In `{{{{title}}}}`, the first match has key `{{title`, which is unknown and
  left intact. Scanning resumes after it, so the inner `{{title}}` is never
  substituted.

**Writing the result.** The substituted string is split on `\n` into lines,
and each line is written followed by `\n` (§0.4). In practice, a template file
ending in `\n` produces a note whose bytes equal the substituted template
bytes.

## 4. New note (`:FzfKastenNewNote`)

`create_new_note_interactively` is at `core.lua:104-142`.

1. **Ask for a title.** If `title` is empty after deleting all `%s`, cancel.
2. **Pick a template.** The user picks from the files under `home/templates/`,
   and only the **basename** of the choice is kept (`pickers.lua:1337-1364`).
   If they cancel, `new_note_template` is used, which in the example
   configuration is `templates/template_new_note.md` (`core.lua:112-115`).
3. **Build the file name.** Apply `sanitize_filename(new_file_name(title))`
   (`config.lua:488-495`), in this order:
   1. Delete every byte in `/ \ : * ? " < > |` and every control byte (`%c`).
      Tabs and newlines are therefore **deleted**, not turned into spaces.
   2. Trim leading and trailing ASCII whitespace.
   3. Collapse each run of ASCII whitespace to a single space.
   4. Delete leading `.`s, then trailing `.`s. Whitespace uncovered by this
      step is not re-trimmed: `". foo"` becomes `" foo"`.
   5. Unicode is kept as it is.
   6. If the result is `""`, abort.
4. **Choose the path:** `home/<sanitized>.md`, always in the **home root**.
5. **Fill the content.** Use `load_template(template, title)` (§3), where
   `title` is the **raw** title as typed (not trimmed, not sanitised) and
   `time` is now. With no template at all, the content is `"# " .. title`
   (`core.lua:131-136`).
   - Example: for the title `Meeting: Acme / Q3`, step 3.1 gives
     `Meeting Acme  Q3` and step 3.3 gives `Meeting Acme Q3`, so the file is
     `home/Meeting Acme Q3.md`. With the example template, the note starts
     `---\ntitle: Meeting: Acme / Q3\ndate: 2026-09-29\n---\n`.
6. **Write.** The plugin replaces the whole buffer with the content
   (`core.lua:138-139`) **even when the file already exists**, and the file
   is written on `:w`. See Q3.

**Creating a note from a link** (`follow_link.create_nonexisting = true`,
`pickers.lua:971-985`) works differently:

- The file is `home/<sanitize_filename(name)>.md`, where `name` is the
  link's name part (§9.2), so `[[a:b]]` creates `ab.md`. If sanitising leaves
  nothing, the note is not created (Q11).
- The content is `load_template(new_note_template, name)`, or `"# " .. name`
  when there is no template. `{{title}}` is the **raw** link name.
- It is applied only when the buffer is empty, meaning the file is new or
  empty.

## 5. Daily and weekly notes (`:FzfKastenDaily`, `:FzfKastenWeekly`)

`core.open_note(type, time = now)` is at `core.lua:15-33`.

1. Compute `date_str = strftime(format, time)`. The path is
   `home/<dir>/<date_str>.md`. The directory is created with `mkdir -p`
   immediately.
2. **Open or create.** If the file exists, open it unchanged. Otherwise fill
   an empty buffer with `load_template(template, date_str, time)`
   (`core.lua:35-50`). The plugin writes it only on `:w`, and the file does
   not exist until then. ttykasten should write it at once.
3. **Templates.** Daily notes use `templates/template_daily_note.md`, and
   weekly notes use `templates/template_weekly_note.md`. `{{title}}` is
   `date_str`, which is `2026-09-29` or `2026-W40`, but the example templates
   use `{{hdate}}` and `{{isoyear}}-W{{week}}` instead.
4. **Dates are creation dates.** `{{date}}` is the date of `time`, not the
   Monday of the week. A weekly note for `2026-W39` created on Thursday
   2026-09-24 has `date: 2026-09-24`.

**Daily example**, created 2026-09-29 at 08:10:

```
---
title: Tuesday, September 29, 2026
date: 2026-09-29
---
# Log
```

## 6. The log picker (`:FzfKastenLog`)

`log_entries(now)` (`pickers.lua:1411-1438`) builds the list in this order:

1. **Days:** for `i = 0 … lookback_days-1` (0 to 29), `t = now − i`
   calendar days, at 12:00. Each entry is the daily note for `t`, labelled
   `strftime("%Y-%m-%d (%a)", t)`, for example `2026-09-29 (Tue)`.
2. **Weeks:** for `i = 0 … lookback_weeks-1` (0 to 7), `t = now − 7i`
   calendar days, at 12:00, and so on the **same weekday** as today. Each
   entry is the weekly note named `strftime("%G-W%V", t)`, labelled with that
   same string.

Newest comes first within each group, and all days come before all weeks.
There are no duplicates within a group.

**Display.** Each entry is the string `"<rel>:1: <mark> <label>"`, where
`<mark>` is `✓` if the file is readable on disk and a space otherwise. Only
the part after the second `:` is shown, so the entry reads ` ✓ 2026-09-29 (Tue)`.

**Selecting an entry** runs `open_note(type, t)` (§5). A note created this way
gets `{{time}}` = `12:00`, and a weekly note gets `{{date}}` = that weekday's
date.

**`<ctrl-x>`** asks for a date `YYYY-MM-DD` (regex `^\d{4}-\d{2}-\d{2}$`),
refuses a future date, and opens that day's daily note at 12:00
(`pickers.lua:1385-1403`).

## 7. Note finder (`:FzfKastenFindNotes`)

**What is listed** (`pickers.lua:189-248, 445-474`):

- Every path from `rg --files --no-ignore-vcs --glob '*.md'` in home (§2.2),
  as a path **relative to home**.
- Templates **are** included, and nothing else is excluded.
- `rg` returns the paths in no particular order.
- Selecting an entry opens `home/<rel>`.

**Matching:**

- **A plain query** is matched by fzf against the **path only**: an fzf fuzzy
  subsequence, where space-separated words are ANDed and fzf's operators work.
  Pinned by `tests/find_spec.lua:101-128`.
- **A romaji query** (one starting with `/`, and only when a migemo backend,
  `ttyskk migemo`, is present) turns the rest of the query into a regex. It
  is matched against the path **and the note's heading lines**.
  - A heading line is any line matching `^#+\s+\S` (`pickers.lua:93-95,121`).
    It may be anywhere in the file, including frontmatter and fenced blocks,
    and the whole line, hashes included, is used.
  - Body text is never matched (`tests/find_spec.lua:120-123`).
  - A bare `/` shows everything. If the backend cannot answer, the rest of
    the query is used as a plain query.
- `romaji.headings = false` would restrict romaji matching to paths. The
  default is true.

The heading cache (`find.headings_stale_after`) and the shell scripts
(`pickers.lua:376-443`) exist for speed only, and change nothing in the
result.

**Insert link** (`:FzfKastenInsert`, `pickers.lua:557-592`) lists files the
same way (via `fzf.files`) and inserts
`transform.insert_link(basename_without_ext)` = `[[<name>]]`. It inserts the
**name, never the directory**.

## 8. Tasks

Tasks are the most important part of this specification. There is no index:
the notes are the only record of the tasks.

### 8.1. Line grammar

The patterns are at `config.lua:323-347`, and the example configuration does
not change them.
A line is a checkbox of kind **open**, **done** or **cancelled** if it matches
one of the following, tried in that order (`tasks.lua:791-800`):

| Kind | Lua pattern | Rule |
|---|---|---|
| open | `^%s*[-*]%s+%[ %]%s+(.+)$` | `/^\s*[-*]\s+\[ \]\s+(.+)$/` |
| done | `^%s*[-*]%s+%[[xX]%]%s+(.+)$` | `/^\s*[-*]\s+\[[xX]\]\s+(.+)$/` |
| cancelled | `^%s*[-*]%s+%[%-%]%s+(.+)$` | `/^\s*[-*]\s+\[-\]\s+(.+)$/` |

The capture is the task's **raw text**.

- The bullet must be `-` or `*`. A `+` bullet or a numbered item (`1.`) is
  **never** a checkbox.
- At least one whitespace byte is required between the bullet and `[`, and
  again after `]`.
- `- [ ]` with nothing after it is not a task. However, `- [ ]  ` (two
  trailing spaces) is one, with raw text `" "` and therefore empty text.
- Any other mark is not a checkbox. For example, `- [>] …` is a plain
  **bullet** (§8.5).

**Splitting a checkbox.** Every writer splits a checkbox line with
`toggle = ^(%s*[-*]%s+%[)([ xX-])(%])` into `before` (indent + bullet + `[`),
`mark`, `after` (`]`) and `rest` (everything after `]`, starting with its
whitespace). The line is exactly `before..mark..after..rest`
(`tasks.lua:370-381`, pinned by `tests/tasks_spec.lua:20-45`). This pattern
does **not** require text after `]`.

**Marks written** (`config.lua:349`): open is a space, done is `x`, and
cancelled is `-`. `X` is read as done, but never written.

### 8.2. What a task's raw text contains

`parse_task_text` (`tasks.lua:325-360`) works through the raw text in the
order below. The task's `text` is what remains at the end.

1. **Priority.** Match `^%((%u)%)%s+` (`/^\(([A-Z])\)\s+/`). The priority must
   come first and be followed by whitespace, and it is removed from `text`.
   `(2)`, `(1)`, `(a)` and `(AB)` are **not** priorities; `(2)` stays part of
   the text.
2. **Due date.** Search the remaining text for
   `due:(%d%d%d%d%-%d%d%-%d%d[T%d:]*)`
   (`/due:(\d{4}-\d{2}-\d{2}[T\d:]*)/`). It may be anywhere in the text,
   there is no word boundary before `due:`, and the first match wins.
   - Example: `due:2026-07-25T15:00` gives the due date `2026-07-25T15:00`.
   - The token **stays in** `text`, and is stripped only for display
     (`tasks.lua:933-941`).
   - A hand-written `due:today` or `due:YYYY-MM-DD` does not match and is
     plain text.
3. **Done stamp.** Search for `%s*done:(%d%d%d%d%-%d%d%-%d%d %d%d:%d%d)`
   (`/\s*done:(\d{4}-\d{2}-\d{2} \d{2}:\d{2})/`). The first capture is
   `done_at`, and **every** match, together with its leading whitespace, is
   removed from `text` (`config.lua:446-449`).
4. **Cancel stamp.** The same, with `%s*cancelled:(%d%d%d%d%-%d%d%-%d%d)`,
   giving `cancelled_at` (`config.lua:455-458`).
5. **Block ids.** Strip every block id (§9.1) from `text`, each together with
   the one whitespace byte before it.
6. **Strike.** Trim `text`. If it starts **and** ends with `~~`, remove that
   one outer pair (`tasks.lua:316-322`). A `~~` that does not wrap the whole
   text is kept.
7. **Final trim.** Trim `text` again.

The task's `id` is the block id of the **raw** text (§9.1). Tags are **not**
removed from `text`.

Worked example (compare `tests/blockid_spec.lua:176-185`):

```
- [-] (A) ~~#projectx review draft #todo due:2026-09-14~~ cancelled:2026-09-16 ^a1b2c3
```

This line parses as follows:

- kind: cancelled
- priority: `A`
- due: `2026-09-14`
- cancelled_at: `2026-09-16`
- id: `a1b2c3`
- text: `#projectx review draft #todo due:2026-09-14`

### 8.3. Which lines are scanned

`scannable_lines(lines, fm_end)` (`tasks.lua:250-278`) is shared by the scan
and by TaskTag / mint-ids.

**Frontmatter** (`tasks.lua:141-156`):

- The note has frontmatter only if line 1 is exactly `---`. It is closed by
  the next line that is exactly `---`, and those lines are excluded.
- If the block is never closed, the note has **no** frontmatter, and nothing
  is excluded.
- Only flat pairs are read: `^([%w_-]+):%s*(.-)%s*$`, with the value
  trimmed and not unquoted.

**Fences:**

- A line matching `^%s*(```+)` or `^%s*(~~~+)` opens a fenced block. Any
  indentation is allowed.
- The block is closed only by a delimiter of the same character that is **at
  least as long** as the opener.
- Delimiter lines and everything between them are excluded.

**Headings:** `^#+%s+(.*)$`, which requires whitespace after the hashes, so
`#todo foo` is **not** a heading. A heading line is never scannable.

**Scope.** With `scope = "all"`, the default and the example setting, every
other line is scannable.

With `scope = "headings"`, which the example configuration does not use, a line is scannable
only below a heading whose trimmed, lowercased text matches one of
`^tasks?%f[%A]`, `^to%-?dos?%f[%A]`, `^タスク` or `^やること`, up to the next
heading (`config.lua:297`). Lowercasing is ASCII-only.

**A note is skipped entirely when:**

- it lies under `tasks.ignore.dirs` (`templates/`), or
- its frontmatter `tasks:` value is exactly `false`, `no` or `off`
  (`tasks.lua:219-221,769-770`). For example, a specification note can opt
  out with `tasks: false`.

**Pre-filter.** A file is read at all only if `rg` finds
`^\s*[-*]\s+\[[ xX-]\]\s+` in it (`config.lua:331`). This is a superset of
the three kinds, and it changes nothing except which files are read.

### 8.4. A note's date

`note_date(path, lines, fm)` is at `tasks.lua:170-203`. It is also used for
week views (`week.lua:197`). The first of these that yields a date wins:

1. **The `tasks.date` hook**, if set. The example configuration sets one that
   works as follows.
   - Look at `lines[1..10]`, the file's first 10 lines, frontmatter included.
   - For each, match `^%*%*作成日%*%*[::]%s*(%d%d%d%d%-%d%d%-%d%d)`. The class
     `[::]` is two **ASCII** colons, so only `:` works and the full-width `：`
     does **not**.
   - Example: a line `**作成日**: 2026-03-12` dates the note 2026-03-12.
   - In general, the first YYYY-MM-DD in the hook's return value is taken.
2. **The filename.** The first `\d{4}-\d{2}-\d{2}` anywhere in the basename.
   - `2026-09-18 meeting acme.md` gives 2026-09-18.
   - `2026-05-14-10-30-00.md` gives 2026-05-14.
   - `20260514-1030.md` gives nothing.
3. **Frontmatter keys**, `date` then `created`. The first
   `\d{4}-\d{2}-\d{2}` in the value.

Otherwise the note is **undated**. mtime is never used.

### 8.5. Nesting and `require_tag = "todo"`

The scan walks the note's scannable lines top to bottom with a stack
(`tasks.lua:787-894`).

**List items.** A line is a list item if it is a checkbox (§8.1) or a
**bullet**. A bullet is `^%s*[-*+]%s+(.+)$` or `^%s*%d+[%.%)]%s+(.+)$`, and a
checkbox is tested first.

**Lines at the margin end the list** (Q9). Every line of the file that is
not a scannable list item and starts with a non-whitespace byte (`^%S`)
**empties the stack**. This covers a paragraph at column 0, a heading, a
fence delimiter or fenced line at column 0, and the frontmatter. Blank lines
and indented prose leave the stack unchanged, since they sit inside a loose
list item.

**Indent.** A line's indent is its leading spaces and tabs, counted with a
tab as 4 (`tasks.lua:282-288`).

**For each list item:**

1. Pop every stack entry whose indent is **≥** the item's indent. The new
   top, if any, is the item's parent.
2. If the item is a bullet, push it with `tagged = parent.tagged` (or false
   when there is no parent). A bullet **never introduces** the tag, even if
   its text contains `#todo`, but it passes its parent's tag down.
3. If the item is a checkbox, it is **tagged** when `has_tag(text, "todo")`
   is true or the parent is tagged. Push it.

**`has_tag(text, tag)`** (`tasks.lua:311-313`) tests `text` for
`#todo%f[^%w_-]`: the next byte must not be `[A-Za-z0-9_-]`, the same set
the tag pattern reads (§10). It is case-sensitive and has no left boundary.
So:

| Text | Tagged? |
|---|---|
| `#todos` | no |
| `#TODO` | no |
| `foo#todo` | **yes** |
| `#todo-x` | no (Q8) |
| `#todo_list` | no (Q8) |
| `#todo,` | yes |
| `#todo ^abc123` | yes |

It is applied to the parsed `text` (§8.2), so a `#todo` inside a strike still
counts.

**Classification:**

- **Task list:** the tagged checkboxes.
- **Inbox** (`:FzfKastenTaskInbox`): the untagged checkboxes.
- By default both lists exclude done and cancelled tasks
  (`tasks.lua:901-908`). The week digest asks for done tasks too.

Example:

```
- [x] (B) draft proposal #todo #projectx due:2026-07-31 done:2026-08-06 19:29 ^a1b2c3
  - [x] (A) collect quotes done:2026-07-31 08:58        <- subtask, inherits #todo
```

### 8.6. Aging (`since_days = 60`) and `always`

The cutoff is `strftime("%Y-%m-%d", now − 60 calendar days)`
(`tasks.lua:753-758`). A note is **aged out** when:

- it has a date, and
- that date is lexicographically **< cutoff**, and
- its path relative to home is not exactly `tasks/active.md`.

Because `require_tag` is set, aging **never** removes a tagged task. It only
drops the untagged, inbox checkboxes of aged-out notes
(`tasks.lua:774-779,888`). Undated notes are never aged out.

### 8.7. Collecting and ordering tasks

For each task, the plugin records `path`, `rel`, `lineno` (1-based),
`date` (the note's date), `priority`, `due`, `done_at`, `cancelled_at`, `id`,
`depth`, `parent` and `context` (the text of the list item directly above
it).

**Sorting** (`tasks.lua:697-729`) always uses the fields of the item's
**outermost ancestor task** (its root), so subtasks stay under their root.

| Mode | Keys, in order |
|---|---|
| `priority` (the default) | `root.priority` (none sorts as `~`, i.e. last) → `root.due` (none sorts as `9999-99-99`) → path → `root.lineno` |
| `due` | due → priority → path → lineno |
| `added` | note date (none sorts as `9999-99-99`) → path → lineno |

Ties within one root are broken by `lineno` ascending. Reversing the order
reverses the roots but not the steps within a root.

### 8.8. Write operations

**Common rule: the block id stays last.** Every line rewrite (toggle, cancel,
tag and due) goes through `keeping_block_id` (`tasks.lua:393-403`):

1. Take the line's first block id.
2. Strip **all** block ids from the line.
3. Apply the rewrite.
4. If the rewrite succeeded, append the id again with `with_block_id`, which
   trims trailing whitespace and appends ` ^id`.

`tag_at` is the one exception (below).

**Common rule: timestamps.** `now` means the local time when the operation is
performed.

#### TaskToggle / done (`toggle_line`, `tasks.lua:408-435`)

- **Refusals:** a cancelled mark is refused (`cancelled`), and a line with no
  checkbox is refused.
- **Mark:** a space becomes `x`. `x` or `X` becomes a space.
- **Stamp:** then remove **every** `%s*done:YYYY-MM-DD HH:MM` from the line,
  and trim trailing whitespace. If the task is now done, append
  `strftime(" done:%Y-%m-%d %H:%M")`.
- **Examples** (`tests/tasks_spec.lua:47-74`, `tests/blockid_spec.lua:131-141`):

```
- [ ] draft #todo ^t3k9aa        ->  - [x] draft #todo done:2026-09-29 14:05 ^t3k9aa
- [x] foo done:2026-07-17 10:00  ->  - [ ] foo
```

#### TaskCancel (`cancel_line`, `tasks.lua:452-493`)

The line is split per §8.1.

- **Refusals:** the mark `x` or `X` is refused (`done`); the mark is compared
  case-insensitively (Q6). A line with no checkbox, or whose `rest` is only
  whitespace, is refused (`no checkbox`).
- **Split `rest`** into `lead` (its leading whitespace, kept verbatim) and
  `text` (the rest, with trailing whitespace trimmed).
- **Priority:** if `text` starts with a priority, set `kept` = the priority
  together with its trailing whitespace run, and remove it from `text`.
- **Cancel stamp:** remove every `%s*cancelled:YYYY-MM-DD` from `text`, trim,
  and unstrike (§8.2 step 6).
- **If the mark was `-`**, reopen: the new mark is a space, and nothing is
  added.
- **Otherwise**, cancel: `text = "~~"..text.."~~"`, then append
  `strftime(" cancelled:%Y-%m-%d")`, and the new mark is `-`.
- **Result:** `before .. mark .. after .. lead .. kept .. text`. A `due:` and
  the tags end up **inside** the strike.

```
- [ ] (A) #projectx review draft #todo due:2026-09-14 ^a1b2c3
  -> - [-] (A) ~~#projectx review draft #todo due:2026-09-14~~ cancelled:2026-09-16 ^a1b2c3
- [-] (A) ~~foo~~ cancelled:2026-07-17  ->  - [ ] (A) foo
```

#### TaskTag, one line or a visual range (`tasks.tag`, `tasks.lua:1316-1369`; `tag_line`, `tasks.lua:505-527`)

Only lines that are scannable (§8.3) in the whole file are considered. For
each line in `[line1, line2]`:

1. **Promote to a checkbox.** If the line has no checkbox of any mark
   (`split_checkbox` fails):
   1. Split it into `indent` (leading whitespace) and `text`, with trailing
      whitespace trimmed.
   2. Remove one leading `-` or `*` bullet (`^[-*]%s+`). `+` and `1.` are not
      removed.
   3. If `text` is now empty, skip the line.
   4. The line becomes `indent .. "- [ ] " .. text`.
2. **Add the tag.** If `has_tag(line, "todo")` is true, skip the line (it is
   already tagged). Otherwise the line becomes the line with trailing
   whitespace trimmed, then ` #todo`.
3. **Mint an id (`on_tag = true`).** If the line was changed and has no block
   id, append a new id (§9.1). The id avoids every id already in the file and
   those minted earlier in this range.

An existing checkbox with any mark is tagged in place: `- [-] ~~foo~~ cancelled:… #todo`.
If no line changes, the operation fails with "Nothing to tag".
Examples (`tests/tasks_spec.lua:117-149`, with `on_tag`):

```
buy milk            ->  - [ ] buy milk #todo ^k2m9qa
  - buy milk        ->    - [ ] buy milk #todo ^p0x7e1
1. buy milk         ->  - [ ] 1. buy milk #todo ^…
- [ ] foo ^abc123   ->  - [ ] foo #todo ^abc123
```

**`tag_at`, the variant used by the task list and picker** (`t` / `<ctrl-t>`,
`tasks.lua:1053-1093`) differs:

- It accepts only lines matching open or done (§8.1). It refuses cancelled
  lines and prose.
- It rewrites the line with `tag_line`, as TaskTag does, so the id stays
  last: `- [ ] foo ^abc123` becomes `- [ ] foo #todo ^abc123` (Q5).
- It never mints an id.

#### TaskDue (`plugin:47`; `set_due` / `due_at`, `tasks.lua:1156-1253`; `due_line`, `tasks.lua:626-651`)

**Resolving the argument** (`resolve_due`, `tasks.lua:587-614`). The argument
is trimmed, then read as follows:

- **Empty:** clear the due date.
- **Absolute**, passed through unchanged:
  - `^\d{4}-\d{2}-\d{2}$`, or
  - `^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$`.

  The date's calendar validity is not checked. Seconds, zones and a space
  separator are rejected (`tests/tasks_spec.lua:235-248`).
- **Relative**, resolved to `%Y-%m-%d` relative to today. English forms are
  case-insensitive, and Japanese forms must match exactly.

  | Form | Result |
  |---|---|
  | `today`, `今日` | +0 days |
  | `tomorrow`, `tom`, `tmr`, `明日`, `あした` | +1 day |
  | `明後日`, `あさって` | +2 days |
  | `^\+?\d+[dw]$` (`+3d`, `3d`, `2w`, `0d`) | N days or 7N days. Negative values are not accepted. |
  | `sun…sat`, `sunday…saturday`, `日 月 火 水 木 金 土` | the nearest such day **at or after** today. Today's own weekday gives today. |

- **Anything else:** refused ("bad date"). Nothing is written.

**Rewriting the line** (`due_line`):

- **Refusals:** a line with no checkbox, or a mark that is not a space, is
  refused (`not open`). This covers done, `X` and cancelled tasks.
- Remove **every** `%s*due:(\d{4}-\d{2}-\d{2}[T\d:]*)` from the line, and trim
  trailing whitespace.
- **Clearing:** that is the result. If there was no due date, the line is
  unchanged, and the operation reports "No due date to clear" and writes
  nothing.
- **Setting:** append ` due:<DATE>`. Appending the same due date again leaves
  the line unchanged, and nothing is written.
- The id is kept last.

```
- [ ] foo due:2026-07-20 #todo ^t3k9aa  --(fri, on Wed 2026-07-22)-->
  - [ ] foo #todo due:2026-07-24 ^t3k9aa
```

#### TaskAdd, capture (`plugin:44`; `tasks.add`, `tasks.lua:1497-1552`)

**Target:** `home/tasks/active.md`, from `capture_note`, falling back to
`always[1]` (`tasks.lua:1437-1444`). If the file is missing, its directory is
created and the file will contain only the new line. There is **no**
template.

**The line** (`new_task_line`, `tasks.lua:533-548`) is built in this order:

1. `"- [ ] " .. trim(text)`. Blank text is refused.
2. If `has_tag(line, "todo")` is false, append ` #todo`.
3. If a due date was given, append ` due:<ISO>`.
4. With `on_capture = true`, append ` ^<new id>` (via `with_block_id`). The
   id avoids the ids already in the file.

**Insertion:** the line is **appended as the last line** of the file, after
the §0.4 read. Trailing blank lines stay above it. The whole file is then
written.

**Argument forms:**

- `:FzfKastenTaskAdd <text>` captures `<text>` verbatim: no due-date
  resolution and no tag parsing.
- `:FzfKastenTaskAdd` with no argument prompts for the text (`init.lua:51-60`).
- The guided capture (`tasks.capture`, `tasks.lua:1636-1688`), used by `a` in
  the list and `<alt-a>` in the picker, prompts for three things in turn:
  1. The text.
  2. Tags to add, each appended as ` #name` unless `has_tag` is already true.
  3. A due date, resolved as for TaskDue. An unrecognised due date is
     dropped, and the task is captured without one.

Example: `call Acme back` with due `fri` on Wed 2026-09-30 appends
`- [ ] call Acme back #todo due:2026-10-02 ^b7d2e5` to `tasks/active.md`.

#### Mint ids (`:FzfKastenBlockIds`, `tasks.lua:1465-1495`)

Over the range, which defaults to the whole file, a new id is appended to
each line that meets all of these conditions:

- it is scannable,
- it is a checkbox with any mark,
- it has no block id, and
- it has `has_tag(line, "todo")` on the **line itself**.

Inherited tags do not count here.

#### Undo (`:FzfKastenTaskUndo`, `tasks.lua:1381-1429`)

The plugin keeps an **in-memory** stack of the last 50 file writes made by
the list or picker writers (`toggle_at`, `cancel_at`, `tag_at`, `due_at`) and
by `add`. Buffer edits are undone with Vim's `u` instead. Each stack entry is
`{path, lineno, before, after, added}`.

Undo pops the newest entry:

- If the file or the line is gone, or the line is not byte-equal to `after`,
  it refuses and drops the entry.
- Otherwise it restores `before`. For an addition, it deletes the line.

This history exists only inside one nvim process. It is not shared, so
ttykasten needs its own record of what it wrote (Q7).

## 9. Block ids and links

### 9.1. Block ids

**Grammar** (`utils.lua:109`): `%s%^([%w][%w-]+)%f[%W]`. In words:

- a whitespace byte,
- `^`,
- then an ASCII alphanumeric,
- then one or more of `[A-Za-z0-9-]`,
- where the last character is alphanumeric, followed by a non-alphanumeric
  byte or the end of the line.

So an id is **at least 2 characters** and must be **preceded by whitespace**
(never at the start of a line).

| Line | Id read |
|---|---|
| `x ^2` | none (too short, `tests/blockid_spec.lua:68`) |
| `a^bcdef` | none (no whitespace before `^`) |
| `^abc-` | `abc` |
| `^abc日本` | `abc` |

**Where ids are looked for:**

- **Reading** (`utils.lua:124`) finds the first id **anywhere** in the line.
  For example, it reads `^t3k9aa` from `- [x] plan ^t3k9aa done:…`.
- **Stripping** (`utils.lua:138`) removes every id together with its one
  leading whitespace byte.
- **Writing** (`with_block_id`, `utils.lua:154`) produces
  `rtrim(strip(line)) .. " ^" .. id`. The id always goes last, and replaces
  any id already on the line.

**Minting** (`utils.lua:171-201`): 6 characters drawn uniformly from
`abcdefghijklmnopqrstuvwxyz0123456789`. A draw that collides with a `taken`
set is redrawn, up to 64 times. The `taken` set is the ids in the lines of the
note being edited (`utils.lua:203`).

Minted ids look like `^a1b2c3` and `^t3k9aa`.

### 9.2. Wikilinks

**Finding links.** A link is `%[%[(.-)%]%]` (`/\[\[(.*?)\]\]/`), the shortest
match, found repeatedly along a line. `[[a]]b]]` yields `a`, and `[[[a]]]`
yields `[a`. Links never span lines.

**Parsing the inside** (`split_link`, `utils.lua:53-84`; pinned by
`tests/follow_spec.lua:56-119`), in this order:

1. **Alias:** split at the **first** `|`. Everything after it is the alias, so
   an alias may itself contain `#`.
2. **Anchor:** split the remainder at the **first** `#`. Everything after it
   is the anchor, so an anchor may itself contain `#`: `note#Q#A` has the
   anchor `Q#A`.
3. **Directory:** if the name contains `/`, the directory is everything
   before the last `/`, and the name is what follows it.
4. **Extension:** strip a trailing `.md`. Only the configured extension is
   stripped: `note.v2` stays `note.v2`, and `note.v2.md` becomes `note.v2`.

| Link | name | anchor | alias | dir |
|---|---|---|---|---|
| `[[sub/deep/note.md#見出し\|呼び名]]` | `note` | `見出し` | `呼び名` | `sub/deep` |
| `[[#top]]` | `""` (the same note) | `top` | | |
| `[[active#^a1b2c3]]` | `active` | `^a1b2c3` (a line link) | | |

A note is **named** by its basename minus its last extension
(`utils.lua:86-100`). A leading dot is not an extension: `.bashrc` keeps its
name. Names are compared exactly and case-sensitively.

**Resolving a name** (`pickers.lua:943-967`):

- Find every note found by the Vim glob (§2.2) whose name equals the link's
  name.
- If there is more than one **and** the link gave a directory, keep those
  whose parent directory equals `dir` or ends with `/`..`dir`, if any do.
- Zero matches creates the note (§4), because `create_nonexisting` is on.

**Anchors** (`pickers.lua:1014-1032`):

- The anchor is trimmed first.
- `^id` names the first line whose block id (§9.1) equals `id`.
- Otherwise it names the first line matching `^#+%s+(.-)%s*$` whose text,
  lowercased (ASCII only), equals the lowercased anchor. The text is matched
  as written, not as a slug.

**Renaming** (`core.lua:195-268`):

1. Move the file first. The new name is sanitised as in §4.
2. Then, in every note, rewrite each link whose name equals the old name to
   `[[new` + `#anchor` if present + `|alias` if present + `]]`.

Directory and extension are **dropped** from the rewritten link. Only
changed files are written (§0.4).

### 9.3. YankLink (`:FzfKastenYankLink`, `pickers.lua:1215-1253`)

YankLink works on the line under the cursor, in a note named `N`.

- A blank line is refused.
- If the line has no id, mint one (avoiding the note's ids) and write the line
  as `with_block_id(line, id)`. The rest of the line is untouched.
- The yanked text is `[[N#^id]]`. The note's directory is never included.
- With `block_id.alias`, which is off in the example configuration, the yanked text would be
  `[[N#^id|alias]]`, where the alias is the line with these removed: the id,
  the checkbox or bullet, the tags, and `[`, `]`, `|`. Whitespace is squeezed,
  and the alias is cut to 40 characters plus `…` (`pickers.lua:1175-1198`).

### 9.4. LinkToDaily (`:FzfKastenLinkToDaily`, `pickers.lua:1278-1335`)

1. Run YankLink's steps. This may mint an id in the source note.
2. The entry is `daily_bullet .. link`, which by default is `- [[N#^id]]`.
3. The target is today's daily note, `lognote/<%Y-%m-%d>.md`.
   - **The daily note is open in nvim:** the entry is inserted **after the
     cursor line** of a window showing it, or at the end of the buffer if no
     window shows it.
   - **The file exists but is not open:** the entry is **appended as the
     last line** (§0.4). This is what ttykasten should do.
   - **The file does not exist:** its directory is created, and its content
     is `load_template(daily.template, strftime("%Y-%m-%d"))` with
     time = now (or `"# " .. date` without a template). The content is split
     into lines, the entry is appended, and the file is written.

   With the example daily template, a new note therefore ends in `# Log`
   followed by `- [[…#^…]]`.

## 10. Tags and backlinks

**Tags** (`patterns.tag`, `config.lua:105`): `#([%w_-]+)`, that is
`/#([A-Za-z0-9_-]+)/`. There is no left boundary, and tags are ASCII only.

| Text | Tag read |
|---|---|
| `#projectx` | `projectx` |
| `#v2` | `v2` |
| `a#b` | `b` |
| `http://x/#frag` | `frag` |
| `#1` | `1` |
| `# Heading` | none (space after `#`) |
| `#会議` | none (non-ASCII) |

The tag listing uses the equivalent rg regex `#[a-zA-Z0-9_-]+`
(`tasks.lua:1565`, `pickers.lua:477`) over every `*.md`, sorted, with
`#todo` first. A word starting with `@` has no meaning to the plugin.

**Backlinks** (`collect_backlinks`, `pickers.lua:663-697`). The backlinks of
note `T` are all lines, in all notes found by the Vim glob (§2.2) except `T`
itself (compared with symlinks resolved), that contain at least one link
(§9.2) whose name equals `T`.

- Anchors, aliases, directories and `.md` are all allowed in the link.
- Each such line is reported once, as `rel:lineno: <trimmed line>`.
- Fenced code is **not** skipped, and templates are included.

(The link graph is not in scope here. It differs from backlinks in three
ways: it skips fenced blocks and inline code spans, it ignores `templates/`,
and it drops self-links, `graph.lua:37-66,103-110`.)

## 11. Week views and the week digest

**Week specifications** (`week.lua:65-101`):

| Spec | Week |
|---|---|
| empty | this ISO week |
| `-1`, `+2` | weeks relative to this one |
| `2026-W37`, `2026-w37`, `2026W37` | that ISO week (1-53); week 1 contains Jan 4 |
| `YYYY-MM-DD` | the week containing that day |

Each week has:

- a `label` of `strftime("%G-W%V", monday)`, using the **ISO year**,
- `from` = its Monday, and
- `to` = its Sunday.

**Week notes** (`week.lua:171-212`) are the notes listed by `rg --files`
(§2.2) that meet all of these conditions:

- not under `templates/`,
- not matching any Lua pattern in `week.ignore_patterns` (none in the
  example configuration),
- not the week's own weekly note, which is
  `lognote/<strftime("%G-W%V", monday)>.md`, which equals `label`, and
- having a date (§8.4) between `from` and `to` inclusive.

When a `tasks.date` hook is set, as in the example configuration, the plugin reads every file whole to date
it. The notes are sorted by date, then by relative path.

**Digest layout** (`week.lua:419-554`). Sections are separated by blank lines,
in this order:

1. `# <label>  <from> to <to>`, then a blank line, then
   `<N> notes, [[<weekly name>]]`.
2. `## Calendar (<count>)`, followed by the calendar lines (§12). This comes
   first because the calendar is enabled.
3. One section per note:
   - The heading `## <MM-DD Ddd>  [[<name>]]`, followed by ` -- <title>` when
     the note has a title that differs from its name. A title is a leading
     H1 that is the note's only H1.
   - An outline of its other headings, as `- text`, indented two spaces per
     level below the shallowest heading.
   - Up to 8 non-blank, non-heading body lines, each prefixed with `> `, then
     `> …` if there are more.
   - Frontmatter is skipped, and `#` lines inside fences are not headings
     (`week.lua:307-355`).
4. `## Finished this week (<n>)`: tasks done in the week, whatever their
   note, sorted by `done_at`.
5. `## Still open in this week's notes (<n>)`: open tasks in this week's
   notes, in `added` order.
   - Both task sections cover **tagged tasks only**, over all notes with no
     date window.
   - Each line is `- ✓ <(P) >text  ([[name#^id]])` or
     `- ☐ <(P) >text  ([[name]])`. The link includes `#^id` only when the
     task has an id.
   - These lines are deliberately **not** checkboxes.
6. The digest sources. In the example configuration this is `## Messages`:
   the output of the hypothetical command `mail-week <from> <to>`, or
   `(unavailable: <error>)` if the command fails.
7. **Looking ahead** (`ahead = 1`, the next week):
   - `## Coming up (<count>)  <from> to <to>`, with the calendar.
   - `## Due <from> to <to> (<n>)`: open tagged tasks due in the next week,
     sorted by due date.

The digest is a scratch buffer and is never written to disk by the plugin.

## 12. Calendar output (for `{{agenda}}` and the digest)

`calendar.lua:51-68,284-375`. ttykasten needs this only to produce these
placeholders or the digest.

**Command:**

```
gcalcli --nocolor agenda --tsv --details location --details id --calendar Personal FROM TO
```

- `TO` is exclusive.
- The TSV is parsed by its header row. An event with no `start_time` is an
  all-day event, and Google's end date for it is exclusive.
- The result is cached for 900 s in `~/.cache/nvim/fzfkasten/calendar/`.

**One event** is formatted as one of:

- `HH:MM-HH:MM  title  @location` (the location part only if present)
- `all day  title`
- `all day (MM-DD..MM-DD)  title` (spanning several days)

Within a day, all-day events come first, then timed events by start time,
then by title.

**`{{agenda}}`** is the events of that day, each line prefixed with `- `,
joined with `\n`.

- If the calendar is unavailable, it is the single line
  `- (calendar unavailable: <reason>)`.
- If the cache is stale, the line
  `- (calendar unavailable (<err>; as of MM-DD HH:MM))` is appended.

**`{{agenda_week}}`** covers the ISO week. Each day with events gets a header
line `MM-DD Ddd` followed by `- ` event lines. Days with no events are
omitted.

## 13. Nvim-only behaviour (ttykasten can skip)

The following have no file-level meaning:

- **fzf UI:** fzf/fzf-lua layout, previews, headers, prompts and key
  bindings. This includes `fzf_opts`, `notes.*.fzf_opts.cmd`, the panel, and
  the "Multiple matches" picker.
- **Romaji:** migemo, `/` queries, and the heading cache
  (`find.headings_stale_after`).
- **Buffers:** buffer marking (`note_buffer.*`: diagnostics, autoformat),
  filetype, cursor placement, `zz`, and `checktime` reloads.
- **Registers:** clipboard registers for YankLink and LinkToDaily.
- **The task list buffer** (`tasklist.lua`): its preview, colours, virtual
  text, key maps, sort cycling and `list.*` options. Its writes are the
  `_at` writers of §8.8.
- **Refusals tied to buffers:** the refusal to write a file whose nvim buffer
  has unsaved changes (`tasks.lua:1007-1011`), and the "not an editable
  buffer" refusal.
- **Undo:** Vim's `u` for buffer edits.
- **Other views:** the digest's folds and buffer, `:FzfKastenRecent`
  (mtime-ordered, `.git` pruned, `templates/` ignored, limit 50), the graph
  pickers and HTML export, SearchContent (live `rg`), and the Claude pane
  integration (`claude.*`).
- **Hooks:** `tasks.filter` and `on_collect`, which are unset in the example
  configuration.

## 14. Open questions

1. **ISO week in filenames.** *Resolved 2026-09-30: both tools use
   `%G-W%V`.* The plugin used `%Y-W%V`, which gave one ISO week two names
   around New Year: 2025-12-29 was `2025-W01` and 2027-01-01 was `2027-W53`.
   The plugin now defaults to `%G-W%V` and has `{{isoyear}}`, and the
   example weekly template's title is `{{isoyear}}-W{{week}}`.
2. **Locale.** `%A`, `%B` and `%a` follow the process's `LC_TIME`. The example
   configuration runs under `en_US.UTF-8`. Should ttykasten hard-code English, or follow the
   locale?
3. **NewNote on an existing name.** The plugin replaces the buffer content of
   an existing file with the template (`core.lua:138`). This is lost only if
   saved. Should ttykasten refuse, open the existing file, or overwrite?
4. **Buffer operations as file writes.** ttykasten's writes will normalise
   CRLF and BOMs (§0.4). The plugin's buffer operations would have preserved
   them. Is that acceptable?
5. **`tag_at` put `#todo` after the id.** *Resolved 2026-09-30:* `tag_at`
   now goes through `tag_line`, so the id stays last in every writer (§8.8).
6. **Uppercase `[X]`.** *Resolved 2026-09-30:* TaskCancel refuses `[X]` as
   done, like `[x]` (§8.8). It still toggles to open, and TaskDue still
   refuses it as "not open".
7. **Undo across tools.** The plugin's undo stack lives in memory in each
   nvim process. Should ttykasten keep a persistent journal (outside the
   collection), and should it refuse when the line is no longer byte-equal to
   what it wrote, as the plugin does?
8. **`has_tag` treated `_` and `-` as terminators**, so `#todo-x` and
   `#todo_list` counted as `#todo`. *Resolved 2026-09-30:* the right boundary
   is now `%f[^%w_-]`, matching the tag pattern (§8.5). The left side still
   has no boundary, so `foo#todo` counts, as the tag pattern reads it.
9. **Nesting ignored prose and headings**, so an indented checkbox under a
   new heading became a subtask of the previous section's last item.
   *Resolved 2026-09-30:* a non-list line at the margin empties the stack
   (§8.5).
10. **Due tokens** with trailing junk (`due:2026-09-1400`) are captured
    because of `[T%d:]*`. They also sort and compare as strings. Replicate
    this byte-for-byte?
11. **Link-created notes were not sanitised**, so `[[a:b]]` would have
    created `a:b.md`. *Resolved 2026-09-30:* they are sanitised like new
    notes, and an empty result is refused (§4).
