//! Zeroed desktop shell. It owns the window, the native menu and the
//! kernel host. All document behaviour lives in `carve-core`.

pub mod kernel_bridge;
pub mod menu;
pub mod session;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, State};

/// Event telling the page that files wait in [`OpenedFiles`].
pub const OPEN_FILES_EVENT: &str = "app://open-files";

/// Event asking the page whether the window may close (its close button or
/// Quit): it answers with [`close_approved`] once unsaved changes are dealt
/// with.
pub const CLOSE_REQUESTED_EVENT: &str = "app://close-requested";

/// Whether closing the window waits for the page. The page arms it once it
/// listens for [`CLOSE_REQUESTED_EVENT`], so a page that never loaded can
/// never keep the app from closing.
#[derive(Debug, Default)]
pub struct CloseGuard {
    armed: AtomicBool,
    approved: AtomicBool,
    edited: AtomicBool,
}

impl CloseGuard {
    /// Arms or disarms the guard.
    pub fn arm(&self, armed: bool) {
        self.armed.store(armed, Ordering::SeqCst);
    }

    /// Records that the page agreed to close.
    pub fn approve(&self) {
        self.approved.store(true, Ordering::SeqCst);
    }

    /// True while a close has to ask the page first.
    pub fn holds(&self) -> bool {
        self.armed.load(Ordering::SeqCst) && !self.approved.load(Ordering::SeqCst)
    }

    /// Records whether the job has unsaved changes.
    pub fn set_edited(&self, edited: bool) {
        self.edited.store(edited, Ordering::SeqCst);
    }

    /// True when a quit from outside the window (the Dock menu, logging
    /// out, a quit event from another app) has to ask the page first. A job
    /// without unsaved changes lets it go at once, so logging out is not
    /// held up for nothing.
    pub fn holds_terminate(&self) -> bool {
        self.holds() && self.edited.load(Ordering::SeqCst)
    }
}

/// Arms the close guard while the page listens for close requests.
#[tauri::command]
fn close_guard(guard: State<'_, CloseGuard>, armed: bool) {
    guard.arm(armed);
}

/// The page agreed to close: the app quits.
#[tauri::command]
fn close_approved(app: AppHandle, guard: State<'_, CloseGuard>) {
    guard.approve();
    app.exit(0);
}

/// Closes and quits at once, or asks the page first while the guard holds.
fn request_close(app: &AppHandle) {
    if !app.state::<CloseGuard>().holds() {
        app.exit(0);
        return;
    }
    if let Err(error) = app.emit(CLOSE_REQUESTED_EVENT, ()) {
        eprintln!("could not emit {CLOSE_REQUESTED_EVENT}: {error}");
        app.exit(0);
    }
}

/// Files the system asked the app to open (a double click in the Finder),
/// kept until the page takes them, since the first arrive before the page
/// has loaded.
#[derive(Debug, Default)]
pub struct OpenedFiles(Mutex<Vec<String>>);

impl OpenedFiles {
    /// Queues `paths` for the page.
    pub fn push(&self, paths: impl IntoIterator<Item = String>) {
        if let Ok(mut queue) = self.0.lock() {
            queue.extend(paths);
        }
    }

    /// Takes every queued path, oldest first.
    pub fn take(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }
}

/// Hands the page the files the system asked the app to open.
#[tauri::command]
fn take_opened_files(opened: State<'_, OpenedFiles>) -> Vec<String> {
    opened.take()
}

/// Rebuilds the menu bar from the menus the page shows.
#[tauri::command]
fn menu_set(
    app: AppHandle,
    icons: State<'_, menu::MenuIcons>,
    menus: Vec<menu::MenuSpec>,
) -> Result<(), String> {
    let images = icons.snapshot();
    let menu = menu::build(&app, Some(&menus), &images).map_err(|error| error.to_string())?;
    app.set_menu(menu).map_err(|error| error.to_string())?;
    #[cfg(target_os = "macos")]
    {
        let predefined = menu::predefined_icons(&menus, &images);
        let checks = menu::check_icons(&menus, &images);
        app.run_on_main_thread(move || menu::finish_images(&predefined, &checks))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Keeps the menu icons the page rendered, as PNG files by icon id, for
/// the menus it sends next.
#[tauri::command]
fn menu_icons(
    icons: State<'_, menu::MenuIcons>,
    images: std::collections::BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    match icons.insert(images) {
        0 => Ok(()),
        dropped => Err(format!(
            "{dropped} menu icons could not be read or had the wrong size"
        )),
    }
}

/// The application data folder: the Zeroed folder that holds the tool
/// database, the gadgets and the imported post processors.
fn data_folder() -> std::path::PathBuf {
    let store = carve_core::cam::tooldb::ToolStore::user_default();
    store
        .path()
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir)
}

/// File > Open Application Data Folder: shows the folder in the Finder.
/// With `sub` set to `posts` it shows the folder installed post processors
/// live in (Post-Processor Management > Open File Location), with
/// `furniture` the maker's furniture templates (Open Library Folder).
#[tauri::command]
fn open_data_folder(sub: Option<String>) -> Result<(), String> {
    let folder = match sub.as_deref() {
        None => data_folder(),
        Some("posts") => carve_core::post::PostLibrary::user_default()
            .user_dir()
            .to_path_buf(),
        Some("furniture") => carve_core::furniture::library::user_default_folder(),
        Some(other) => return Err(format!("no data folder named {other}")),
    };
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(&folder)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Opens a post processor the user installed or customised in the system's
/// text editor. Shipped posts have no file of their own to edit.
#[tauri::command]
fn edit_post(id: String) -> Result<(), String> {
    let library = carve_core::post::PostLibrary::user_default();
    let path = library
        .user_file(&id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("{id} is a shipped post processor: customize it to edit a copy"))?;
    let mut command = if cfg!(target_os = "macos") {
        let mut open = std::process::Command::new("open");
        open.arg("-t");
        open
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("notepad")
    } else {
        std::process::Command::new("xdg-open")
    };
    command
        .arg(&path)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Save Toolpaths with Output direct to machine: a fresh path in the
/// temporary folder for the file `name` the kernel then writes, which
/// [`send_to_driver`] hands to the machine's driver program.
#[tauri::command]
fn direct_output_path(name: String) -> Result<String, String> {
    let file = std::path::Path::new(&name)
        .file_name()
        .ok_or_else(|| format!("{name} is not a file name"))?
        .to_owned();
    let folder = std::env::temp_dir().join("zeroed-direct-output");
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    Ok(folder.join(file).to_string_lossy().into_owned())
}

/// Opens toolpath files in the driver program a post processor names in
/// its `DIRECT_OUTPUT` setting, which sends them on to the machine.
#[tauri::command]
fn send_to_driver(paths: Vec<String>, driver: String) -> Result<(), String> {
    if driver.trim().is_empty() {
        return Err("the post processor names no driver".to_string());
    }
    let mut command = if cfg!(target_os = "macos") {
        let mut open = std::process::Command::new("open");
        open.arg("-a").arg(&driver);
        open
    } else {
        std::process::Command::new(&driver)
    };
    command
        .args(&paths)
        .spawn()
        .map_err(|error| format!("{driver} could not be started: {error}"))?;
    Ok(())
}

/// What the last session left after a crash, once (see [`session`]).
#[tauri::command]
fn session_recovery(session: State<'_, session::Session>) -> session::Recovery {
    session.take_recovery()
}

/// Records the autosave backup of the job's unsaved changes, or none, for
/// the next start to offer if this session crashes.
#[tauri::command]
fn session_backup(session: State<'_, session::Session>, path: Option<String>) {
    session.record_backup(path);
}

/// Deletes an autosave backup the user chose to discard. Only backup files
/// can be deleted this way.
#[tauri::command]
fn discard_backup(path: String) -> Result<(), String> {
    let path = std::path::PathBuf::from(path);
    if !session::is_backup(&path) {
        return Err(format!("{} is not a backup", path.display()));
    }
    std::fs::remove_file(&path).map_err(|error| error.to_string())
}

/// Moves an autosave backup the user keeps for later to a name autosave
/// never writes, and answers that path.
#[tauri::command]
fn keep_backup(path: String) -> Result<String, String> {
    session::keep_backup(std::path::Path::new(&path))
        .map(|kept| kept.display().to_string())
        .map_err(|error| error.to_string())
}

/// Opens a crash report in the text editor. Only files in the crash
/// reports folder can be opened this way.
#[tauri::command]
fn show_crash_report(path: String) -> Result<(), String> {
    let path = std::path::PathBuf::from(path);
    let folder = data_folder().join(session::REPORTS_FOLDER);
    if path.parent() != Some(folder.as_path()) || !path.is_file() {
        return Err(format!("{} is not a crash report", path.display()));
    }
    let mut command = if cfg!(target_os = "macos") {
        let mut open = std::process::Command::new("open");
        open.arg("-t");
        open
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("notepad")
    } else {
        std::process::Command::new("xdg-open")
    };
    command
        .arg(&path)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Marks the window as holding unsaved changes (the dot in its close
/// button) and names the file it shows, so Command-clicking the title lists
/// the folders it is in.
#[tauri::command]
fn set_document_state(
    window: tauri::Window,
    guard: State<'_, CloseGuard>,
    edited: bool,
    path: Option<String>,
) -> Result<(), String> {
    guard.set_edited(edited);
    #[cfg(target_os = "macos")]
    {
        let pointer = window.ns_window().map_err(|error| error.to_string())? as usize;
        window
            .run_on_main_thread(move || {
                use objc2_app_kit::NSWindow;
                use objc2_foundation::NSString;
                // SAFETY: the pointer is the live NSWindow of this Tauri
                // window, used on the main thread while the window exists.
                let ns_window = unsafe { &*(pointer as *const NSWindow) };
                ns_window.setDocumentEdited(edited);
                ns_window
                    .setRepresentedFilename(&NSString::from_str(path.as_deref().unwrap_or("")));
            })
            .map_err(|error| error.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (window, edited, path);
    Ok(())
}

/// Turns on the web view's continuous spell checking, which WebKit leaves
/// off for embedding apps: the text boxes then underline misspelt words.
/// WebKit reads the setting from the user defaults when the first web view
/// is made, so this runs before the window opens.
#[cfg(target_os = "macos")]
fn enable_spell_checking() {
    use objc2_foundation::{NSString, NSUserDefaults};
    let defaults = NSUserDefaults::standardUserDefaults();
    defaults.setBool_forKey(
        true,
        &NSString::from_str("WebContinuousSpellCheckingEnabled"),
    );
}

/// Starts the application. Returns when the last window closes.
pub fn run() {
    #[cfg(target_os = "macos")]
    enable_spell_checking();
    let root = data_folder();
    session::install_panic_hook(
        root.join(session::REPORTS_FOLDER),
        env!("CARGO_PKG_VERSION").to_string(),
    );
    let session = session::Session::begin(
        &root,
        std::process::id(),
        session::now(),
        session::process_alive,
    );
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            kernel_bridge::kernel_call,
            kernel_bridge::kernel_cancel,
            kernel_bridge::kernel_progress,
            menu_set,
            menu_icons,
            open_data_folder,
            edit_post,
            direct_output_path,
            send_to_driver,
            take_opened_files,
            close_guard,
            close_approved,
            session_recovery,
            session_backup,
            discard_backup,
            keep_backup,
            show_crash_report,
            set_document_state
        ])
        .manage(session)
        .manage(OpenedFiles::default())
        .manage(CloseGuard::default())
        .manage(menu::MenuIcons::default())
        .setup(|app| {
            app.manage(kernel_bridge::KernelHost::new());
            #[cfg(target_os = "macos")]
            terminate::install(app.handle());
            let menu = menu::build(app.handle(), None, &Default::default())?;
            app.set_menu(menu)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if app.state::<CloseGuard>().holds() {
                    api.prevent_close();
                    request_close(app);
                }
            }
        })
        .on_menu_event(|app, event| {
            if event.id().0 == menu::QUIT_ID {
                request_close(app);
                return;
            }
            if let Some(index) = menu::recent_index(&event.id().0) {
                if let Err(error) = app.emit(menu::OPEN_RECENT_EVENT, index) {
                    eprintln!("could not emit {}: {error}", menu::OPEN_RECENT_EVENT);
                }
                return;
            }
            if let Some(name) = menu::gadget_name(&event.id().0) {
                if let Err(error) = app.emit(menu::GADGET_EVENT, name) {
                    eprintln!("could not emit {}: {error}", menu::GADGET_EVENT);
                }
                return;
            }
            if let Err(error) = app.emit(menu::COMMAND_EVENT, &event.id().0) {
                eprintln!("could not emit {}: {error}", menu::COMMAND_EVENT);
            }
        })
        .build(tauri::generate_context!());

    match result {
        Ok(app) => app.run(on_run_event),
        Err(error) => {
            eprintln!("Zeroed could not start: {error}");
            std::process::exit(1);
        }
    }
}

/// Queues the files the system asks the app to open and tells the page,
/// and keeps an exit the event loop asks for until the page agrees.
fn on_run_event(app: &AppHandle, event: tauri::RunEvent) {
    if let tauri::RunEvent::Exit = event {
        app.state::<session::Session>().end();
        return;
    }
    if let tauri::RunEvent::ExitRequested { api, code, .. } = &event {
        if code.is_none() && app.state::<CloseGuard>().holds() {
            api.prevent_exit();
            request_close(app);
        }
        return;
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    if let tauri::RunEvent::Opened { urls } = event {
        let paths: Vec<String> = urls
            .iter()
            .filter_map(|url| url.to_file_path().ok())
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        if paths.is_empty() {
            return;
        }
        app.state::<OpenedFiles>().push(paths);
        if let Err(error) = app.emit(OPEN_FILES_EVENT, ()) {
            eprintln!("could not emit {OPEN_FILES_EVENT}: {error}");
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    let _ = (app, event);
}

/// The system's quit path. The Dock menu's Quit, logging out, restarting
/// and a quit event from another app all go through `terminate:` of the
/// application, which the event loop lets through without asking. The
/// application delegate gets `applicationShouldTerminate:`, which asks the
/// page first while the job has unsaved changes, as the close button and
/// the Quit item do.
#[cfg(target_os = "macos")]
mod terminate {
    use std::sync::OnceLock;

    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::MainThreadMarker;
    use objc2::{ffi, sel};
    use objc2_app_kit::NSApplication;
    use tauri::{AppHandle, Manager};

    use super::{request_close, CloseGuard};

    static APP: OnceLock<AppHandle> = OnceLock::new();

    /// `NSTerminateCancel` and `NSTerminateNow`.
    const CANCEL: usize = 0;
    const NOW: usize = 1;

    extern "C-unwind" fn should_terminate(
        _delegate: &AnyObject,
        _selector: Sel,
        _sender: *mut AnyObject,
    ) -> usize {
        let Some(app) = APP.get() else {
            return NOW;
        };
        if !app.state::<CloseGuard>().holds_terminate() {
            return NOW;
        }
        // The page asks and quits the app itself once the user agrees, so
        // this quit stops here. Logging out waits for the answer: macOS
        // says the app stopped it and the user logs out again.
        request_close(app);
        CANCEL
    }

    /// Gives the application delegate `applicationShouldTerminate:`.
    pub fn install(app: &AppHandle) {
        let _ = APP.set(app.clone());
        let Some(mtm) = MainThreadMarker::new() else {
            eprintln!("the quit handler must be installed on the main thread");
            return;
        };
        let Some(delegate) = NSApplication::sharedApplication(mtm).delegate() else {
            eprintln!("the application has no delegate to hold unsaved work on quit");
            return;
        };
        let handler: extern "C-unwind" fn(&AnyObject, Sel, *mut AnyObject) -> usize =
            should_terminate;
        // SAFETY: the class is the live delegate's own class; the method
        // added has the signature its type encoding states (an unsigned
        // long reply, the receiver, the selector and the sender), and
        // class_addMethod leaves a method the class already has untouched.
        let added = unsafe {
            let object: *const AnyObject = (&*delegate as *const _) as *const AnyObject;
            let class = ffi::object_getClass(object) as *mut AnyClass;
            let imp: Imp = std::mem::transmute::<
                extern "C-unwind" fn(&AnyObject, Sel, *mut AnyObject) -> usize,
                Imp,
            >(handler);
            ffi::class_addMethod(
                class,
                sel!(applicationShouldTerminate:),
                imp,
                c"Q@:@".as_ptr(),
            )
        };
        if !added.as_bool() {
            eprintln!("the application delegate already answers applicationShouldTerminate:");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CloseGuard, OpenedFiles};

    #[test]
    fn a_quit_from_outside_asks_only_about_unsaved_work() {
        let guard = CloseGuard::default();
        guard.set_edited(true);
        assert!(!guard.holds_terminate(), "no page listens yet");
        guard.arm(true);
        assert!(guard.holds_terminate());
        guard.set_edited(false);
        assert!(!guard.holds_terminate(), "a saved job quits at once");
        guard.set_edited(true);
        guard.approve();
        assert!(!guard.holds_terminate(), "the page agreed");
    }

    #[test]
    fn close_waits_for_the_page_only_while_it_listens() {
        let guard = CloseGuard::default();
        assert!(
            !guard.holds(),
            "a page that never loaded cannot block closing"
        );
        guard.arm(true);
        assert!(guard.holds());
        guard.approve();
        assert!(!guard.holds());
        let unarmed = CloseGuard::default();
        unarmed.arm(true);
        unarmed.arm(false);
        assert!(!unarmed.holds());
    }

    #[test]
    fn opened_files_are_taken_once_in_order() {
        let opened = OpenedFiles::default();
        opened.push(["/a.carve".to_string()]);
        opened.push(["/b.svg".to_string(), "/c.stl".to_string()]);
        assert_eq!(opened.take(), ["/a.carve", "/b.svg", "/c.stl"]);
        assert!(opened.take().is_empty());
    }
}
