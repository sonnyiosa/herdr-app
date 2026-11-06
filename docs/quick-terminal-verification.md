# Verify the quick terminal

Use this method before merging a quick terminal implementation. The feature does not exist yet, so the expected results below are acceptance criteria, not results already observed. Record the commit, platform, display server, shortcut, and test result for every run. Keep screenshots, a video, and command output outside the sandbox directory so teardown does not remove the evidence.

## Prepare an isolated run

On Linux, run `scripts/quick-terminal-verify-preflight` from the repository root. Choose a unique `HERDR_APP_SANDBOX` name and confirm that `/var/tmp/herdr-app-$HERDR_APP_SANDBOX` does not exist. Run `scripts/sandbox up` with that name exported. Use `scripts/sandbox status` as the readiness check. Run `scripts/sandbox down` with the same name when finished. Never use a sandbox name belonging to another run. The sandbox creates its own app home, core session, and headless display. Its `down` command deletes its sandbox directory.

On macOS and Windows, use a separate test machine or isolated desktop session with a scratch app home and a uniquely named `HC_SESSION`. Record the app process and core session started by the run. A human must perform keyboard and pointer actions on a real desktop. Do not automate input on a person's real display. Stop only the app and core started by the test. Preserve the evidence outside the scratch home. Validate the platform-specific launch and cleanup procedure before using it; the Linux sandbox does not provide macOS or Windows isolation.

## Check the user's workflow

Run each case on the actual platform. Capture the action and result. A command response or screenshot alone does not prove keyboard focus or process survival.

1. Configure an unused system-wide shortcut. Focus another application. Press the shortcut. The quick terminal appears on the intended display and receives typed input. Record a video that shows both applications and the keystroke's effect.
2. Start a long-running terminal process that prints a unique marker and remains alive. Hide the quick terminal using its intended user action. Show it again. The same process and marker remain, and new input reaches it. Record its terminal identity or process identity before and after, plus the visible output.
3. Repeat the shortcut ten times. Each show reuses the same terminal, and each hide leaves the main window and process intact. Record window and terminal identities rather than relying only on appearance.
4. If focus-loss hiding is enabled, focus another application. Verify that the quick terminal hides, its process survives, and the next shortcut restores it. With the option disabled, verify that losing focus leaves it visible.
5. Reserve the shortcut in another application before registering it. Verify that Herdr reports the conflict and that its existing window and keys remain usable. Disable or change the shortcut and verify recovery.
6. Close or exit the quick terminal's process. Verify that the resulting window state matches the documented policy and that another toggle can recover. Restart Herdr with the isolated home and check the documented persistence policy.
7. Check the main window before and after toggling, including its placement, selected layout, and existing key bindings. Check a second display and a different macOS Space or Linux compositor where available. Record unsupported compositor behavior as a limitation, not a pass.
8. Run `just test` and `just check` for the implementation. Verify existing user settings and edited key files survive an upgrade. Review `defaults/AGENTS.md` and `src/SKILL.md` against any new commands or settings.

## Distinguish the test surfaces

The Linux sandbox exercises an Xwayland desktop. It does not prove native Wayland layer-shell or compositor shortcut behavior. Run those on a real Wayland session. A simulated app command also does not prove a system-wide shortcut; press the actual shortcut with another app focused. Run macOS and Windows checks on those systems. Do not claim a platform passes when its native run was skipped.

The first implementation must add a repeatable automation driver once its actual shortcut configuration and window controls exist. The driver must exercise the public user path, preserve the evidence above, and fail when a terminal is recreated or destroyed. Until then this document and the preflight are a verification method, not an automated feature test.
