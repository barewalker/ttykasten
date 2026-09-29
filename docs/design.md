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

Next steps: block links (principle 4), the task list and inbox, due dates,
turning a line into a task, and the week digest.

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
