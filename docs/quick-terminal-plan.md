# Quick terminal migration plan

Give users a terminal they can summon and hide without losing its session. Start with macOS as the proposed first platform and validate the other platforms before promising parity. Keep the feature opt-in, including its system-wide shortcut. Obtain Can's approval for any additions to the app core or requests to herdr-core before implementation.

## Build verification first

- [ ] Use `docs/quick-terminal-verification.md` as the acceptance method and run `scripts/quick-terminal-verify-preflight` on the Linux test host.
- [ ] Prove an isolated launch, observation, and cleanup on each platform selected for implementation. Add an automated user-path driver once the prototype establishes the actual shortcut and window controls. Preserve its evidence outside the test home.
- [ ] Treat an untested platform or shortcut as unverified. Do not start feature implementation until its isolated verification route is established.

## Establish the behavior and boundary

- [ ] Confirm with Can whether a quick terminal is a separate Herdr window or a view of the existing window. Recommend a separate, persistent window so the main layout stays intact.
- [ ] Prototype a kept mod window containing a terminal using the existing window and layout APIs. Check whether it can be created hidden, shown, focused, hidden on focus loss, positioned on a selected display, and reused without destroying its terminal. Record what the existing APIs actually support.
- [ ] Prototype a system-wide shortcut on macOS, including registration failure and an already occupied shortcut. Check behavior when Herdr is hidden, another app is active, and the desktop uses multiple Spaces. Do not use Ghostty's private Dock APIs without a separate review.
- [ ] Answer the project's three core-growth questions for each missing capability. Present the proposed boundary to Can and wait for his go before changing the app core or herdr-core. Keep window content and user-facing presentation in a mod wherever the existing APIs allow it.
- [ ] Decide the initial behavior with Can. Recommend one persistent quick window, an opt-in shortcut, top-edge placement, a half-screen height, focus on show, and optional focus-loss hiding. Preserve the main window and restore the previously focused application where the platform permits. Treat screen selection, animation, other edges, and automatic hiding as follow-up options unless needed for a polished first release.

## Deliver a persistent quick window

- [ ] After approval, implement only the missing native window lifecycle and visibility controls identified by the prototype. Keep quick-window state separate from the main window's saved placement. Repeated show and hide operations must reuse the same window and session.
- [ ] Implement the terminal content in a shipped mod or layout if the prototype proves that route works. Define what happens when the terminal process exits, the user closes the window, or Herdr restarts. Do not terminate a running session merely because the quick window hides.
- [ ] Add an opt-in system-wide shortcut with a clear registration error and a way to change or disable it. Keep existing in-window bindings and edited user key files compatible. If defaults change, follow the shipped-key migration rules.
- [ ] Update `defaults/AGENTS.md` and `src/SKILL.md` for any new commands, settings, or agent-facing behavior. Add settings and command coverage to existing tests as needed.
- [ ] Run `just test` and `just check`. Test the actual macOS build with an isolated app home and core. Verify summon from another app, focus and typing, hide and reshow with a running process, repeat toggles, process exit, restart, shortcut conflict, multiple displays, and Spaces. Check main-window behavior remains unchanged. Capture a short video and screenshots for Can's review.

## Validate additional platforms

- [ ] Prototype Linux separately on X11 and Wayland. Check compositor support for global shortcuts, positioning, focus, and layer-shell windows. Do not promise identical behavior where the compositor prevents it. Exercise UI changes in `scripts/sandbox` and shut it down afterward.
- [ ] Prototype Windows global shortcut, monitor placement, focus restoration, and always-on-top behavior on Windows. Run the platform checks on Windows.
- [ ] Ship each platform only after its shortcut and window lifecycle work on that platform. Document unsupported environments and provide a usable manual trigger where appropriate.

## Evidence and unresolved decisions

### How Ghostty implements it

- [Ghostty's binding documentation](../../ghostty/src/input/Binding.zig) describes `toggle_quick_terminal` and the `global:` binding prefix. Ghostty does not assign a default shortcut.
- [Ghostty's configuration](../../ghostty/src/config/Config.zig) documents top, bottom, left, right, and center positions, screen selection, dimensions, and focus-loss hiding. Animation is implemented on macOS. Autohide defaults to true on macOS and false on Linux.
- [The macOS controller](../../ghostty/macos/Sources/Features/QuickTerminal/QuickTerminalController.swift) toggles a persistent window. When shown, it remembers the previous application, creates a terminal if its surface tree is empty, positions the window, and brings it to the front. Closing a live root surface hides it; an exited process empties the tree so a later show can create a new terminal.
- [The GTK application](../../ghostty/src/apprt/gtk/class/application.zig) toggles an existing quick window or creates one if the window protocol supports it. [The Wayland backend](../../ghostty/src/apprt/gtk/winproto/wayland.zig) handles compositor-specific window behavior. macOS behavior cannot be assumed to transfer directly to Linux.

The first implementation should use these as behavior and design references. They do not establish that Herdr's GPUI windows or mod APIs have the same capabilities.

### Herdr constraints

Herdr opens its main window in `src/main.rs`. Its `global` key section is scoped to the Herdr window in `src/keys.rs`. Existing kept mod windows and terminal blocks merit testing before adding new APIs. `window.close` and hide have different consequences, so the prototype must verify session survival.

Source inspection narrows the mod-window option. `openOverlay` with `kind: "window"` creates an overlay inside `AppWindow`, not a separate operating-system window. It cannot be summoned while the app window is hidden by itself. The initial core proposal therefore needs a native persistent window and an OS-wide shortcut registration. Keep the terminal's content in a mod or layout if a prototype confirms that works. Do not change herdr-core without a separate demonstrated need and Can's approval.

No prototype or platform test has been run for this plan. Shortcut registration, hidden-window creation, focus restoration, Spaces behavior, and mod ownership remain unproven. Can's core-boundary approval is a prerequisite, not an assumption.
