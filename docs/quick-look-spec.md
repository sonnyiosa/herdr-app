# Quick Look spec

On macOS, Cmd+Shift+click on a file link in terminal text shows that file in the system Quick Look panel. The user sees an image, PDF, video, or source file without leaving Herdr or opening another app. Cmd+click keeps its current behavior: it opens the link.

## Terms

- **File link.** A link in terminal text that resolves to a file or folder that exists on this computer. It is `links::Target::File` after `links::resolve`. Detected paths, `file:` URLs and OSC 8 hyperlinks can all be file links.
- **Link gesture.** What a left-click with modifiers does to a link: open it or preview it.
- **Quick Look panel.** The macOS preview panel (`QLPreviewPanel`), the same panel that Space shows in Finder.
- **Preview click.** A left-click with Cmd+Shift held and no Control or Option.

## Scope

In scope:

- macOS.
- File links in any terminal pane, on the normal screen and the alternate screen.
- One file in the panel at a time.

Out of scope:

- Linux and Windows. Ctrl+Shift+click there keeps its current behavior.
- Web URLs, other scheme URLs, and terminal addresses. Quick Look does not preview them.
- Files that exist only on another machine. A remote pane's path is a file link only when the same path exists on this computer. This is the current `links::resolve` rule.
- Changing the preview modifiers in `keys.json`. The open modifier is not configurable today either.
- Several files in one panel, and arrow keys to move between files.
- Changes to herdr-core.

## Hover

1. The pointer is over a terminal and the user holds Cmd+Shift. Herdr underlines the file link under the pointer and shows the pointing-hand cursor.
2. Herdr does not underline a URL or a terminal address while Cmd+Shift is held.
3. The underline follows the modifiers. Herdr redraws when the user presses or releases Shift while Cmd is held, so the underline switches between open targets and preview targets.

## Click

1. A preview click on a file link opens the Quick Look panel with that file.
2. The preview click does not start a selection, does not move the copy cursor, and does not go to a program that tracks the mouse.
3. The panel is open and the user makes a preview click on a different file link. The panel shows the new file.
4. The panel is open and the user makes a preview click on the file it shows. The panel closes.
5. A path with a `:line` or `:line:col` suffix previews the file. `links::resolve` removes the suffix today.
6. A folder link shows the folder in the panel, as Finder does.
7. The file is gone at click time. Nothing happens. Herdr checks that the file exists when the user clicks, not only when the user hovers.
8. A preview click on a URL, a terminal address, or text with no link keeps the current Cmd+Shift+click behavior: it starts a selection.

## Panel

1. The panel becomes the key window, so Space and Esc close it. This is the system behavior.
2. When the panel closes, the Herdr window that showed it gets focus back, with the same terminal focused.
3. The panel closes when Herdr quits.
4. A terminal that the user closes while the panel is open does not close the panel. The panel keeps the file it shows.

## Compatibility

Today Cmd+Shift+click on macOS starts a selection everywhere in a terminal. After this change it starts a selection everywhere except on a file link. Cmd+click, Shift+click, and the Linux and Windows gestures do not change.

## App core

The three questions in `AGENTS.md` all point to the app core:

1. The app acts on it. A click in terminal text changes what the app does.
2. Only the app sees it. Mods do not get mouse events from terminal text.
3. Mods cannot do it. A mod cannot show a native macOS panel.

## Agent-facing changes

- `README.md` names the gesture next to the Cmd-click sentence.
- `defaults/AGENTS.md` names the gesture next to the Ctrl-click (Cmd-click on macOS) sentence, so users' agents know it.
- No new command and no new `settings.json` key.
