# Copy mode plan

This plan builds the behavior in `docs/copy-mode-spec.md`. Each step ends with a check that must pass before the next step starts.

## Terms

- **`Vt`.** The local libghostty-vt terminal in `src/vt.rs`. It holds the screen, the scrollback, the selection and the search.
- **`TerminalView`.** The GPUI terminal pane in `src/terminal.rs`.
- **Screen point.** A `(row, col)` pair, where `row` is a screen row counted from the top of the scrollback.

## Data shape

The copy-mode state lives in a new module, `src/copy_mode.rs`. `TerminalView` holds `copy: Option<CopyMode>`, where `None` means copy mode is not active.

```rust
pub struct Point { pub row: u32, pub col: u16 }

pub enum Selection {
    Chars { anchor: Point },
    Lines { anchor_row: u32 },
}

pub enum Direction { Forward, Backward }

pub struct Prompt { pub direction: Direction, pub query: String }

pub struct CopyMode {
    pub cursor: Point,
    pub selection: Option<Selection>,
    pub prompt: Option<Prompt>,
    pub last_search: Option<Direction>,
    pub entry_offset: usize,
}
```

Motions are pure functions over a small read trait, so tests can run them without Ghostty:

```rust
pub trait Rows {
    fn rows(&self) -> u32;
    fn cols(&self) -> u16;
    fn row(&self, y: u32) -> Option<Row>; // text, cell spots with width, wraps flag
}

pub fn motion(rows: &impl Rows, from: Point, m: Motion) -> Point;
```

`Vt` implements `Rows`. `Motion` is one enum with a variant for each motion key in the spec.

## Steps

### 1. Get the Ghostty source

- [ ] Run `git submodule update --init --depth 1 vendor/ghostty` (commit `291fd050`) and build once with `cargo build`.

The Ghostty headers in `include/ghostty/vt/` at that commit supply the calls that copy mode needs:

- `GhosttySelection` (`selection.h`) is two grid refs, `start` and `end`, and a `rectangle` flag. The app sets it with `GHOSTTY_TERMINAL_OPT_SELECTION`. There is no linewise flag, so a linewise selection goes from column 0 of the first row to the last column of the last row.
- Tracked grid refs (`grid_ref_tracked.h`) move with the content. They report when their row is no longer there.
- `GHOSTTY_SEARCH_DATA_MATCHES` (`search.h`) gives all matches, from newest to oldest. `GHOSTTY_SEARCH_DATA_SELECTED_MATCH` gives the selected match. The Ghostty search cannot change its case rule.
- `GHOSTTY_SCROLL_VIEWPORT_ROW` (`terminal.h`) scrolls the viewport to a row.

### 2. Add the `Vt` read and write calls

- [ ] Change `link_row` and `wraps` in `src/vt.rs` into one `pub fn row(&self, y: u32) -> Option<Row>`. Keep `link_at` working on top of it.
- [ ] Add `pub fn viewport_top(&self) -> u32`, `pub fn screen_rows(&self) -> u32` and `pub fn cursor_point(&self) -> Option<Point>`. `cursor_point` returns `None` when the cursor is not in the viewport.
- [ ] Add a tracked point type that wraps a tracked grid ref. It gives the current screen point, or the first row when its row is no longer there. Use it for the copy cursor and the anchor.
- [ ] Add `pub fn set_selection(&mut self, start: Point, end: Point, lines: bool)`, with `rectangle` set to `false`.
- [ ] Add `pub fn matches(&self) -> Vec<(Point, Point)>` from `GHOSTTY_SEARCH_DATA_MATCHES`, so that a search can start from the copy cursor.
- [ ] Add `pub fn show_row(&mut self, row: u32)`. It uses `GHOSTTY_SCROLL_VIEWPORT_ROW` to scroll the minimum distance that makes the row visible.
- [ ] Check: `vt.rs` tests feed bytes with wide characters, wrapped lines and more than one screen of output. They make sure that `row`, `set_selection` with `selection_text`, `matches` and `show_row` give the expected text and offsets. A test trims the scrollback and makes sure that a tracked point moves to the first row. The existing link tests must still pass.

### 3. Build the model and the motions

- [ ] Write `src/copy_mode.rs` with the data shape above. Port the word, big-word and paragraph rules from the TUI (`/tmp/herdr/src/pane/terminal.rs`, `word_motion` and `paragraph_motion_target`).
- [ ] Check: unit tests run each motion on a fake `Rows`. Cover word classes, punctuation, wrapped lines, wide characters, the first and last row, short rows during vertical motion, and an empty scrollback.

### 4. Connect copy mode to `TerminalView`

- [ ] Add `copy: Option<CopyMode>` to `TerminalView`. Add `enter_copy_mode` and `exit_copy_mode`, which follow the start and stop rules in the spec.
- [ ] In `key_down`, when `copy` is `Some`, send the key to a copy-mode handler and call `cx.stop_propagation()`, before `key_bytes`. The handler updates the state, calls `set_selection` and `show_row`, and calls `cx.notify()`.
- [ ] While copy mode is active, `send` sends nothing to the program. `paste` adds the clipboard text to the query when the search prompt is open, and does nothing when the prompt is closed.
- [ ] Stop copy mode on a mouse press in the pane and when the terminal closes.
- [ ] Add `terminal.copy_mode` to `COMMANDS` in `src/commands.rs` and to the dispatch in `src/window.rs` (near `terminal.search`) and `src/quick_terminal.rs`.
- [ ] Check: `#[gpui::test]` tests in `src/terminal.rs`, next to the find bar tests, cover start and stop, the scroll offset restore, `v` and `V` selection with `y` copying the expected clipboard text, keys and `send:` keys that do not reach the program, and paste that does not reach the program.

### 5. Add search

- [ ] Handle the `/` and `?` prompt in the copy-mode key handler. On `Enter`, call `start_search` and `set_search_needle`. When the search is complete, move the copy cursor to the first match in `matches` in the search direction from the copy cursor.
- [ ] Make `n` and `N` move the copy cursor to the next match in `matches` in the given direction, with wrap. Extend an active selection to the match.
- [ ] Make `y` with no selection copy the current match.
- [ ] Make sure the find bar and copy-mode search do not both own the search. Starting copy mode closes the find bar.
- [ ] Check: gpui tests for forward and backward search, `n` and `N` wrap, `y` on a match, paste into the prompt, and `Escape` from the prompt.

### 6. Draw copy mode

- [ ] In `paint_grid`, draw the copy cursor quad after the search highlights, and do not draw the terminal cursor while copy mode is active.
- [ ] Draw the copy-mode bar in the place of the find bar, with the content that the spec gives.
- [ ] Check: run the app with an isolated app home. Take screenshots of the copy cursor on a wide character, a charwise selection, a linewise selection, the search prompt and the match count.

### 7. Keys, docs and release notes

- [ ] Copy the current `defaults/keys.json` and `defaults/keys.macos.json` into `src/keys/shipped/` as `v0.1.13-keys.json` and `v0.1.13-keys.macos.json`, and add them to the list in `src/keys.rs`. Then add `"ctrl-f enter": "terminal.copy_mode"` to the `terminal` section of both files.
- [ ] Check that `keys::parse` accepts a two-key sequence. Add a test that `ctrl-f` followed by another key still reaches the program as `ctrl-f` and then that key, in that order.
- [ ] Describe `terminal.copy_mode`, its keys and the keys in copy mode in `defaults/AGENTS.md` and `src/SKILL.md`.
- [ ] Add a CHANGELOG entry under the next version.
- [ ] Check: `just test` and `just docs-check` pass. The shipped-keys upgrade test passes with the new originals.

### 8. Test the real app

- [ ] Build and start the macOS app with an isolated app home and core. In a shell with more output than one screen, and in an alternate-screen program such as `less`, do these steps: start copy mode with the default key, move with each motion group, select, search, copy, and paste the clipboard into another program to check the text. Let output arrive while copy mode is active, with the viewport at the bottom and scrolled up.
- [ ] Check: every step in the spec behaves as written. Record any difference as a spec change or a bug before you close the work.
