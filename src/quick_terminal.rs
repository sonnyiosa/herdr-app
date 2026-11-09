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
    height: Rc<Cell<f32>>,
    terminal: Option<Entity<TerminalView>>,
    error: Option<String>,
    visible: Rc<Cell<bool>>,
    dock_owned: Rc<Cell<bool>>,
    animation: Rc<native::Animation>,
    _activation: Subscription,
    terminal_subscription: Option<Subscription>,
    starting: bool,
}

impl QuickTerminal {
    fn new(core: Arc<Core>, height: f32, window: &mut Window, cx: &mut Context<Self>) -> Self {
        native::make_borderless(window);
        let visible = Rc::new(Cell::new(true));
        let dock_owned = Rc::new(Cell::new(false));
        let animation = Rc::new(native::Animation::default());
        let close_visible = visible.clone();
        let close_dock_owned = dock_owned.clone();
        let close_animation = animation.clone();
        window.on_window_should_close(cx, move |window, cx| {
            close_visible.set(false);
            native::set_visible(window, close_visible.clone(), close_dock_owned.clone(), close_animation.clone(), None, cx);
            false
        });
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() && this.visible.replace(false) {
                native::set_visible(window, this.visible.clone(), this.dock_owned.clone(), this.animation.clone(), None, cx);
            }
        });
        let mut quick = Self { core, height: Rc::new(Cell::new(height)), terminal: None, error: None, visible, dock_owned, animation, _activation: activation, terminal_subscription: None, starting: false };
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
                                this.terminal = None;
                                this.terminal_subscription = None;
                                if this.visible.replace(false) {
                                    native::set_visible(window, this.visible.clone(), this.dock_owned.clone(), this.animation.clone(), None, cx);
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
            .on_action(cx.listener(|this, action: &gpui_shell::action::ShellAction, window, cx| {
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
                native::set_visible(window, this.visible.clone(), this.dock_owned.clone(), this.animation.clone(), None, cx);
            } else {
                this.visible.set(true);
                let pending = (this.height.get() != height)
                    .then(|| window.display(cx).map(|d| (f32::from(d.bounds().size.height) * height, height, this.height.clone())))
                    .flatten();
                native::set_visible(window, this.visible.clone(), this.dock_owned.clone(), this.animation.clone(), pending, cx);
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
    match cx.open_window(options, |window, cx| cx.new(|cx| QuickTerminal::new(core, height, window, cx))) {
        Ok(handle) => {
            cx.global_mut::<QuickTerminalState>().window = Some(handle);
            let _ = handle.update(cx, |this, window, cx| {
                native::set_visible(window, this.visible.clone(), this.dock_owned.clone(), this.animation.clone(), None, cx);
            });
        }
        Err(error) => tracing::error!("cannot open quick terminal: {error:#}"),
    }
}

mod native {
    use super::*;

    const AUTO_HIDE_DOCK: usize = 1;
    const HIDE_DOCK: usize = 1 << 1;
    const POP_UP_MENU_LEVEL: isize = 101;
    const DURATION: std::time::Duration = std::time::Duration::from_millis(200);

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Point { x: f64, y: f64 }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Size { width: f64, height: f64 }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Rect { origin: Point, size: Size }

    fn with_height(mut frame: Rect, height: f64) -> Rect {
        frame.origin.y += frame.size.height - height;
        frame.size.height = height;
        frame
    }

    #[derive(Default)]
    pub struct Animation {
        sequence: Cell<u64>,
        frame: Cell<Option<Rect>>,
        running: Cell<bool>,
        initialized: Cell<bool>,
    }

    impl Animation {
        fn apply_height(&self, sequence: u64, frame: Rect, height: Option<(f32, f32, Rc<Cell<f32>>)>) -> Rect {
            if self.sequence.get() != sequence { return frame; }
            if let Some((pixels, fraction, applied)) = height {
                let frame = with_height(frame, f64::from(pixels));
                self.frame.set(Some(frame));
                applied.set(fraction);
                frame
            } else {
                frame
            }
        }
    }

    #[link(name = "objc")]
    unsafe extern "C" {
        fn sel_registerName(name: *const c_char) -> *const c_void;
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn objc_msgSend();
        #[cfg(target_arch = "x86_64")]
        fn objc_msgSend_stret();
    }

    unsafe fn send_id(object: *mut c_void, selector: &[u8]) -> *mut c_void {
        let f: unsafe extern "C" fn(*mut c_void, *const c_void) -> *mut c_void = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
        unsafe { f(object, sel_registerName(selector.as_ptr().cast())) }
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

    unsafe fn rect(object: *mut c_void, name: *const c_char) -> Rect {
        let selector = unsafe { sel_registerName(name) };
        #[cfg(target_arch = "x86_64")]
        {
            let mut result = std::mem::MaybeUninit::<Rect>::uninit();
            let f: unsafe extern "C" fn(*mut Rect, *mut c_void, *const c_void) = unsafe { std::mem::transmute(objc_msgSend_stret as *const ()) };
            unsafe { f(result.as_mut_ptr(), object, selector); result.assume_init() }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let f: unsafe extern "C" fn(*mut c_void, *const c_void) -> Rect = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
            unsafe { f(object, selector) }
        }
    }

    unsafe fn frame(window: *mut c_void) -> Rect {
        unsafe { rect(window, c"frame".as_ptr()) }
    }

    unsafe fn set_frame(window: *mut c_void, frame: Rect, alpha: f64, animated: bool) {
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
            let set: unsafe extern "C" fn(*mut c_void, *const c_void, Rect, bool) = std::mem::transmute(objc_msgSend as *const ());
            set(target, sel_registerName(c"setFrame:display:".as_ptr()), frame, true);
            let opacity: unsafe extern "C" fn(*mut c_void, *const c_void, f64) = std::mem::transmute(objc_msgSend as *const ());
            opacity(target, sel_registerName(c"setAlphaValue:".as_ptr()), alpha);
            if animated {
                let send: unsafe extern "C" fn(*mut c_void, *const c_void) = std::mem::transmute(objc_msgSend as *const ());
                send(context_class, sel_registerName(c"endGrouping".as_ptr()));
            }
        }
    }

    pub fn set_visible(window: &Window, visible: Rc<Cell<bool>>, dock_owned: Rc<Cell<bool>>, animation: Rc<Animation>, height: Option<(f32, f32, Rc<Cell<f32>>)>, cx: &App) {
        let Some(window) = ns_window(window) else { return };
        let desired = visible.get();
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
                    if desired {
                        let screen = send_id(window, b"screen\0");
                        if !screen.is_null() && !dock_owned.get() {
                            let screen_frame = rect(screen, c"frame".as_ptr());
                            let visible_frame = rect(screen, c"visibleFrame".as_ptr());
                            let inset = (visible_frame.origin.x - screen_frame.origin.x).max(0.0);
                            let mut target = animation.frame.get().unwrap_or_else(|| frame(window));
                            target.origin.x = screen_frame.origin.x + inset;
                            target.size.width = (screen_frame.size.width - inset).max(1.0);
                            animation.frame.set(Some(target));
                        }
                    }
                    if !animation.running.get() && (!desired || animation.frame.get().is_none()) {
                        animation.frame.set(Some(frame(window)));
                    }
                    let frame = animation.frame.get().unwrap_or_else(|| frame(window));
                    let frame = animation.apply_height(sequence, frame, height);
                    let hidden = Rect { origin: Point { x: frame.origin.x, y: frame.origin.y + frame.size.height }, size: frame.size };
                    if desired {
                        if options & (AUTO_HIDE_DOCK | HIDE_DOCK) == 0 {
                            set_options(app, sel_registerName(c"setPresentationOptions:".as_ptr()), options | AUTO_HIDE_DOCK);
                            dock_owned.set(true);
                        }
                        let set_level: unsafe extern "C" fn(*mut c_void, *const c_void, isize) = std::mem::transmute(objc_msgSend as *const ());
                        set_level(window, sel_registerName(c"setLevel:".as_ptr()), POP_UP_MENU_LEVEL);
                        let activate: unsafe extern "C" fn(*mut c_void, *const c_void, bool) = std::mem::transmute(objc_msgSend as *const ());
                        let is_visible: unsafe extern "C" fn(*mut c_void, *const c_void) -> bool = std::mem::transmute(objc_msgSend as *const ());
                        if !animation.initialized.replace(true) || !is_visible(window, sel_registerName(c"isVisible".as_ptr())) {
                            set_frame(window, hidden, 0.0, false);
                        }
                        activate(app, sel_registerName(c"activateIgnoringOtherApps:".as_ptr()), true);
                        f(window, sel_registerName(c"makeKeyAndOrderFront:".as_ptr()), std::ptr::null_mut());
                        set_frame(window, frame, 1.0, true);
                    } else {
                        set_frame(window, hidden, 0.0, true);
                    }
                    animation.running.set(true);
                }
            }
            timer.await;
            unsafe {
                if animation.sequence.get() == sequence {
                    animation.running.set(false);
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
        use super::{Animation, Cell, Point, Rc, Rect, Size, with_height};

        #[test]
        fn height_change_keeps_the_top_edge_fixed() {
            let original = Rect { origin: Point { x: 20.0, y: 500.0 }, size: Size { width: 900.0, height: 500.0 } };
            let changed = with_height(original, 750.0);
            assert_eq!(changed.origin.y, 250.0);
            assert_eq!(changed.size.height, 750.0);
            assert_eq!(changed.origin.x, 20.0);
            assert_eq!(changed.size.width, 900.0);
        }

        #[test]
        fn superseded_show_keeps_height_change_pending() {
            let animation = Animation::default();
            let applied = Rc::new(Cell::new(0.5));
            let frame = Rect { origin: Point { x: 0.0, y: 500.0 }, size: Size { width: 900.0, height: 500.0 } };
            animation.sequence.set(2);
            let unchanged = animation.apply_height(1, frame, Some((750.0, 0.75, applied.clone())));
            assert_eq!(unchanged.size.height, 500.0);
            assert_eq!(applied.get(), 0.5);
            let changed = animation.apply_height(2, frame, Some((750.0, 0.75, applied.clone())));
            assert_eq!(changed.size.height, 750.0);
            assert_eq!(applied.get(), 0.75);
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
