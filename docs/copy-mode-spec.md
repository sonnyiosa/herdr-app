# Copy mode spec

Copy mode lets a user move through a terminal's screen and scrollback with the keyboard, select text, search, and copy, without the mouse. The behavior follows copy mode in the Herdr TUI (`prefix+[`).

## Terms

- **Copy cursor.** The cell that copy mode moves. It is separate from the program's terminal cursor.
- **Screen row.** A row counted from the top of the scrollback, so the number does not change when the viewport scrolls. Ghostty calls this `GHOSTTY_POINT_TAG_SCREEN`.
- **Anchor.** The cell where a selection starts. The copy cursor is the other end.
- **Charwise selection.** Selects from the anchor cell to the copy cursor cell, across rows.
- **Linewise selection.** Selects every full screen row from the anchor row to the copy cursor row.

## Scope

In scope:

- One terminal pane at a time. Each `TerminalView` keeps its own copy-mode state.
- The normal screen and the alternate screen. The alternate screen has no scrollback.
- Charwise and linewise selection.
- Literal search forward and backward, using the existing Ghostty search engine.

Out of scope:

- Block (rectangle) selection.
- Changing the keys inside copy mode from `keys.json`. Only the key that starts copy mode can be changed.
- Changes to herdr-core.

## Start and stop

- The `terminal.copy_mode` command starts copy mode in the focused terminal. The default key is the two-key sequence `ctrl-f enter` on all platforms. It is a GPUI two-key binding in the `terminal` section of `keys.json`.
- On start, the copy cursor goes to the terminal cursor if that cursor is in the viewport. If it is not, the copy cursor goes to the bottom-left cell of the viewport. The app keeps the viewport scroll offset as it was at the start.
- When copy mode stops, the viewport goes back to that offset and the copy cursor goes away.
- The command runs on a terminal that is already in copy mode. It does not change the state.
- `q` stops copy mode. `Escape` clears an active selection or search first. When neither is active, `Escape` stops copy mode.
- `y` and `Enter` copy the selection to the clipboard, with `TerminalView::copy`, and stop copy mode. With no selection, they copy the current search match. With no selection and no match, they only stop copy mode.
- A mouse press in the pane and the closing of the terminal also stop copy mode.

## Keys in copy mode

While copy mode is active, no input goes to the program. A key that has no copy-mode meaning does nothing. App keys from `keys.json` (for example `cmd-t`, `cmd-c`) work as before. These terminal keys change in copy mode:

- `terminal.paste` adds the clipboard text to the query while the search prompt is open. When the prompt is closed, it does nothing.
- A `send:` key does nothing.

| Keys | Action |
|---|---|
| `h` `j` `k` `l`, arrow keys | Move one cell or one row |
| `0`, `Home` | Go to column 0 |
| `^` | Go to the first non-blank cell in the row |
| `$`, `End` | Go to the last non-blank cell in the row |
| `w` `b` `e` | Go to the next word start, the previous word start, the next word end |
| `W` `B` `E` | The same for words that only whitespace separates |
| `{` `}` | Go to the previous or next blank row |
| `g`, `G` | Go to the first or the last screen row |
| `ctrl-u`, `ctrl-d` | Move half a viewport up or down |
| `ctrl-b`, `ctrl-f`, `PageUp`, `PageDown` | Move one viewport up or down |
| `v`, `space` | Start or stop a charwise selection |
| `V` | Start or stop a linewise selection |
| `/`, `?` | Start a forward or backward search prompt |
| `n`, `N` | Go to the next match or the previous match, relative to the last search direction |
| `y`, `Enter` | Copy and stop |
| `q`, `Escape` | Stop (see above) |

Motion rules:

- A word is a run of cells of one class. The classes are whitespace, punctuation (ASCII characters in ``!"#$%&'()*+,-./:;<=>?@[\]^`{|}~``) and other text. `W`, `B` and `E` use only two classes, whitespace and non-whitespace.
- Word motions follow rows that the terminal wrapped, so they treat a wrapped line as one line.
- A wide character is one stop for the copy cursor. The copy cursor is never on the spacer cell of a wide character.
- Vertical motion keeps the column when it can and moves it to the last cell of a shorter row.
- When the copy cursor moves out of the viewport, the viewport scrolls the minimum distance that shows it again.

## Selection

- `v` sets the anchor to the copy cursor. The selection then follows the copy cursor.
- `V` selects full rows from the anchor row to the copy cursor row.
- Pressing `v` during a charwise selection, or `V` during a linewise selection, clears it. Pressing the other key changes the selection kind and keeps the anchor.
- The app sets the selection on the Ghostty terminal, so Ghostty draws it and `selection_text` reads it in the same way as a mouse selection.

## Search

- `/` or `?` shows a prompt in the copy-mode bar. Typed characters, `Backspace` and paste edit the query. `Enter` runs the search. `Escape` closes the prompt and keeps the previous query.
- Copy-mode search uses the same Ghostty search as the find bar. Matching is literal and byte-exact, except that ASCII letters match without case. Matches show with the same highlight as the find bar.
- After `Enter`, the copy cursor goes to the first match in the search direction, from the copy cursor.
- `n` repeats the search in its direction. `N` searches in the other direction.
- When a selection is active, it extends to the match.

## Copy-mode bar

A bar at the top-right of the pane, in the same place as the find bar, shows:

- `COPY`, or `COPY · select` or `COPY · line` when a selection is active.
- The search prompt (`/query` or `?query`) while the user types a query.
- `n/N  i/total` when a search has matches.

The copy cursor shows as a filled cell in the accent color, at the opacity of the selected search match. The program's terminal cursor is not drawn while copy mode is active.

## Output during copy mode

- Output from the program continues to go to the terminal.
- The copy cursor and the anchor stay on the same content when output scrolls it. When scrollback is trimmed and their row no longer exists, they move to the first existing row.
- If the viewport is at the bottom, it continues to follow the output, as it does outside copy mode.

## Agent-facing changes

- `terminal.copy_mode` is a new command in `src/commands.rs`. It shows in the palette, in `herdr-app do`, and to mods through `run`.
- `defaults/AGENTS.md` and `src/SKILL.md` describe the command and the default keys.
