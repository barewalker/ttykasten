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
ttykasten find          # find a note, with a coloured preview
ttykasten link          # pick any line of any note; copy [[note#^id]]
```

Status: an early prototype. The task list and the week digest are planned (see [docs/design.md](docs/design.md)).

## Requirements

- [fzf](https://github.com/junegunn/fzf) and [ripgrep](https://github.com/BurntSushi/ripgrep) on `PATH`
- Rust 1.88 or later to build
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
| `find` | find a note by path and open it |
| `new [TITLE]` | create a note from the template and open it |
| `log` | list recent daily and weekly notes; `ctrl-x` picks a date |
| `link` | pick a line of any note; give it a `^id` if it has none; copy `[[note#^id]]` |
| `link-daily` | the same, and append `- [[note#^id]]` to today's daily note |
| `backlinks` | pick a line and list the lines linking to it (or, for a line with no id, to its note); open one at its line |
| `config-example` | print an example config file |

Leave out `TEXT` or `TITLE` to type it into an input line.

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

## Compatibility

[docs/format.md](docs/format.md) specifies the on-disk format that both tools
share: file names, templates and placeholders, task lines, block ids and
links. Every write normalises the file the way Neovim's `readfile` /
`writefile` do (LF line endings, no BOM, a final newline).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
