# ttykasten design notes

ttykasten takes the features of fzfkasten.nvim out of Neovim and into the
terminal. It moves in the same direction as the move from skkeleton to ttyskk:
Neovim stays the editor you write in, while what knows about notes and tasks
lives outside it, in a single executable.

## What to keep from earlier tools, and what to avoid

| Tool | What worked | Why it was dropped |
|---|---|---|
| zk (zk-org) | — | Notes have to be typed as command-line arguments, and the commands have to be memorised. |
| Logseq | Links can point at a block (a line) inside a note, not only at the note. | Heavy. Does not run in a terminal. |
| fzfkasten.nvim | `<leader>k` lists every action, so rarely used ones stay reachable, while frequent ones get their own keys that the hand learns. Writing happens in Neovim. | Nothing works without opening Neovim. |

## Principles

1. **One entry point.** Run `ttykasten` with no arguments and every action is
   listed in fzf with a description (the counterpart of the `<leader>k`
   panel). Nothing needs to be memorised.
2. **Frequent actions get their own keys.** A terminal multiplexer's key
   bindings (herdr, tmux) or shell aliases call them directly, bypassing the
   menu. These are what the hand learns.
3. **Never make the user type a note body as an argument.** Writing is handed
   to `$EDITOR`. One-line input, such as capturing a task, is taken in an
   input line shown after start-up. Subcommands and arguments exist as an
   interface for key bindings and for scripts or agents; people are not
   expected to learn them.
4. **Block links come first.** Pick any line of any note with fzf, give it a
   `^id`, and get `[[note#^id]]`; write it into today's daily note; find the
   links that point at a line — all as one flow. The syntax is fzfkasten's
   `^id`.
5. **Stay light.** One operation is one process, and full-text search runs
   `rg` once. File operations are expensive on WSL. An index is considered
   only if this turns out to be slow in practice.
6. **Exactly the same format as fzfkasten.nvim.** Both tools can read and
   write the same collection, side by side. The specification is
   [format.md](format.md).

## Scope of the first prototype

- The menu (principle 1)
- new: create a note (the title comes from the input line; the note is made
  from a template and opened in `$EDITOR`)
- capture: capture a task into the capture note
- daily: today's daily note (created from its template if missing)
- log: list and create daily and weekly notes
- find: find notes

Next steps: notes and search (romaji find, full-text search, tags, recent
notes, template picker, rename, link graph). (Block links, tasks and the
week digest are done: see Progress.)

## Language

Rust, like ttyskk: a single executable that is easy to distribute and fast on
WSL. fzf and rg are called as external commands.

## Progress

2026-09-30: the first prototype works: the menu, `new`, `capture`, `daily`,
`weekly`, `log` and `find`. Daily and weekly notes, new notes and task capture
were checked byte for byte against the plugin run under `nvim --headless`
(with the random block ids masked).

Decisions on the open questions in format.md:

- Q1: weekly notes are named by ISO year, `%G-W%V`. The plugin changed from
  `%Y-W%V` on the same day, and weekly templates use `{{isoyear}}`.
- Q2: day and month names written into notes are always English.
- Q3: if a note with the same name already exists, it is opened as it is,
  not overwritten from the template.

Differences from the plugin:

- `new` skips the template picker and always uses `new_note_template`.
- `{{agenda}}` and `{{agenda_week}}` are left unsubstituted.
- `find` has no romaji (migemo) search yet.

2026-09-30 (later): a config file. `~/.config/ttykasten/config.toml` is read
on every start, so changes need no rebuild.

- Option names and defaults follow fzfkasten.nvim's `setup()` (home `~/notes`,
  daily notes in `daily/`, and so on). Personal values go in the config file.
  `config.example.toml` lists every option (`ttykasten config-example` prints
  it).
- `language = "en" | "ja"` chooses the language of messages; English is the
  default. Day and month names written into notes stay English regardless,
  for format compatibility.
- The menu's items, their order, key hints and labels come from the config.
  No key is hard-coded. A configured key also works as an action name
  (`ttykasten kd`). A `command` item runs any shell command (the counterpart
  of the plugin's `panel.items`).
- The capture note (`tasks.capture_note` / `always`), the tag added on
  capture (`require_tag`), block ids (`block_id`), fzf arguments and preview
  colours are all configurable too.

2026-09-30 (later still): block links (principle 4).

- `link` lists every non-blank line of every note in fzf, with the line
  highlighted in the preview. The picked line keeps its `^id`, or gets a new
  one written into the note (YankLink, format.md §9.3), and `[[note#^id]]`
  is copied. `block_id.alias` adds the plugin's `|alias`.
- `link-daily` also appends `daily_bullet` + the link to today's daily note,
  creating it from its template if needed (LinkToDaily, §9.4; the title is
  `notes.daily.format`, as in the plugin's code).
- `backlinks` picks a line and lists the lines linking to it: to the line
  when it has an id (including `[[#^id]]` inside its own note), otherwise to
  its note (§10). Enter opens the editor at that line (`editor_line`).
- A terminal has no registers, so "yank" means the clipboard: `clipboard`
  is "osc52" by default (works over ssh and mosh), "none", or a command.
- Checked byte for byte against the plugin's YankLink and LinkToDaily under
  `nvim --headless`, with the daily note both missing and existing.

2026-09-30 (evening): the task list and inbox, read-only (step 1 of tasks).

- `tasks` and `inbox` scan the notes the way the plugin's `collect` does
  (format.md §8.3–§8.7): notes with a checkbox (rg, honouring .gitignore),
  frontmatter opt-out and ignored dirs, scannable lines, the nesting stack
  with `require_tag` inheritance and the margin rule (Q9), note dates, aging
  (`since_days`, `always`), and sorting by the root task.
- The plugin's `tasks.date` hook is Lua, so ttykasten has `date_patterns`
  (regexes on a note's first `date_lines` lines) in its place.
- Entries read as the plugin's `entry_text`: indent and `↳` for subtasks,
  `(P)`, progress `[closed/total]`, `[due …]`, and `← context` for a task
  whose parent is not shown. `alt-s` cycles the order through fzf's
  `transform` and `reload`, via two hidden subcommands.
- Checked against the plugin's `collect` + `entry_text` on a real collection:
  task list and inbox, in all three orders, identical line for line.

2026-10-07: acting on tasks (the rest of the task work).

- In `tasks` and `inbox`, keys act on the task under the cursor: done/open,
  cancel/reopen, tag, due date, guided capture, undo, sort, show closed.
  The rewrites are the plugin's toggle_line, cancel_line, tag_line (as
  tag_at uses it) and due_line, with the id kept last; `resolve_due` reads
  today, tomorrow, +3d, 2w, fri, 明日, 金 and so on.
- Each list row carries a fingerprint of its line; an action refuses when
  the line has changed since the list was drawn.
- fzf runs the actions through `transform` (which also reloads the list and
  puts the outcome first in the header, so a narrow window cuts the key
  help, not the message). Actions that need input (due date, capture) run
  through `execute` and leave their outcome in a status file for the
  refresh to show.
- Undo is a journal outside the collection (format.md Q7): every write by
  a task action, capture, `tag` and `mint-ids` is recorded, and undo refuses
  a line that changed since.
- `tag` is TaskTag over picked lines (tab marks several), honouring the
  scannable rules and `block_id.on_tag`; `mint-ids` is the plugin's
  BlockIds over a picked note.
- Checked byte for byte against the plugin's toggle_at, cancel_at, due_at,
  tag_at, add (guided capture), TaskTag and BlockIds under nvim --headless.

2026-10-07 (later): no writing under an unsaved Neovim buffer.

- The plugin refuses to write a file whose buffer has unsaved changes in
  its own Neovim; ttykasten cannot see Neovim's buffers, so it reads the
  swap file instead (Vim's block 0: the pid at bytes 24..28, `U` at byte
  1007 while the buffer is modified, cleared on `:w`). Before every write
  of a note, a swap file for it whose process is alive and whose buffer is
  modified makes ttykasten refuse. In the task list the reason shows in the
  header; undo keeps the entry so it can be retried after saving.
- `nvim_swap_dir` overrides the location; "" turns the check off.
- Verified with a real Neovim: refused while unsaved, written after `:w`.

2026-10-07 (evening): the week view, the digest and the calendar.

- `week [SPEC]` lists the notes dated in a week (§11); `digest [SPEC]` builds
  the plugin's digest_lines: notes with outline and excerpt, the calendar,
  tasks finished in the week and open in its notes, sources, and the week
  ahead. On a terminal it goes through `pager`, coloured; piped, it is the
  Markdown itself.
- The calendar (§12) runs gcalcli (or `calendar.command`), parses its TSV by
  header, caches it for `cache.ttl`, and falls back to a stale cache with
  "(… as of MM-DD HH:MM)". `{{agenda}}` and `{{agenda_week}}` are filled,
  and like the plugin they say "calendar unavailable: …" when it is off.
- The plugin's digest sources are Lua functions; here they are shell
  commands with {from} {to} {ahead_from} {ahead_to}.
- Checked against the plugin with a fake gcalcli and a fake mail command:
  the digest and a daily note using {{agenda}} / {{agenda_week}} are
  identical. On a real collection (calendar and sources off), three weeks
  had the same lines; two differed only in the order of tasks with equal
  sort keys (the same done_at or due), because Lua's table.sort is not
  stable. ttykasten keeps them in "added" order. A tie-breaker in the plugin
  (path, then line) would make both orders the same.

