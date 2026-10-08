# ttykasten

Zettelkasten notes and tasks from the terminal, with no editor plugin in the
way. ttykasten reads and writes the same collection as
[fzfkasten.nvim](https://github.com/barewalker/fzfkasten.nvim), byte for byte,
so the two can be used side by side on one set of Markdown files.

Run `ttykasten` with no arguments and every action is listed in
[fzf](https://github.com/junegunn/fzf). Writing is left to your `$EDITOR`.

```sh
ttykasten               # the menu
ttykasten daily         # today's daily note, created from its template if missing
ttykasten capture       # capture a one-line task (typed into an input line)
ttykasten tasks         # the task list, read from the notes themselves
ttykasten find          # find a note, with a coloured preview
ttykasten link          # pick any line of any note; copy [[note#^id]]
```

Status: early, but it covers the plugin's note, task, link and week
features (see [docs/design.md](docs/design.md)).

## Requirements

- [fzf](https://github.com/junegunn/fzf) and [ripgrep](https://github.com/BurntSushi/ripgrep) on `PATH`
- Rust 1.88 or later to build
- Optional: [ttyskk](https://github.com/barewalker/ttyskk) for romaji search,
  [gcalcli](https://github.com/insanum/gcalcli) for the calendar
- Linux or macOS (WSL works)

## Install

```sh
cargo install --git https://github.com/barewalker/ttykasten
```

## Actions

| Action | What it does |
|---|---|
| (none) | show the menu |
| `daily` | open today's daily note, creating it from its template if missing |
| `weekly` | open this week's weekly note (ISO week, `%G-W%V`) |
| `capture [TEXT]` | append a task to the capture note |
| `tasks` | the task list: open tasks carrying `require_tag`, subtasks under their parent; `alt-s` sorts by priority, due date or when added; enter opens the task's line |
| `inbox` | checkboxes without the tag, so nothing captured is lost |
| `capture-guided` | capture a task, then pick tags from the collection and give a due date |
| `tag` | pick lines (tab marks several) and turn them into tasks: `- [ ] … #todo` |
| `mint-ids` | pick a note and give each tagged task in it a `^id` |
| `undo` | undo the last task edit, even from an earlier run |
| `week [WEEK]` | the notes dated in a week; `ctrl-d` shows its digest. `WEEK` is empty (this week), `-1`, `2026-W37` or a date |
| `digest [WEEK]` | the week digest: each note's outline and opening lines, the calendar, tasks finished and still open, extra sources, and what is coming up next week |
| `find` | find a note by path and open it; a query starting with `/` is romaji, matched against paths and headings (`/kaigi` finds 会議) |
| `grep` | search note contents as you type; `/` for romaji here too |
| `tags` | every `#tag` with its count; pick one to list the lines carrying it |
| `recent` | notes by when they were last changed |
| `new [TITLE]` | create a note from a template (picked from `templates/`) and open it |
| `rename` | rename a note and rewrite every link to it in every note |
| `log` | list recent daily and weekly notes; `ctrl-x` picks a date |
| `link` | pick a line of any note; give it a `^id` if it has none; copy `[[note#^id]]` |
| `link-daily` | the same, and append `- [[note#^id]]` to today's daily note |
| `links` | pick a note and see what it is linked with, a few steps deep (→ out, ← in, ↔ both) |
| `orphans` | notes with no link in or out |
| `dead-links` | links that no note answers to |
| `hubs` | notes by how many links meet there |
| `backlinks` | pick a line and list the lines linking to it (or, for a line with no id, to its note); open one at its line |
| `config-example` | print an example config file |
| `--version` | print the version and the commit it was built from |

Leave out `TEXT` or `TITLE` to type it into an input line.

In `tasks` and `inbox`, keys act on the task under the cursor and the list
reloads in place: `ctrl-x` done/open, `ctrl-d` cancel/reopen, `ctrl-t` add
the tag, `alt-l` due date (`fri`, `+3d`, `tomorrow`, `2026-10-02`; blank
clears), `alt-a` capture, `alt-u` undo, `alt-s` sort, `alt-x` show done and
cancelled tasks. All keys are configurable (`[tasks.keys]`). An action
refuses when the line changed after the list was drawn, and every write is
kept in an undo journal outside the collection.

ttykasten writes note files directly. Before each write it reads Neovim's
swap file for that note and refuses if the note is open in a running Neovim
with unsaved changes, so neither side's edits are lost (`nvim_swap_dir`).

Links point at a line, not only at a note: `[[note#^id]]` finds its line
wherever it moves, because the id travels with the line. `link` copies the
link through the terminal (OSC 52) by default, so it works over ssh and mosh;
set `clipboard` to `"none"` or to a command such as `"wl-copy"` to change
that. Subcommands exist
so that key bindings (tmux, herdr, shell aliases) can call actions directly.

## Configuration

ttykasten reads `~/.config/ttykasten/config.toml` on every start (or
`$TTYKASTEN_CONFIG`, or `$XDG_CONFIG_HOME/ttykasten/config.toml`), so changes
take effect without rebuilding. Option names and defaults follow
fzfkasten.nvim's `setup()`: to share a collection, give both the same values.

```toml
language = "en"            # or "ja"
home = "~/zettelkasten"
hdate_format = "%A, %B %d, %Y"
new_note_template = "templates/template_new_note.md"

[notes.daily]
dir = "journal"
template = "templates/daily.md"

[tasks]
require_tag = "todo"
always = ["tasks/active.md"]   # the first entry is where `capture` writes

[block_id]
on_capture = true

# The menu: items, order, key hints and labels are all yours.
[[menu.items]]
action = "daily"
key = "kd"                 # `ttykasten kd` works too

[[menu.items]]
label = "git status"
command = "git status; read -r _"
```

`ttykasten config-example` prints every option with its default
([config.example.toml](config.example.toml)).

## Calendar

`{{agenda}}` and `{{agenda_week}}` in templates, and the digest's calendar
sections, read events through
[gcalcli](https://github.com/insanum/gcalcli) (or any command printing its
TSV) when `[calendar] enabled = true`. Results are cached for 15 minutes,
and when the command fails an older result is shown, marked with when it
was fetched.

## Compatibility

[docs/format.md](docs/format.md) specifies the on-disk format that both tools
share: file names, templates and placeholders, task lines, block ids and
links. Every write normalises the file the way Neovim's `readfile` /
`writefile` do (LF line endings, no BOM, a final newline).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
