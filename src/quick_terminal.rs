use std::ffi::{c_char, c_void};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::*;
use gpui::prelude::FluentBuilder;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{hc::Core, terminal::{TerminalEvent, TerminalView}};

const LABEL: &str = "herdr.quick-terminal";

fn find_terminal(list: &serde_json::Value, identity: &str) -> Option<String> {
    list.get("terminals")?.as_array()?.iter().find_map(|terminal| {
        let address = terminal.get("address")?.as_str()?;
        (terminal["labels"][LABEL].as_str() == Some(identity)
            && terminal["state"].as_str() == Some("live")
            && !address.contains('/'))
            .then(|| address.to_owned())
    })
}

struct QuickTerminalState {
    core: Arc<Core>,
    window: Option<WindowHandle<QuickTerminal>>,
    height: f32,
}
impl Global for QuickTerminalState {}

struct QuickTerminal {
    core: Arc<Core>,
    terminal: Option<Entity<TerminalView>>,
    error: Option<String>,
    visible: Rc<Cell<bool>>,
    dock_owned: Rc<Cell<bool>>,
    animation: Rc<native::Animation>,
    previous_app: Option<native::PreviousApp>,
    preserve_previous: bool,
    _activation: Subscription,
    terminal_subscription: Option<Subscription>,
    starting: bool,
}

impl QuickTerminal {
    fn new(core: Arc<Core>, previous_app: Option<native::PreviousApp>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        native::make_borderless(window);
        let visible = Rc::new(Cell::new(true));
        let dock_owned = Rc::new(Cell::new(false));
        let animation = Rc::new(native::Animation::default());
        let close_visible = visible.clone();
        let close_dock_owned = dock_owned.clone();
        let close_animation = animation.clone();
        window.on_window_should_close(cx, move |window, cx| {
            close_visible.set(false);
            native::set_visible(window, close_dock_owned.clone(), close_animation.clone(), None, cx);
            false
        });
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() && this.visible.replace(false) {
                this.previous_app = None;
                this.preserve_previous = false;
                native::set_visible(window, this.dock_owned.clone(), this.animation.clone(), None, cx);
            }
        });
        let mut quick = Self { core, terminal: None, error: None, visible, dock_owned, animation, previous_app, preserve_previous: false, _activation: activation, terminal_subscription: None, starting: false };
        quick.start(window, cx);
        quick
    }

    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.starting { return; }
        self.starting = true;
        self.error = None;
        let core = self.core.clone();
        let display = crate::terminal::initial_display(window);
        let identity = format!("{:x}", Sha256::digest(crate::mods::app_dir().to_string_lossy().as_bytes()));
        cx.spawn_in(window, async move |this, cx| {
            let run_core = core.clone();
            let result = cx.background_executor().spawn(async move {
                let list = run_core.call("terminal.list", json!({}))?;
                if !list["terminals"].is_array() {
                    anyhow::bail!("terminal.list did not return terminals");
                }
                if let Some(address) = find_terminal(&list, &identity) { return Ok(address); }
                run_core.run(vec![], Some(crate::mods::home().to_string_lossy().into_owned()), json!({ (LABEL): identity }), None, Some((80, 24)), display)
            }).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.starting = false;
                if !Arc::ptr_eq(&core, &this.core) {
                    this.start(window, cx);
                    return;
                }
                match result {
                    Ok(address) => {
                        let terminal = cx.new(|cx| {
                            let mut terminal = TerminalView::attach(this.core.clone(), address, (80, 24), cx);
                            terminal.set_window_visibility(this.visible.clone());
                            terminal
                        });
                        this.terminal_subscription = Some(cx.subscribe_in(&terminal, window, |this, _, event: &TerminalEvent, window, cx| {
                            if matches!(event, TerminalEvent::Ended) {
                                this.previous_app = None;
                                this.preserve_previous = false;
                                this.terminal = None;
                                this.terminal_subscription = None;
                                if this.visible.replace(false) {
                                    native::set_visible(window, this.dock_owned.clone(), this.animation.clone(), None, cx);
                                }
                                cx.notify();
                            }
                        }));
                        if native::visible(window) {
                            let focus = terminal.read(cx).focus_handle().clone();
                            focus.focus(window, cx);
                        }
                        this.terminal = Some(terminal);
                    }
                    Err(error) => this.error = Some(format!("Could not start terminal: {error:#}")),
                }
                cx.notify();
            });
        }).detach();
    }
}

impl Render for QuickTerminal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().key_context("Herdr").bg(crate::settings::color("background"))
            .on_action(cx.listener(|this, action: &herdr_shell::action::ShellAction, window, cx| {
                let Some(terminal) = &this.terminal else { cx.propagate(); return };
                match action.id() {
                    "terminal.copy" => {
                        let copied = terminal.update(cx, |terminal, cx| {
                            if !terminal.copy(cx) { return false; }
                            terminal.clear_selection(cx);
                            true
                        });
                        if !copied { cx.propagate(); }
                    }
                    "terminal.paste" => terminal.update(cx, |terminal, cx| terminal.paste(cx)),
                    "terminal.paste_image" => terminal.update(cx, |terminal, cx| terminal.paste_image(cx)),
                    "terminal.search" => terminal.update(cx, |terminal, cx| terminal.open_search(window, cx)),
                    "terminal.copy_mode" => terminal.update(cx, |terminal, cx| terminal.enter_copy_mode(window, cx)),
                    _ => cx.propagate(),
                }
            }))
            .when_some(self.terminal.as_ref(), |el, terminal| el.child(crate::window::shown_terminal(terminal, cx)))
            .when_some(self.error.as_ref(), |el, error| el.text_color(crate::settings::color("foreground")).child(error.clone()))
    }
}

pub fn init(core: Arc<Core>, cx: &mut App) {
    cx.set_global(QuickTerminalState { core, window: None, height: 0.5 });
}

pub fn set_height(height: Option<f32>, cx: &mut App) {
    if cx.has_global::<QuickTerminalState>() {
        cx.global_mut::<QuickTerminalState>().height = height.unwrap_or(0.5);
    }
}

pub fn reconnected(core: Arc<Core>, cx: &mut App) {
    let Some(state) = cx.try_global::<QuickTerminalState>() else { return };
    let window = state.window;
    cx.global_mut::<QuickTerminalState>().core = core.clone();
    if let Some(window) = window {
        let _ = window.update(cx, |this, _, cx| {
            this.core = core.clone();
            if let Some(terminal) = &this.terminal {
                terminal.update(cx, |terminal, cx| terminal.reattach(core, cx));
            }
        });
    }
}

pub fn toggle(cx: &mut App) {
    let Some(state) = cx.try_global::<QuickTerminalState>() else { return };
    let handle = state.window;
    let core = state.core.clone();
    let height = state.height;
    if let Some(handle) = handle {
        if handle.update(cx, |this, window, cx| {
            if this.visible.get() {
                this.visible.set(false);
                this.preserve_previous = !window.is_window_active();
                if !this.preserve_previous {
                    if let Some(previous_app) = this.previous_app.take() {
                        previous_app.activate();
                    }
                }
                native::set_visible(window, this.dock_owned.clone(), this.animation.clone(), None, cx);
            } else {
                let frontmost = native::PreviousApp::frontmost();
                if frontmost.is_some() || !this.preserve_previous {
                    this.previous_app = frontmost;
                }
                this.preserve_previous = false;
                this.visible.set(true);
                native::set_visible(window, this.dock_owned.clone(), this.animation.clone(), Some(height), cx);
                if this.error.is_some() || this.terminal.is_none() {
                    this.start(window, cx);
                }
                if let Some(terminal) = &this.terminal {
                    let focus = terminal.read(cx).focus_handle().clone();
                    focus.focus(window, cx);
                }
            }
        }).is_ok() { return; }
    }
    let bounds = cx.primary_display().map(|display| {
        let mut bounds = display.bounds();
        bounds.size.height = bounds.size.height * height;
        bounds
    });
    let options = WindowOptions {
        window_bounds: bounds.map(WindowBounds::Windowed),
        titlebar: None,
        is_movable: false,
        is_minimizable: false,
        ..Default::default()
    };
    let previous_app = native::PreviousApp::frontmost();
    match cx.open_window(options, |window, cx| cx.new(|cx| QuickTerminal::new(core, previous_app, window, cx))) {
        Ok(handle) => {
            cx.global_mut::<QuickTerminalState>().window = Some(handle);
            let _ = handle.update(cx, |this, window, cx| {
                native::set_visible(window, this.dock_owned.clone(), this.animation.clone(), this.visible.get().then_some(height), cx);
            });
        }
        Err(error) => tracing::error!("cannot open quick terminal: {error:#}"),
    }
}

mod native {
    use super::*;
    use objc2::{msg_send, runtime::AnyObject};
    use objc2_app_kit::NSScreen;
    use objc2_foundation::{MainThreadMarker, NSNumber, NSRect, NSString};

    const AUTO_HIDE_DOCK: usize = 1;
    const HIDE_DOCK: usize = 1 << 1;
    const POP_UP_MENU_LEVEL: isize = 101;
    const DURATION: std::time::Duration = std::time::Duration::from_millis(200);

    #[derive(Clone, Copy)]
    struct Display {
        id: Option<u32>,
        frame: NSRect,
        left_dock_inset: f64,
    }

    #[derive(Clone, Copy)]
    struct Placement {
        display_id: Option<u32>,
        shown: NSRect,
        left_dock_inset: f64,
    }

    fn placement_on(mut displays: impl Iterator<Item = Display>, previous: Option<Placement>, height: f32, dock_owned: bool) -> Option<Placement> {
        let primary = displays.next()?;
        let display = match previous.and_then(|placement| placement.display_id) {
            Some(id) if primary.id != Some(id) => displays.find(|display| display.id == Some(id)).unwrap_or(primary),
            _ => primary,
        };
        // Our auto-hide changes visibleFrame while a hide can still reverse.
        let inset = previous
            .filter(|placement| dock_owned && display.id.is_some() && placement.display_id == display.id)
            .map_or(display.left_dock_inset, |placement| placement.left_dock_inset);
        let mut shown = display.frame;
        let terminal_height = shown.size.height * f64::from(height);
        shown.origin.x += inset;
        shown.origin.y += shown.size.height - terminal_height;
        shown.size.width = (shown.size.width - inset).max(1.0);
        shown.size.height = terminal_height;
        Some(Placement { display_id: display.id, shown, left_dock_inset: inset })
    }

    fn current_placement(previous: Option<Placement>, height: f32, dock_owned: bool) -> Option<Placement> {
        let marker = MainThreadMarker::new().expect("Quick Terminal runs on the main thread");
        let screens = NSScreen::screens(marker);
        let key = NSString::from_str("NSScreenNumber");
        // AppKit lists the primary display first.
        let displays = screens.iter().map(|screen| {
            let id = screen.deviceDescription().objectForKey(&key)
                .and_then(|value| value.downcast_ref::<NSNumber>().map(NSNumber::as_u32));
            let frame = screen.frame();
            let visible = screen.visibleFrame();
            Display { id, frame, left_dock_inset: (visible.origin.x - frame.origin.x).max(0.0) }
        });
        placement_on(displays, previous, height, dock_owned)
    }

    fn hidden_frame(mut shown: NSRect) -> NSRect {
        shown.origin.y += shown.size.height;
        shown
    }

    #[derive(Default)]
    pub struct Animation {
        sequence: Cell<u64>,
        placement: Cell<Option<Placement>>,
        initialized: Cell<bool>,
    }

    #[link(name = "objc")]
    unsafe extern "C" {
        fn sel_registerName(name: *const c_char) -> *const c_void;
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn objc_msgSend();
    }

    unsafe fn send_id(object: *mut c_void, selector: &[u8]) -> *mut c_void {
        let f: unsafe extern "C" fn(*mut c_void, *const c_void) -> *mut c_void = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
        unsafe { f(object, sel_registerName(selector.as_ptr().cast())) }
    }

    pub struct PreviousApp(*mut c_void);

    impl PreviousApp {
        pub fn frontmost() -> Option<Self> {
            unsafe {
                let workspace = send_id(objc_getClass(c"NSWorkspace".as_ptr()), b"sharedWorkspace\0");
                let app = send_id(workspace, b"frontmostApplication\0");
                if app.is_null() { return None; }
                let pid: unsafe extern "C" fn(*mut c_void, *const c_void) -> i32 = std::mem::transmute(objc_msgSend as *const ());
                if pid(app, sel_registerName(c"processIdentifier".as_ptr())) == std::process::id() as i32 { return None; }
                send_id(app, b"retain\0");
                Some(Self(app))
            }
        }

        pub fn activate(&self) {
            unsafe {
                let terminated: unsafe extern "C" fn(*mut c_void, *const c_void) -> bool = std::mem::transmute(objc_msgSend as *const ());
                if !terminated(self.0, sel_registerName(c"isTerminated".as_ptr())) {
                    let activate: unsafe extern "C" fn(*mut c_void, *const c_void, usize) -> bool = std::mem::transmute(objc_msgSend as *const ());
                    activate(self.0, sel_registerName(c"activateWithOptions:".as_ptr()), 1 << 1);
                }
            }
        }
    }

    impl Drop for PreviousApp {
        fn drop(&mut self) {
            unsafe {
                let release: unsafe extern "C" fn(*mut c_void, *const c_void) = std::mem::transmute(objc_msgSend as *const ());
                release(self.0, sel_registerName(c"release".as_ptr()));
            }
        }
    }

    fn ns_window(window: &Window) -> Option<*mut c_void> {
        let handle = HasWindowHandle::window_handle(window).ok()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return None };
        let window = unsafe { send_id(handle.ns_view.as_ptr(), b"window\0") };
        (!window.is_null()).then_some(window)
    }

    pub fn visible(window: &Window) -> bool {
        let Some(window) = ns_window(window) else { return false };
        let f: unsafe extern "C" fn(*mut c_void, *const c_void) -> bool = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
        unsafe { f(window, sel_registerName(c"isVisible".as_ptr())) }
    }

    pub fn make_borderless(window: &Window) {
        let Some(window) = ns_window(window) else { return };
        let set_style: unsafe extern "C" fn(*mut c_void, *const c_void, usize) = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
        unsafe { set_style(window, sel_registerName(c"setStyleMask:".as_ptr()), 0); }
    }

    unsafe fn frame(window: *mut c_void) -> NSRect {
        unsafe { msg_send![&*window.cast::<AnyObject>(), frame] }
    }

    unsafe fn set_frame(window: *mut c_void, frame: NSRect, alpha: f64, animated: bool) {
        unsafe {
            let context_class = objc_getClass(c"NSAnimationContext".as_ptr());
            if animated {
                let send: unsafe extern "C" fn(*mut c_void, *const c_void) = std::mem::transmute(objc_msgSend as *const ());
                send(context_class, sel_registerName(c"beginGrouping".as_ptr()));
                let context = send_id(context_class, b"currentContext\0");
                let duration: unsafe extern "C" fn(*mut c_void, *const c_void, f64) = std::mem::transmute(objc_msgSend as *const ());
                duration(context, sel_registerName(c"setDuration:".as_ptr()), DURATION.as_secs_f64());
            }
            let target = if animated { send_id(window, b"animator\0") } else { window };
            let set: unsafe extern "C" fn(*mut c_void, *const c_void, NSRect, bool) = std::mem::transmute(objc_msgSend as *const ());
            set(target, sel_registerName(c"setFrame:display:".as_ptr()), frame, true);
            let opacity: unsafe extern "C" fn(*mut c_void, *const c_void, f64) = std::mem::transmute(objc_msgSend as *const ());
            opacity(target, sel_registerName(c"setAlphaValue:".as_ptr()), alpha);
            if animated {
                let send: unsafe extern "C" fn(*mut c_void, *const c_void) = std::mem::transmute(objc_msgSend as *const ());
                send(context_class, sel_registerName(c"endGrouping".as_ptr()));
            }
        }
    }

    pub fn set_visible(window: &Window, dock_owned: Rc<Cell<bool>>, animation: Rc<Animation>, show_height: Option<f32>, cx: &App) {
        let Some(window) = ns_window(window) else { return };
        let desired = show_height.is_some();
        let sequence = animation.sequence.get().wrapping_add(1);
        animation.sequence.set(sequence);
        let timer = cx.background_executor().timer(DURATION);
        unsafe { send_id(window, b"retain\0"); }
        cx.foreground_executor().spawn(async move {
            unsafe {
                if animation.sequence.get() == sequence {
                    let f: unsafe extern "C" fn(*mut c_void, *const c_void, *mut c_void) = std::mem::transmute(objc_msgSend as *const ());
                    let class = objc_getClass(c"NSApplication".as_ptr());
                    let app = send_id(class, b"sharedApplication\0");
                    let get_options: unsafe extern "C" fn(*mut c_void, *const c_void) -> usize = std::mem::transmute(objc_msgSend as *const ());
                    let set_options: unsafe extern "C" fn(*mut c_void, *const c_void, usize) = std::mem::transmute(objc_msgSend as *const ());
                    let options = get_options(app, sel_registerName(c"presentationOptions".as_ptr()));
                    let previous = animation.placement.get();
                    let placement = show_height.and_then(|height| current_placement(previous, height, dock_owned.get()));
                    let display_changed = placement.zip(previous)
                        .is_some_and(|(current, previous)| current.display_id != previous.display_id);
                    let placement = placement.or(previous);
                    animation.placement.set(placement);
                    let shown = placement.map_or_else(|| frame(window), |placement| placement.shown);
                    let hidden = hidden_frame(shown);
                    if desired {
                        if options & (AUTO_HIDE_DOCK | HIDE_DOCK) == 0 {
                            set_options(app, sel_registerName(c"setPresentationOptions:".as_ptr()), options | AUTO_HIDE_DOCK);
                            dock_owned.set(true);
                        }
                        let set_level: unsafe extern "C" fn(*mut c_void, *const c_void, isize) = std::mem::transmute(objc_msgSend as *const ());
                        set_level(window, sel_registerName(c"setLevel:".as_ptr()), POP_UP_MENU_LEVEL);
                        let activate: unsafe extern "C" fn(*mut c_void, *const c_void, bool) = std::mem::transmute(objc_msgSend as *const ());
                        let is_visible: unsafe extern "C" fn(*mut c_void, *const c_void) -> bool = std::mem::transmute(objc_msgSend as *const ());
                        if !animation.initialized.replace(true) || display_changed || !is_visible(window, sel_registerName(c"isVisible".as_ptr())) {
                            set_frame(window, hidden, 0.0, false);
                        }
                        activate(app, sel_registerName(c"activateIgnoringOtherApps:".as_ptr()), true);
                        f(window, sel_registerName(c"makeKeyAndOrderFront:".as_ptr()), std::ptr::null_mut());
                        set_frame(window, shown, 1.0, true);
                    } else {
                        set_frame(window, hidden, 0.0, true);
                    }
                }
            }
            timer.await;
            unsafe {
                if animation.sequence.get() == sequence {
                    let is_key: unsafe extern "C" fn(*mut c_void, *const c_void) -> bool = std::mem::transmute(objc_msgSend as *const ());
                    if desired && !is_key(window, sel_registerName(c"isKeyWindow".as_ptr())) {
                        // App activation completes after makeKeyAndOrderFront: and can give key status back to another window.
                        let f: unsafe extern "C" fn(*mut c_void, *const c_void, *mut c_void) = std::mem::transmute(objc_msgSend as *const ());
                        f(window, sel_registerName(c"makeKeyAndOrderFront:".as_ptr()), std::ptr::null_mut());
                    }
                    if !desired {
                        let f: unsafe extern "C" fn(*mut c_void, *const c_void, *mut c_void) = std::mem::transmute(objc_msgSend as *const ());
                        f(window, sel_registerName(c"orderOut:".as_ptr()), std::ptr::null_mut());
                        if dock_owned.replace(false) {
                            let app = send_id(objc_getClass(c"NSApplication".as_ptr()), b"sharedApplication\0");
                            let get: unsafe extern "C" fn(*mut c_void, *const c_void) -> usize = std::mem::transmute(objc_msgSend as *const ());
                            let set: unsafe extern "C" fn(*mut c_void, *const c_void, usize) = std::mem::transmute(objc_msgSend as *const ());
                            let options = get(app, sel_registerName(c"presentationOptions".as_ptr()));
                            set(app, sel_registerName(c"setPresentationOptions:".as_ptr()), options & !AUTO_HIDE_DOCK);
                        }
                    }
                }
                let release: unsafe extern "C" fn(*mut c_void, *const c_void) = std::mem::transmute(objc_msgSend as *const ());
                release(window, sel_registerName(c"release".as_ptr()));
            }
        }).detach();
    }

    #[cfg(test)]
    mod tests {
        use super::{Display, hidden_frame, placement_on};
        use objc2_foundation::{NSPoint, NSRect, NSSize};

        const PRIMARY: Display = Display {
            id: Some(1),
            frame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1440.0, 900.0)),
            left_dock_inset: 48.0,
        };
        const EXTERNAL: Display = Display {
            id: Some(2),
            frame: NSRect::new(NSPoint::new(-1920.0, 100.0), NSSize::new(1920.0, 1200.0)),
            left_dock_inset: 0.0,
        };

        #[test]
        fn show_uses_current_geometry_of_the_last_connected_display() {
            let previous = placement_on([EXTERNAL].into_iter(), None, 0.5, false);
            let moved = Display {
                frame: NSRect::new(NSPoint::new(1440.0, -100.0), NSSize::new(1600.0, 1000.0)),
                ..EXTERNAL
            };
            let placement = placement_on([PRIMARY, moved].into_iter(), previous, 0.75, false).unwrap();
            assert_eq!(placement.shown, NSRect::new(NSPoint::new(1440.0, 150.0), NSSize::new(1600.0, 750.0)));
        }

        #[test]
        fn show_fits_the_primary_display_after_disconnect() {
            let previous = placement_on([EXTERNAL].into_iter(), None, 0.5, false);
            let placement = placement_on([PRIMARY].into_iter(), previous, 0.5, false).unwrap();
            assert_eq!(placement.shown, NSRect::new(NSPoint::new(48.0, 450.0), NSSize::new(1392.0, 450.0)));
            assert_eq!(hidden_frame(placement.shown), NSRect::new(NSPoint::new(48.0, 900.0), NSSize::new(1392.0, 450.0)));
        }

        #[test]
        fn reversing_a_hide_preserves_the_left_dock_inset() {
            let previous = placement_on([PRIMARY].into_iter(), None, 0.5, false).unwrap();
            let dock_hidden = Display { left_dock_inset: 0.0, ..PRIMARY };
            let placement = placement_on([dock_hidden].into_iter(), Some(previous), 0.5, true).unwrap();
            assert_eq!(placement.shown, previous.shown);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::find_terminal;
    use serde_json::json;

    #[test]
    fn reuse_only_this_homes_local_quick_terminal() {
        let list = json!({"terminals": [
            {"address": "remote/t1", "state": "live", "labels": {"herdr.quick-terminal": "ours"}},
            {"address": "t1", "state": "finished", "labels": {"herdr.quick-terminal": "ours"}},
            {"address": "t2", "state": "live", "labels": {"herdr.quick-terminal": "another-home"}},
            {"address": "t3", "state": "live", "labels": {"herdr.quick-terminal": "ours"}}
        ]});
        assert_eq!(find_terminal(&list, "ours").as_deref(), Some("t3"));
        assert_eq!(find_terminal(&list, "missing"), None);
    }
}
