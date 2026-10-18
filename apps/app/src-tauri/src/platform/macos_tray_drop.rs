//! macOS: dragging files onto the menu bar icon opens a device panel right
//! under it; dropping on a device row sends the files there (ADR-0022).
//!
//! The status item's button lives in a private `NSStatusBarWindow`. No view
//! in it registers for dragged types, so AppKit treats the window itself as
//! the destination and forwards the dragging messages to its delegate. Since
//! macOS 26 the visible icon is drawn by Control Center, which still
//! forwards drags to this window.

#![allow(unsafe_code)]

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSDragOperation, NSDraggingDestination, NSDraggingInfo, NSEvent, NSEventMask,
    NSPasteboardTypeFileURL, NSScreen, NSWindow, NSWindowCollectionBehavior, NSWindowDelegate,
};
use objc2_foundation::{NSArray, NSURL};
use std::cell::RefCell;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, WebviewUrl, WebviewWindowBuilder};

const PANEL_LABEL: &str = "tray-drop";
const PANEL_WIDTH: f64 = 300.0;
const PANEL_HEIGHT: f64 = 380.0;
const TRAY_ID: &str = "main";

/// Mirror of `settings.app.menu_bar_drop`, read from AppKit callbacks.
static ENABLED: AtomicBool = AtomicBool::new(true);

pub struct Ivars {
    app: AppHandle,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "LanSendTrayDropTarget"]
    #[ivars = Ivars]
    pub struct TrayDropTarget;

    unsafe impl NSObjectProtocol for TrayDropTarget {}

    unsafe impl NSWindowDelegate for TrayDropTarget {}

    unsafe impl NSDraggingDestination for TrayDropTarget {
        #[unsafe(method(draggingEntered:))]
        fn dragging_entered(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            if !accepts(sender) {
                return NSDragOperation::None;
            }
            let app = &self.ivars().app;
            if !show_panel(app, sender.draggingDestinationWindow().as_deref()) {
                return NSDragOperation::None;
            }
            let _ = app.emit_to(PANEL_LABEL, "event:tray-drop-enter", ());
            NSDragOperation::Copy
        }

        #[unsafe(method(draggingUpdated:))]
        fn dragging_updated(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            if accepts(sender) {
                NSDragOperation::Copy
            } else {
                NSDragOperation::None
            }
        }

        #[unsafe(method(draggingExited:))]
        fn dragging_exited(&self, _sender: Option<&ProtocolObject<dyn NSDraggingInfo>>) {
            let _ = self
                .ivars()
                .app
                .emit_to(PANEL_LABEL, "event:tray-drop-exit", ());
        }

        /// Dropped on the icon itself: the panel stays open with the files
        /// staged until a device is clicked or the user clicks elsewhere.
        #[unsafe(method(performDragOperation:))]
        fn perform_drag_operation(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> Bool {
            let paths = file_paths(sender);
            if paths.is_empty() {
                return Bool::NO;
            }
            let app = &self.ivars().app;
            let _ = app.emit_to(PANEL_LABEL, "event:tray-drop-staged", paths);
            dismiss_on_outside_click(app);
            Bool::YES
        }
    }
);

impl TrayDropTarget {
    fn new(mtm: MainThreadMarker, app: AppHandle) -> Retained<Self> {
        let this = mtm.alloc::<Self>().set_ivars(Ivars { app });
        unsafe { msg_send![super(this), init] }
    }
}

thread_local! {
    /// `NSWindow.delegate` is weak; the target lives as long as the app.
    static TARGET: RefCell<Option<Retained<TrayDropTarget>>> = const { RefCell::new(None) };
    /// One-shot global mouse monitor while files are staged.
    static MONITOR: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
}

/// Makes the status item's window a drop destination. Runs from `setup`,
/// after the tray icon exists; the panel follows the settings later.
pub fn setup(app: &AppHandle) -> anyhow::Result<()> {
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| anyhow::anyhow!("no tray icon to attach the drop target to"))?;
    let handle = app.clone();
    let installed = tray.with_inner_tray_icon(move |inner| {
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        let Some(window) = inner
            .ns_status_item()
            .and_then(|item| item.button(mtm))
            .and_then(|button| button.window())
        else {
            return false;
        };
        let target = TrayDropTarget::new(mtm, handle);
        window.registerForDraggedTypes(&NSArray::from_slice(&[file_url_type()]));
        window.setDelegate(Some(ProtocolObject::from_ref(&*target)));
        TARGET.with(|cell| cell.replace(Some(target)));
        true
    })?;
    anyhow::ensure!(installed, "the menu bar icon has no window yet");
    Ok(())
}

/// Applies `settings.app.menu_bar_drop`: the hidden panel (one extra web
/// view) only exists while the feature is on.
pub fn apply_settings(app: &AppHandle, enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    let existing = app.get_webview_window(PANEL_LABEL);
    match (enabled, existing) {
        (true, None) => {
            if let Err(err) = build_panel(app) {
                tracing::warn!("menu bar drop panel not created: {err}");
            }
        }
        (false, Some(panel)) => {
            let _ = panel.destroy();
        }
        _ => {}
    }
}

fn build_panel(app: &AppHandle) -> tauri::Result<()> {
    let panel = WebviewWindowBuilder::new(
        app,
        PANEL_LABEL,
        WebviewUrl::App("index.html?panel=tray-drop".into()),
    )
    .title("LanSend")
    .inner_size(PANEL_WIDTH, PANEL_HEIGHT)
    .resizable(false)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible_on_all_workspaces(true)
    .focused(false)
    .visible(false)
    .shadow(true)
    // Rows react to the first click although the app is not active.
    .accept_first_mouse(true)
    .build()?;
    // Pointers are not Send; the address is only dereferenced on the main
    // thread, where the panel's NSWindow lives as long as the panel.
    let address = panel.ns_window()? as usize;
    app.run_on_main_thread(move || {
        // SAFETY: see above; Tauri hands out the panel's live NSWindow.
        let window = unsafe { &*(address as *const NSWindow) };
        // Also over full-screen apps, and out of the ⌘` window cycle.
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        window.setHidesOnDeactivate(false);
    })?;
    Ok(())
}

fn file_url_type() -> &'static objc2_app_kit::NSPasteboardType {
    // SAFETY: an AppKit constant, valid for the process lifetime.
    unsafe { NSPasteboardTypeFileURL }
}

fn accepts(sender: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
    ENABLED.load(Ordering::Relaxed)
        && sender
            .draggingPasteboard()
            .availableTypeFromArray(&NSArray::from_slice(&[file_url_type()]))
            .is_some()
}

/// File URLs on the dragging pasteboard. Finder hands out file reference
/// URLs (`file:///.file/id=…`), hence `filePathURL`. Inside the App Sandbox
/// reading them here is what grants access to the files.
fn file_paths(sender: &ProtocolObject<dyn NSDraggingInfo>) -> Vec<String> {
    let Some(items) = sender.draggingPasteboard().pasteboardItems() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.stringForType(file_url_type()))
        .filter_map(|text| NSURL::URLWithString(&text))
        .filter_map(|url| url.filePathURL().and_then(|url| url.path()))
        .map(|path| path.to_string())
        .collect()
}

/// Puts the panel under the status item and shows it without activating
/// the app. Cocoa screen coordinates start at the bottom left of the
/// primary screen; Tauri's logical positions at its top left.
fn show_panel(app: &AppHandle, status_window: Option<&NSWindow>) -> bool {
    let Some(panel) = app.get_webview_window(PANEL_LABEL) else {
        return false;
    };
    if let (Some(window), Some(mtm)) = (status_window, MainThreadMarker::new()) {
        let item = window.frame();
        let primary_height = NSScreen::screens(mtm)
            .firstObject()
            .map(|screen| screen.frame().size.height)
            .unwrap_or(0.0);
        let mut x = item.origin.x + item.size.width / 2.0 - PANEL_WIDTH / 2.0;
        if let Some(screen) = window.screen() {
            let visible = screen.visibleFrame();
            let max_x = visible.origin.x + visible.size.width - PANEL_WIDTH - 8.0;
            x = x.min(max_x).max(visible.origin.x + 8.0);
        }
        let y = primary_height - item.origin.y + 4.0;
        let _ = panel.set_position(LogicalPosition::new(x, y));
    }
    panel.show().is_ok()
}

/// The panel is never key (that would activate the app), so a click
/// elsewhere is noticed through a global monitor, which only sees clicks
/// meant for other apps. It removes itself after the first one.
fn dismiss_on_outside_click(app: &AppHandle) {
    remove_monitor();
    let handle = app.clone();
    let block = RcBlock::new(move |_event: NonNull<NSEvent>| {
        let _ = handle.emit_to(PANEL_LABEL, "event:tray-drop-dismiss", ());
        // Not from inside the handler: removing the monitor releases this
        // block. From another thread the task is queued for later.
        let handle = handle.clone();
        std::thread::spawn(move || {
            let _ = handle.run_on_main_thread(remove_monitor);
        });
    });
    let monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
        NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::OtherMouseDown,
        &block,
    );
    MONITOR.with(|cell| cell.replace(monitor));
}

fn remove_monitor() {
    if let Some(monitor) = MONITOR.with(|cell| cell.take()) {
        // SAFETY: the object came from `addGlobalMonitorForEventsMatchingMask`.
        unsafe { NSEvent::removeMonitor(&monitor) };
    }
}
