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

Next steps: see the future-work list in Progress; the plugin's note, task,
link and week features are covered.

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

2026-10-08: notes and search.

- `find` with romaji: a query starting with `/` switches fzf's own matching
  off and reloads the list with the notes whose path or headings match the
  regex `ttyskk migemo --flavour rg` makes; leaving the `/` switches it back
  (the plugin's shell binds, done with hidden subcommands).
- `grep` (SearchContent): rg as you type, ignore files disregarded; `/` for
  romaji; a query that is not a valid regex is searched as plain text.
- `tags` (SearchByTag): every tag with its count, then the lines carrying it.
- `recent`: notes by mtime, .git pruned, `recent.ignore_dirs` left out.
- `new` picks a template from `templates/` (default first; Esc = default).
- `rename` (§9.2): sanitises the new name, refuses an existing target or a
  note with unsaved changes in Neovim before touching anything, moves the
  file and rewrites every link to the old name.
- The link graph: `links` (tree), `orphans`, `dead-links`, `hubs`, built
  like graph.lua (fences and code spans skipped, self-links dropped).
- The week digest's ties now follow the plugin's new tie-breakers
  (done_at or due, then path, then line).
- Checked: on a real collection the graph (orphans, dead links, hubs and
  three trees) is identical to graph.lua's; rename is byte-identical to
  rename_note on a scratch collection; the real digests for three weeks are
  identical, ties included.

2026-10-18: filtering and grouping the task list (the plugin's c1bbd09).

- New keys in `tasks` and `inbox`: `alt-r` reverses the order (the steps of
  a job stay in order), `alt-x` now cycles open -> closed too -> closed only,
  `alt-t` keeps the tasks filed under one tag (chosen from the tags in view),
  `alt-e` cycles the due filter (overdue -> due by today -> due this week ->
  no due date -> off), `alt-g` groups by note, tag or due date.
- A step takes its job's tags and due date. In a grouped view each row starts
  with its group's name, since fzf has no rows that are not entries; a task
  with two tags is in both groups.
- Closed tasks are marked `✓` / `✗`, as the plugin now marks them.
- The view lives in the prompt (`tasks [due, reversed, #budget, overdue]> `)
  and is read back from $FZF_PROMPT. The tag chooser is a second fzf run
  through `execute`, which leaves the tag for the refresh to apply.
- Checked against the plugin's collect, group and entry_text on a real
  collection: 15 views (each filter, reverse, each grouping, combinations,
  the inbox) identical line for line.

2026-10-18 (later): every key configurable, and the agenda.

- The last fixed keys are settings: `log.date_key`, `week.digest_key` and
  `romaji.prefix` (the plugin hard-codes these). "" unbinds a key; the
  header leaves out the hint for an unbound one.
- `agenda [WEEKS]` (the plugin's calendar.pick): the events of a week or a
  span (`0..1`, `-1..0`), a line for today, past events dimmed. Enter opens
  that day's daily note; `calendar.keys.copy` copies the picked events'
  lines (the plugin's alt-i puts them into the buffer; a terminal has none);
  `calendar.keys.refresh` runs the command again past the cache.
- Checked against the plugin's span and rows with a fake gcalcli: three
  specs, identical.

2026-10-18 (evening): `doctor` (the plugin's :checkhealth).

- Reads the config itself, so a config that does not parse is reported
  (with the checks run on the defaults) instead of stopping the run.
- Checks: the config file and unknown settings, home and its note count,
  fzf (0.46 or newer, for `transform` and $FZF_PROMPT), rg, the editor, the
  pager and clipboard commands, the templates, the task regexes, the
  capture note and `always`, the menu (unknown actions, items with neither
  action nor command, a key used twice), the swap directory, the calendar
  (gcalcli, its token's permissions, the names) and the romaji search
  (prefix, and the time one query takes, which is the typing latency).
- Exits 1 when it found an error. Its sentences are kept in the module in
  both languages rather than in the message tables, since nothing else
  uses them.
- Romaji in the task list (the plugin's alt-/): as in `find`, a query
  starting with `romaji.prefix` switches fzf's matching off and reloads the
  list with the tasks whose text the romaji matches. The keys' reloads
  carry the query, so the list stays narrowed after an action.
- Tried in a real fzf, driven through tmux: the romaji query and back, the
  tag chooser, every view key, done and undo.

2026-10-18 (night): notes in plain Vim or Neovim (`contrib/ttykasten.vim`).

- ttykasten offers commands an editor can call: `--print` (what would open
  in the editor is printed as `PATH<tab>LINE`, links printed instead of
  copied), `keys` (the menu's keys), `link-at FILE COL` (the plugin's
  follow_link: the link under a column, its note, the `^id` line or the
  heading it names; `follow_link.create_nonexisting` as in the plugin) and
  `toggle`, `cancel`, `tag-line`, `due` on FILE LINE.
- The Vim script maps `<leader>` + every menu key, and in markdown buffers
  gf and the line's task actions. fzf runs with `:!` in Vim and in a
  terminal tab in Neovim (whose `:!` has no terminal); the answer comes
  back through a temporary file.
- `insert-link` (the plugin's insert_link): pick a note, get `[[note]]`.
- The swap guard also reads Vim's default swap beside the note
  (`.note.md.swp`); Vim's block 0 is the same as Neovim's.
- Tried in `nvim --clean` and `vim -u NONE` (9.2) through tmux: gf (id,
  heading, a directory, duplicate names, a missing note, no link), each
  line action, insert-link, menu keys, the menu, the digest, and an action
  that fails. ttykasten's stderr goes to a file and comes back as Vim
  messages, since the terminal's copy is gone once Vim redraws (fzf draws
  on /dev/tty, not stderr).

2026-10-18 (late): `note`, the plugin's note panel.

- Pick a note, then: open, the lines linking to it, what it is linked
  with, rename, delete. rename, links and backlinks were split into a
  picker and an action on a given note, which the panel calls.
- Delete differs from the plugin, which removes the file: ttykasten says
  how many lines link to the note, asks for "yes", refuses while it has
  unsaved changes in Vim or Neovim, and moves it to the trash (`gio trash`,
  macOS's `trash`, or the `trash` setting). With no trash command it
  refuses; it never unlinks a note. gio does not trash files on internal
  mounts such as /tmp, so there the deletion fails and the note stays.

2026-10-18 (late, later): a menu that waits for a key.

- Typing `k` (the shell alias) and then `kd` in the menu was one `k` too
  many. The menu now strips the start every key shares (`k`) and waits for
  a key, like `<leader>k` in Neovim with which-key: `d` runs the item at
  once. Search is off meanwhile, so the list keeps its order and each key
  is `pos(N)+accept`; a search key (`menu.search_keys`, space and `/`)
  unbinds them, clears the query and turns search on. `menu.hotkeys =
  false` gives the old menu. Only one-character remainders are keys; a
  longer one is reached by searching.
- `menu.rest = true` lists the actions a hand-made menu leaves out after
  its items.

2026-10-19: checked by hand end to end, and 0.5.0.

- Every action was tried on the real collection and on a copy: the menu,
  tasks, agenda, the note panel, the read-only lists and the Vim script in
  Vim and Neovim.
- `doctor` says when home comes from `$TTYKASTEN_HOME` or
  `$ZETTELKASTEN_HOME`: a variable left set in one shell sent a check to
  the test copy, and nothing said so.
- Tags are coloured as soon as a note is drawn: in fzfkasten.nvim for every
  note under home (`note_buffer.highlight_tags`), and in
  `contrib/ttykasten.vim` for plain Vim and Neovim. A markdown language
  server (marksman) colours them only after reading the collection, a
  second or two after the fresh Neovim that ttykasten starts.
- `contrib/ttykasten.vim` loads under `vim -u NONE`: it sets Vim's
  'cpoptions' while it loads, for its line continuations.
- The README says what a digest source command gets and gives back.
- format.md Q12 (numbers and URL fragments read as tags) stays open.
