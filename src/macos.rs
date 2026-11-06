use gpui::*;

actions!(herdr, [Quit, Hide, HideOthers, ShowAll, Settings]);

pub fn init(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &Settings, _| crate::mods::request(crate::mods::Request::Run { name: "app.settings".into(), args: serde_json::Value::Null }));
    quick_shortcut(cx);
    cx.set_menus([Menu::new("Herdr").items([
        MenuItem::action("Settings", Settings),
        MenuItem::separator(),
        MenuItem::os_submenu("Services", SystemMenuType::Services),
        MenuItem::separator(),
        MenuItem::action("Hide Herdr", Hide),
        MenuItem::action("Hide Others", HideOthers),
        MenuItem::action("Show All", ShowAll),
        MenuItem::separator(),
        MenuItem::action("Quit Herdr", Quit),
    ])]);
}

pub fn action(name: &str) -> Option<Box<dyn Action>> {
    match name {
        "app.quit" => Some(Box::new(Quit)),
        "app.hide" => Some(Box::new(Hide)),
        "app.hide_others" => Some(Box::new(HideOthers)),
        "app.show_all" => Some(Box::new(ShowAll)),
        "app.settings" => Some(Box::new(Settings)),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shortcut {
    key: u32,
    modifiers: u32,
}

impl Shortcut {
    fn parse(value: &str) -> Result<Self, &'static str> {
        let mut modifiers = 0;
        let mut key = None;
        for part in value.split('-') {
            let bit = match part {
                "cmd" => Some(1 << 8),
                "shift" => Some(1 << 9),
                "alt" => Some(1 << 11),
                "ctrl" => Some(1 << 12),
                _ => None,
            };
            if let Some(bit) = bit {
                if modifiers & bit != 0 { return Err("duplicate modifier"); }
                modifiers |= bit;
            } else {
                if key.is_some() { return Err("shortcut must contain one key"); }
                key = Some(match part {
                    "a" => 0, "s" => 1, "d" => 2, "f" => 3, "h" => 4, "g" => 5,
                    "z" => 6, "x" => 7, "c" => 8, "v" => 9, "b" => 11, "q" => 12,
                    "w" => 13, "e" => 14, "r" => 15, "y" => 16, "t" => 17,
                    "1" => 18, "2" => 19, "3" => 20, "4" => 21, "6" => 22, "5" => 23,
                    "9" => 25, "7" => 26, "8" => 28, "0" => 29, "o" => 31,
                    "u" => 32, "i" => 34, "p" => 35, "l" => 37, "j" => 38,
                    "k" => 40, "\\" => 42, "n" => 45, "m" => 46, "space" => 49, "backquote" => 50,
                    _ => return Err("unknown shortcut key"),
                });
            }
        }
        if modifiers == 0 { return Err("shortcut requires a modifier"); }
        Ok(Self { key: key.ok_or("shortcut requires a key")?, modifiers })
    }
}

mod hotkey {
    use super::Shortcut;
    use std::ffi::c_void;
    use std::ptr;

    type Handle = *mut c_void;
    type Status = i32;
    #[repr(C)]
    struct EventType { class: u32, kind: u32 }
    #[repr(C)]
    struct HotKeyId { signature: u32, id: u32 }
    const SIGNATURE: u32 = u32::from_be_bytes(*b"HrdQ");
    const EXCLUSIVE: u32 = 1;
    const EVENT: EventType = EventType { class: u32::from_be_bytes(*b"keyb"), kind: 5 };

    #[link(name = "Carbon", kind = "framework")]
    unsafe extern "C" {
        fn GetApplicationEventTarget() -> Handle;
        fn InstallEventHandler(target: Handle, handler: unsafe extern "C" fn(Handle, Handle, Handle) -> Status,
            count: u32, events: *const EventType, data: Handle, result: *mut Handle) -> Status;
        fn RemoveEventHandler(handler: Handle) -> Status;
        fn RegisterEventHotKey(key: u32, modifiers: u32, id: HotKeyId, target: Handle, options: u32, result: *mut Handle) -> Status;
        fn UnregisterEventHotKey(hotkey: Handle) -> Status;
        fn GetEventParameter(event: Handle, name: u32, kind: u32, actual: *mut u32, size: u32, actual_size: *mut u32, data: Handle) -> Status;
    }

    unsafe extern "C" fn pressed(_: Handle, event: Handle, data: Handle) -> Status {
        let mut id = HotKeyId { signature: 0, id: 0 };
        let status = unsafe { GetEventParameter(event, u32::from_be_bytes(*b"----"), u32::from_be_bytes(*b"hkid"), ptr::null_mut(),
            std::mem::size_of::<HotKeyId>() as u32, ptr::null_mut(), (&mut id as *mut HotKeyId).cast()) };
        if status != 0 || id.signature != SIGNATURE || id.id != 1 { return -9874; }
        let sender = unsafe { &*(data as *const async_channel::Sender<()>) };
        let _ = sender.try_send(());
        0
    }

    pub struct Registration {
        handler: Handle,
        hotkey: Handle,
        _sender: Box<async_channel::Sender<()>>,
    }

    impl Registration {
        pub fn new(shortcut: Shortcut, sender: async_channel::Sender<()>) -> Result<Self, i32> {
            let sender = Box::new(sender);
            let mut handler = ptr::null_mut();
            let mut hotkey = ptr::null_mut();
            let target = unsafe { GetApplicationEventTarget() };
            let status = unsafe { InstallEventHandler(target, pressed, 1, &EVENT, (&*sender as *const async_channel::Sender<()>).cast_mut().cast(), &mut handler) };
            if status != 0 { return Err(status); }
            let status = unsafe { RegisterEventHotKey(shortcut.key, shortcut.modifiers, HotKeyId { signature: SIGNATURE, id: 1 }, target, EXCLUSIVE, &mut hotkey) };
            if status != 0 {
                unsafe { RemoveEventHandler(handler); }
                return Err(status);
            }
            Ok(Self { handler, hotkey, _sender: sender })
        }
    }
    impl Drop for Registration {
        fn drop(&mut self) {
            unsafe { UnregisterEventHotKey(self.hotkey); RemoveEventHandler(self.handler); }
        }
    }
}

struct QuickShortcut {
    sender: async_channel::Sender<()>,
    registration: Option<hotkey::Registration>,
    registered: Option<Shortcut>,
    attempted: Option<Option<String>>,
}

impl Global for QuickShortcut {}

fn quick_shortcut(cx: &mut App) {
    let (sender, receiver) = async_channel::bounded(32);
    cx.set_global(QuickShortcut { sender, registration: None, registered: None, attempted: None });
    cx.spawn(async move |cx| {
        while receiver.recv().await.is_ok() {
            cx.update(|cx| crate::quick_terminal::toggle(cx));
        }
    }).detach();
}

pub fn update_quick_shortcut(person: &crate::settings::Person, cx: &mut App) {
    if !cx.has_global::<QuickShortcut>() { return; }
    let state = cx.global_mut::<QuickShortcut>();
    let configured = person.quick_terminal_shortcut.clone();
    if state.attempted.as_ref() == Some(&configured) { return; }
    state.attempted = Some(configured.clone());
    let result = match configured.as_deref().map(Shortcut::parse).transpose() {
        Ok(None) => { state.registration = None; state.registered = None; Ok(()) },
        Ok(Some(shortcut)) if state.registered == Some(shortcut) => Ok(()),
        Ok(Some(shortcut)) => match hotkey::Registration::new(shortcut, state.sender.clone()) {
            Ok(next) => { state.registration = Some(next); state.registered = Some(shortcut); Ok(()) },
            Err(error) => Err(format!("Could not register the shortcut (macOS error {error}). Choose another shortcut.")),
        },
        Err(error) => Err(format!("Invalid shortcut: {error}. Check quick_terminal_shortcut in settings.json.")),
    };
    let previous_retained = state.registration.is_some();
    if let Err(mut error) = result {
        if previous_retained {
            error.push_str(" The previous shortcut remains active.");
        }
        tracing::warn!("{error}");
        crate::mods::request(crate::mods::Request::OpenOverlay {
            id: crate::mods::new_overlay_id(),
            spec: serde_json::json!({ "kind": "toast", "notification": {
                "title": "Quick terminal shortcut unavailable", "body": error, "level": "error"
            } }),
        });
    }
}

#[cfg(test)]
mod shortcut_tests {
    use super::Shortcut;

    #[test]
    fn parses_quick_terminal_shortcuts() {
        assert_eq!(Shortcut::parse("ctrl-alt-space"), Ok(Shortcut { key: 49, modifiers: (1 << 12) | (1 << 11) }));
        assert_eq!(Shortcut::parse("cmd-shift-backquote"), Ok(Shortcut { key: 50, modifiers: (1 << 8) | (1 << 9) }));
        assert_eq!(Shortcut::parse("cmd-\\"), Ok(Shortcut { key: 42, modifiers: 1 << 8 }));
        assert!(Shortcut::parse("space").is_err());
        assert!(Shortcut::parse("cmd-cmd-a").is_err());
        assert!(Shortcut::parse("cmd-a-b").is_err());
        assert!(Shortcut::parse("cmd-unknown").is_err());
    }
}
