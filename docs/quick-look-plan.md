# Quick Look plan

This plan builds `docs/quick-look-spec.md`. The change is macOS-only. It touches `src/terminal.rs`, a new `src/quick_look.rs`, `Cargo.toml`, `README.md` and `defaults/AGENTS.md`.

## Terms

- **Link gesture.** `LinkGesture`, the action that a click's modifiers ask for: `Open` or `Preview`.
- **Preview target.** A `LinkTarget::External(links::Target::File(_))`. Only a preview target accepts the `Preview` gesture.
- **Quick Look panel.** `QLPreviewPanel::sharedPreviewPanel`.

## Data shape

Today `link_mods(&Modifiers) -> bool` in `src/terminal.rs` answers one question: "do these modifiers open links?" Three call sites use it: the hover check in `render`, the left mouse-down handler, and the mouse-move handler. A second boolean for preview would have to stay in sync with the first at all three sites. One enum replaces the boolean:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinkGesture {
    Open,
    Preview,
}

/// What a click with these modifiers does to a link, if anything.
fn link_gesture(m: &Modifiers) -> Option<LinkGesture>;

impl LinkGesture {
    fn accepts(self, target: &LinkTarget) -> bool;
}
```

The modifier table is the whole rule:

| Platform | Modifiers held | Gesture |
|---|---|---|
| macOS | Cmd | `Open` |
| macOS | Cmd+Shift | `Preview` |
| Linux, Windows | Ctrl | `Open` |
| Any | Any other set, or Option/Alt held | none |

`Open` accepts every `LinkTarget`. `Preview` accepts only a preview target.

The click handler then matches on `(gesture, target)`:

| Gesture | Target | Action |
|---|---|---|
| `Open` | `External(target)` | `links::open(&target)` |
| `Open` | `Terminal(address)` | `cx.emit(TerminalEvent::Peek(address))` |
| `Preview` | `External(File(path))` | `quick_look::toggle(&path)` |
| `Preview` | any other target | no link action; the click continues to selection |

The native side is one module with one entry point:

```rust
// src/quick_look.rs, compiled only on macOS
/// Shows `path` in the Quick Look panel, or closes the panel when it already shows `path`.
pub fn toggle(path: &Path);
```

The module keeps the shown path in one main-thread value. The panel's data source reads that value. No other code reads or writes it.

## Steps

### 1. Prototype the panel (gate)

Build a scratch GPUI binary outside the app on macOS. It shows the Quick Look panel for a file from a click, using `objc2`, `objc2-foundation`, `objc2-app-kit` and `objc2-quick-look-ui`.

The prototype passes when all of these are true:

1. The panel shows the file and becomes the key window.
2. Space and Esc close it.
3. The GPUI window gets focus back after the panel closes.
4. A second call with a different file changes the file the panel shows.
5. The panel keeps its data source when it looks for a controller in the responder chain. Find the smallest way to do this with the GPUI window's view, and record it in this section.

If the prototype fails one of these checks, stop and report the result before step 2.

### 2. Replace `link_mods` with `LinkGesture`

1. Add `LinkGesture`, `link_gesture` and `LinkGesture::accepts` next to `link_mods` in `src/terminal.rs`. Remove `link_mods`.
2. In `render`, keep a hovered link only when the gesture accepts it. Linux and Windows behavior does not change, because no gesture there is `Preview`.
3. In the mouse-move handler, redraw when `link_gesture` returns a gesture or a link is underlined.
4. In the left mouse-down handler, match on `(gesture, target)` per the table above. Return early only when a link action ran. Leave `Preview` as a no-op in this step.
5. Add unit tests in the `src/terminal.rs` test module:
   - The modifier table, for each row and for each platform. Use `cfg!(target_os = "macos")` in the expected value, as `link_mods` does.
   - `Preview` accepts a file target and rejects a URL target and a terminal address.
   - `Open` accepts all three.
6. Run `just check`. It must pass on macOS and on Linux.

### 3. Add `src/quick_look.rs`

1. Add the `objc2` crates to `[target.'cfg(target_os = "macos")'.dependencies]` in `Cargo.toml`.
2. Write `quick_look::toggle` with the method that step 1 recorded. Declare the module with `#[cfg(target_os = "macos")]` in `src/main.rs`.
3. Call `quick_look::toggle(&path)` for the `Preview` row in the mouse-down handler. The call site has `#[cfg(target_os = "macos")]`. On other platforms the `Preview` gesture does not exist, so the arm never runs.
4. Check that the file exists at click time. `link_at` calls `links::resolve`, which checks existence, so the mouse-down handler gets this for free. Do not add a second check.
5. Run `just check` on macOS. Run `just test` on Linux, so that the `cfg` gates compile.

### 4. Update the docs

1. Add one sentence to `README.md` after "Ctrl-click a live terminal address": "On macOS, Cmd+Shift-click a file path to preview it with Quick Look."
2. Add the same rule to `defaults/AGENTS.md` next to the Ctrl-click sentence.
3. Do not edit `CHANGELOG.md`. The release writes it.

### 5. Test the real app on macOS

Build with `just build`. Start the app with `HERDR_APP_HOME` set to a scratch folder and its own core session. Do not touch the user's running app or core.

Check each spec behavior in a terminal pane:

1. Hold Cmd+Shift over a file path, a URL, and a terminal address. Only the file path is underlined.
2. Hold Cmd over the same three. All three are underlined, as before.
3. Cmd+Shift+click an image, a PDF, a source file, a folder, and `src/main.rs:12:3`. The panel shows each one.
4. Cmd+Shift+click the same file again. The panel closes.
5. With the panel open, Cmd+Shift+click another file. The panel shows the new file.
6. Press Space, then repeat and press Esc. The panel closes each time, and the same terminal has focus and takes typing.
7. Cmd+Shift+click a URL and empty text. A selection starts, as before.
8. In a program that tracks the mouse, such as `htop` or `vim` with `mouse=a`, Cmd+Shift+click a file path. The panel opens and the program gets no click.
9. Delete a file while its path is on screen, then Cmd+Shift+click it. Nothing happens.
10. Quit Herdr with the panel open. The panel closes.

Record a short video and screenshots for the pull request.

### 6. Open the pull request

Make one commit per step from 2 to 4, with lowercase single-line conventional messages and no AI co-author lines. Example: `feat: preview file links with quick look on cmd-shift-click`. Open the pull request to `master` with the step 5 evidence.

## Throughput

- **Blocking first step.** The step 1 prototype. Steps 3 and 5 depend on it.
- **Independent workstreams.** Step 2 does not depend on the prototype and can run in parallel with step 1. Step 4 is independent of both.
- **Shared mutable state.** None. Only step 2 and step 3 edit `src/terminal.rs`, and they run in sequence.
- **Smallest safe decomposition.** One owner. The diff is small, and steps 2 and 3 meet at one match arm.
