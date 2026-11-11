use std::cell::{OnceCell, RefCell};
use std::path::{Path, PathBuf};

use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_foundation::{NSInteger, NSString, NSURL};
use objc2_quick_look_ui::{QLPreviewItem, QLPreviewPanel, QLPreviewPanelDataSource};

define_class!(
    /// The panel's data source. It holds the one file the panel shows.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = RefCell<Option<PathBuf>>]
    #[name = "HerdrQuickLookSource"]
    struct Source;

    unsafe impl NSObjectProtocol for Source {}

    unsafe impl QLPreviewPanelDataSource for Source {
        #[unsafe(method(numberOfPreviewItemsInPreviewPanel:))]
        fn number_of_items(&self, _panel: Option<&QLPreviewPanel>) -> NSInteger {
            self.ivars().borrow().is_some() as NSInteger
        }

        #[unsafe(method_id(previewPanel:previewItemAtIndex:))]
        fn item(&self, _panel: Option<&QLPreviewPanel>, _index: NSInteger) -> Option<Retained<ProtocolObject<dyn QLPreviewItem>>> {
            self.ivars().borrow().as_ref().map(|path| {
                let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
                ProtocolObject::from_retained(url)
            })
        }
    }
);

impl Source {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(RefCell::new(None));
        unsafe { msg_send![super(this), init] }
    }
}

thread_local! {
    /// The panel only keeps a weak reference to its data source.
    static SOURCE: OnceCell<Retained<Source>> = const { OnceCell::new() };
}

/// Shows `path` in the Quick Look panel, or closes the panel when it already shows `path`.
///
/// Safe to call from GPUI event handling. The panel's AppKit calls reenter the
/// main queue while they run (the window reorders, and queued GPUI tasks run
/// there), so the panel work is its own main-queue block: it runs after the
/// event's update has returned and the app is no longer borrowed, and the
/// reentrant GPUI tasks find nothing borrowed while the panel comes up.
pub fn toggle(path: &Path) {
    let path: Box<PathBuf> = Box::new(path.to_path_buf());
    unsafe { DispatchQueue::main().exec_async_f(Box::into_raw(path).cast(), toggle_block) };
}

extern "C" fn toggle_block(context: *mut std::ffi::c_void) {
    let path = unsafe { Box::from_raw(context.cast::<PathBuf>()) };
    toggle_on_main_thread(&path);
}

/// The panel work, on the main thread, with no GPUI borrow on the stack.
fn toggle_on_main_thread(path: &Path) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(panel) = (unsafe { QLPreviewPanel::sharedPreviewPanel(mtm) }) else { return };
    SOURCE.with(|source| {
        let source = source.get_or_init(|| Source::new(mtm));
        // No borrow is held across a panel call: the panel can ask the data source back.
        if panel.isVisible() && source.ivars().borrow().as_deref() == Some(path) {
            panel.orderOut(None);
            return;
        }
        source.ivars().replace(Some(path.to_path_buf()));
        // Nothing in Herdr's responder chain takes control of the panel, so the
        // data source set here stays through `updateController`.
        unsafe { panel.setDataSource(Some(ProtocolObject::from_ref(&**source))) };
        if panel.isVisible() {
            unsafe { panel.reloadData() };
        } else {
            panel.makeKeyAndOrderFront(None);
        }
    });
}
