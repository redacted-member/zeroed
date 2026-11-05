//! Native macOS menu bar.
//!
//! The page owns the menus: it builds them from its stores (the same tree
//! the in-window menu bar of the browser build shows) and sends them with
//! the `menu_set` command whenever they change, so recent files, gadgets,
//! check marks and enabled items stay current. This module turns that tree
//! into a Tauri menu under the macOS app menu. Until the page has sent its
//! menus, the bar holds the app menu and an Edit menu whose cut, copy and
//! paste work in text fields.
//!
//! Items and submenus may name an icon: the page renders its icons in
//! color, for the current light or dark appearance, to [`ICON_PX`] square
//! PNG images and sends them with `menu_icons` ([`MenuIcons`]), again when
//! the appearance changes. Enabled items draw them in color; disabled items
//! draw them as template images, so macOS greys them with the label.
//! Check and radio items show their icon too, beside the check mark: the
//! menu builder makes them without an image, so [`finish_images`] sets it.
//!
//! Every custom item emits [`COMMAND_EVENT`] with its id as the payload.
//! Open Recent and gadget items carry their index or name in the id and
//! emit their own events. Cut, copy and paste stay predefined so that they
//! work in text fields of the web view.

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::Deserialize;
use tauri::image::Image;
use tauri::menu::{
    AboutMetadata, CheckMenuItemBuilder, IconMenuItemBuilder, Menu, MenuItemBuilder,
    PredefinedMenuItem, Submenu, SubmenuBuilder,
};
use tauri::{AppHandle, Runtime};

/// Side of a menu icon in pixels: 18 points at twice the pixel density,
/// the height the menu draws item images at.
pub const ICON_PX: u32 = 36;

/// The menu icons the page sent, as RGBA images keyed by icon id.
#[derive(Debug, Default)]
pub struct MenuIcons(Mutex<BTreeMap<String, Vec<u8>>>);

impl MenuIcons {
    /// Keeps the icons of `images` (PNG files of [`ICON_PX`] square), in
    /// place of those kept under the same id. Answers how many images could
    /// not be read or had the wrong size and were dropped.
    pub fn insert(&self, images: BTreeMap<String, Vec<u8>>) -> usize {
        let mut dropped = 0;
        if let Ok(mut icons) = self.0.lock() {
            for (id, png) in images {
                match png_rgba(&png) {
                    Some(rgba) => {
                        icons.insert(id, rgba);
                    }
                    None => dropped += 1,
                }
            }
        }
        dropped
    }

    /// A copy of the kept icons.
    pub fn snapshot(&self) -> BTreeMap<String, Vec<u8>> {
        self.0.lock().map(|icons| icons.clone()).unwrap_or_default()
    }
}

/// The RGBA pixels of the PNG file `png`, or none when it cannot be read
/// or is not [`ICON_PX`] square.
pub fn png_rgba(png: &[u8]) -> Option<Vec<u8>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buf).ok()?;
    if frame.width != ICON_PX || frame.height != ICON_PX {
        return None;
    }
    let pixels = &buf[..frame.buffer_size()];
    let rgba = match frame.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => pixels
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(rgba)
}

/// Event a menu item emits, with the item id as payload. Tauri event names
/// allow no dots, so the id cannot be part of the name.
pub const COMMAND_EVENT: &str = "menu:command";

/// Id prefix of the Open Recent items; the rest of the id is the index into
/// the recent files list.
pub const RECENT_PREFIX: &str = "file.recent.";

/// Event the Open Recent items emit, with the list index as payload.
pub const OPEN_RECENT_EVENT: &str = "menu:open-recent";

/// Id prefix of the gadget items of the Gadgets menu; the rest of the id is
/// the gadget name.
pub const GADGET_PREFIX: &str = "gadget.";

/// Event a gadget item emits, with the gadget name as payload.
pub const GADGET_EVENT: &str = "menu:gadget";

/// Id of the Settings item of the app menu.
pub const SETTINGS_ID: &str = "app.preferences";

/// The app menu's Quit item. It is the shell's own item, not the system
/// one, so a job with unsaved changes can ask before the app goes.
pub const QUIT_ID: &str = "app.quit";

/// Index into the recent files list of an Open Recent item id.
pub fn recent_index(id: &str) -> Option<usize> {
    id.strip_prefix(RECENT_PREFIX)?.parse().ok()
}

/// The gadget name of a gadget item id.
pub fn gadget_name(id: &str) -> Option<&str> {
    id.strip_prefix(GADGET_PREFIX)
        .filter(|name| !name.is_empty())
}

/// One menu of the bar, as the page sends it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MenuSpec {
    /// Title in the menu bar.
    pub label: String,
    /// Items, separators and submenus, top to bottom.
    pub entries: Vec<EntrySpec>,
}

/// An entry of a menu.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EntrySpec {
    /// A command item.
    Item {
        /// Emitted as the event payload.
        id: String,
        /// Menu label.
        label: String,
        /// Tauri accelerator such as `CmdOrCtrl+Z`, or none.
        #[serde(default)]
        accelerator: Option<String>,
        /// Greyed out when false.
        enabled: bool,
        /// Some for a check or radio item, with its state.
        #[serde(default)]
        checked: Option<bool>,
        /// Icon id; check items draw it beside their mark.
        #[serde(default)]
        icon: Option<String>,
    },
    /// A line between groups of items.
    Separator,
    /// A nested menu.
    Submenu {
        /// Menu label.
        label: String,
        /// Its entries.
        entries: Vec<EntrySpec>,
        /// Icon id.
        #[serde(default)]
        icon: Option<String>,
    },
    /// A system item that must stay native: `cut`, `copy` or `paste`.
    Predefined {
        /// Which one.
        name: String,
        /// Icon id.
        #[serde(default)]
        icon: Option<String>,
    },
}

/// Builds the menu bar: the app menu, then `menus` with the images of
/// `icons`, or the start up Edit menu while `menus` is `None`.
///
/// # Errors
/// Returns the Tauri error if a menu item cannot be created.
pub fn build<R: Runtime>(
    app: &AppHandle<R>,
    menus: Option<&[MenuSpec]>,
    icons: &BTreeMap<String, Vec<u8>>,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    menu.append(&app_menu(app)?)?;
    match menus {
        Some(menus) => {
            for spec in menus {
                menu.append(&submenu(app, &spec.label, &spec.entries, icons)?)?;
            }
        }
        None => {
            let edit = SubmenuBuilder::new(app, "Edit")
                .cut()
                .copy()
                .paste()
                .select_all()
                .build()?;
            menu.append(&edit)?;
        }
    }
    Ok(menu)
}

/// The macOS app menu: About, Settings, Services, Hide and Quit.
fn app_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    let settings = MenuItemBuilder::with_id(SETTINGS_ID, "Settings...")
        .accelerator("CmdOrCtrl+,")
        .build(app)?;
    let quit = MenuItemBuilder::with_id(QUIT_ID, "Quit Zeroed")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;
    SubmenuBuilder::new(app, "Zeroed")
        .about(Some(about_metadata(app)))
        .separator()
        .item(&settings)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .item(&quit)
        .build()
}

/// What the standard About panel shows: the name and the one version the
/// app carries (package.json, which tauri.conf.json reads and the Cargo
/// workspace matches).
fn about_metadata<R: Runtime>(app: &AppHandle<R>) -> AboutMetadata<'static> {
    let info = app.package_info();
    let version = info.version.to_string();
    AboutMetadata {
        name: Some(info.name.clone()),
        version: Some(version.clone()),
        short_version: Some(version),
        ..AboutMetadata::default()
    }
}

/// The image of icon `id`, if the page sent it.
fn icon_image<'a>(icons: &'a BTreeMap<String, Vec<u8>>, id: Option<&String>) -> Option<Image<'a>> {
    let rgba = icons.get(id?)?;
    Some(Image::new(rgba, ICON_PX, ICON_PX))
}

fn submenu<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
    entries: &[EntrySpec],
    icons: &BTreeMap<String, Vec<u8>>,
) -> tauri::Result<Submenu<R>> {
    let menu = Submenu::new(app, label, true)?;
    for entry in entries {
        match entry {
            EntrySpec::Item {
                id,
                label,
                accelerator,
                enabled,
                checked,
                icon,
            } => match checked {
                Some(checked) => {
                    let mut builder = CheckMenuItemBuilder::with_id(id.as_str(), label)
                        .enabled(*enabled)
                        .checked(*checked);
                    if let Some(accelerator) = accelerator {
                        builder = builder.accelerator(accelerator);
                    }
                    menu.append(&builder.build(app)?)?;
                }
                None => match icon_image(icons, icon.as_ref()) {
                    Some(image) => {
                        let mut builder = IconMenuItemBuilder::with_id(id.as_str(), label)
                            .enabled(*enabled)
                            .icon(image);
                        if let Some(accelerator) = accelerator {
                            builder = builder.accelerator(accelerator);
                        }
                        menu.append(&builder.build(app)?)?;
                    }
                    None => {
                        let mut builder =
                            MenuItemBuilder::with_id(id.as_str(), label).enabled(*enabled);
                        if let Some(accelerator) = accelerator {
                            builder = builder.accelerator(accelerator);
                        }
                        menu.append(&builder.build(app)?)?;
                    }
                },
            },
            EntrySpec::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
            EntrySpec::Submenu {
                label,
                entries,
                icon,
            } => {
                let child = submenu(app, label, entries, icons)?;
                if let Some(image) = icon_image(icons, icon.as_ref()) {
                    child.set_icon(Some(image))?;
                }
                menu.append(&child)?;
            }
            EntrySpec::Predefined { name, .. } => match name.as_str() {
                "cut" => menu.append(&PredefinedMenuItem::cut(app, None)?)?,
                "copy" => menu.append(&PredefinedMenuItem::copy(app, None)?)?,
                "paste" => menu.append(&PredefinedMenuItem::paste(app, None)?)?,
                _ => {}
            },
        }
    }
    Ok(menu)
}

/// The icons of the system items (cut, copy and paste) that `menus` name,
/// keyed by the action selector of the item, as RGBA images.
pub fn predefined_icons(
    menus: &[MenuSpec],
    icons: &BTreeMap<String, Vec<u8>>,
) -> BTreeMap<String, Vec<u8>> {
    fn visit(
        entries: &[EntrySpec],
        icons: &BTreeMap<String, Vec<u8>>,
        out: &mut BTreeMap<String, Vec<u8>>,
    ) {
        for entry in entries {
            match entry {
                EntrySpec::Predefined {
                    name,
                    icon: Some(icon),
                } => {
                    if let Some(rgba) = icons.get(icon) {
                        out.insert(format!("{name}:"), rgba.clone());
                    }
                }
                EntrySpec::Submenu { entries, .. } => visit(entries, icons, out),
                _ => {}
            }
        }
    }
    let mut out = BTreeMap::new();
    for menu in menus {
        visit(&menu.entries, icons, &mut out);
    }
    out
}

/// Separates the titles of the menus and the item in a [`check_icons`] key.
const PATH_SEP: char = '\u{1f}';

/// The icons of the check and radio items of `menus`, as RGBA images,
/// keyed by the titles from the menu down to the item joined by
/// [`PATH_SEP`] (titles are unique within a menu).
pub fn check_icons(
    menus: &[MenuSpec],
    icons: &BTreeMap<String, Vec<u8>>,
) -> BTreeMap<String, Vec<u8>> {
    fn visit(
        entries: &[EntrySpec],
        path: &str,
        icons: &BTreeMap<String, Vec<u8>>,
        out: &mut BTreeMap<String, Vec<u8>>,
    ) {
        for entry in entries {
            match entry {
                EntrySpec::Item {
                    label,
                    checked: Some(_),
                    icon: Some(icon),
                    ..
                } => {
                    if let Some(rgba) = icons.get(icon) {
                        out.insert(format!("{path}{PATH_SEP}{label}"), rgba.clone());
                    }
                }
                EntrySpec::Submenu { label, entries, .. } => {
                    visit(entries, &format!("{path}{PATH_SEP}{label}"), icons, out);
                }
                _ => {}
            }
        }
    }
    let mut out = BTreeMap::new();
    for menu in menus {
        visit(&menu.entries, &menu.label, icons, &mut out);
    }
    out
}

/// Finishes the images of the menu bar: the image of a disabled item
/// becomes a template image, so the menu greys it like its label, and an
/// enabled item keeps its colors; the system items
/// whose action selector `predefined` names get that image, the check
/// items whose title path `checks` names ([`check_icons`]) theirs; and in a menu
/// where any item has an image, the others get an empty one so that every
/// label starts at the same place. Runs on the main thread.
#[cfg(target_os = "macos")]
pub fn finish_images(predefined: &BTreeMap<String, Vec<u8>>, checks: &BTreeMap<String, Vec<u8>>) {
    use objc2::rc::Retained;
    use objc2::{AllocAnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage, NSMenu};
    use objc2_foundation::{NSData, NSSize};

    /// Side of a menu image in points.
    const POINTS: f64 = 18.0;

    fn image(rgba: &[u8]) -> Option<Retained<NSImage>> {
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, ICON_PX, ICON_PX);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().ok()?;
            writer.write_image_data(rgba).ok()?;
        }
        let image = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(&png))?;
        image.setSize(NSSize::new(POINTS, POINTS));
        Some(image)
    }

    fn finish(
        menu: &NSMenu,
        path: &str,
        predefined: &BTreeMap<String, Vec<u8>>,
        checks: &BTreeMap<String, Vec<u8>>,
    ) {
        let items: Vec<_> = (0..menu.numberOfItems())
            .filter_map(|index| menu.itemAtIndex(index))
            .filter(|item| !item.isSeparatorItem())
            .collect();
        for item in &items {
            let here = format!("{path}{PATH_SEP}{}", item.title());
            if item.image().is_none() {
                let selector = item.action().map(|action| action.name().to_string_lossy());
                let rgba = selector
                    .and_then(|name| predefined.get(name.as_ref()))
                    .or_else(|| checks.get(&here));
                if let Some(image) = rgba.and_then(|rgba| image(rgba)) {
                    item.setImage(Some(&image));
                }
            }
            if let Some(child) = item.submenu() {
                finish(&child, &here, predefined, checks);
            }
        }
        if !items.iter().any(|item| item.image().is_some()) {
            return;
        }
        let blank = NSImage::initWithSize(NSImage::alloc(), NSSize::new(POINTS, POINTS));
        for item in &items {
            match item.image() {
                Some(image) => image.setTemplate(!item.isEnabled()),
                None => item.setImage(Some(&blank)),
            }
        }
    }

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(bar) = NSApplication::sharedApplication(mtm).mainMenu() else {
        return;
    };
    // The top level holds the menus themselves; the app menu keeps its look.
    for index in 1..bar.numberOfItems() {
        let Some(item) = bar.itemAtIndex(index) else {
            continue;
        };
        if let Some(child) = item.submenu() {
            finish(&child, &item.title().to_string(), predefined, checks);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_ids_parse_back_to_their_index() {
        assert_eq!(recent_index("file.recent.0"), Some(0));
        assert_eq!(recent_index("file.recent.12"), Some(12));
        assert_eq!(recent_index("file.recent.x"), None);
        assert_eq!(recent_index("file.open"), None);
    }

    #[test]
    fn gadget_ids_parse_back_to_their_name() {
        assert_eq!(gadget_name("gadget.gear"), Some("gear"));
        assert_eq!(gadget_name("gadget."), None);
        assert_eq!(gadget_name("gadgets.refresh"), None);
    }

    #[test]
    fn the_page_menus_parse() {
        let json = r#"[{ "label": "Edit", "entries": [
            { "type": "item", "id": "edit.undo", "label": "Undo",
              "accelerator": "CmdOrCtrl+Z", "enabled": true, "checked": null },
            { "type": "separator" },
            { "type": "predefined", "name": "cut" },
            { "type": "submenu", "label": "Selection", "entries": [
                { "type": "item", "id": "view.guidesVisible", "label": "Guides Visible",
                  "accelerator": null, "enabled": false, "checked": true }
            ]}
        ]}]"#;
        let menus: Vec<MenuSpec> = serde_json::from_str(json).expect("parses");
        let EntrySpec::Item { icon, .. } = &menus[0].entries[0] else {
            panic!("not an item");
        };
        assert_eq!(icon, &None);
        assert_eq!(menus[0].entries.len(), 4);
        assert_eq!(menus[0].entries[1], EntrySpec::Separator);
        let EntrySpec::Submenu { entries, .. } = &menus[0].entries[3] else {
            panic!("not a submenu");
        };
        assert_eq!(
            entries[0],
            EntrySpec::Item {
                id: "view.guidesVisible".into(),
                label: "Guides Visible".into(),
                accelerator: None,
                enabled: false,
                checked: Some(true),
                icon: None,
            }
        );
    }

    #[test]
    fn items_and_submenus_carry_their_icon() {
        let json = r#"[{ "label": "File", "entries": [
            { "type": "item", "id": "file.open", "label": "Open...", "accelerator": null,
              "enabled": true, "checked": null, "icon": "fileOpen" },
            { "type": "submenu", "label": "Import", "icon": "importVectors", "entries": [] }
        ]}]"#;
        let menus: Vec<MenuSpec> = serde_json::from_str(json).expect("parses");
        let EntrySpec::Item { icon, .. } = &menus[0].entries[0] else {
            panic!("not an item");
        };
        assert_eq!(icon.as_deref(), Some("fileOpen"));
        let EntrySpec::Submenu { icon, .. } = &menus[0].entries[1] else {
            panic!("not a submenu");
        };
        assert_eq!(icon.as_deref(), Some("importVectors"));
    }

    #[test]
    fn system_items_find_their_icons_by_action() {
        let icons = BTreeMap::from([("editCut".to_string(), vec![7u8; 4])]);
        let menus = vec![MenuSpec {
            label: "Edit".into(),
            entries: vec![EntrySpec::Submenu {
                label: "More".into(),
                icon: None,
                entries: vec![
                    EntrySpec::Predefined {
                        name: "cut".into(),
                        icon: Some("editCut".into()),
                    },
                    EntrySpec::Predefined {
                        name: "copy".into(),
                        icon: Some("editCopy".into()),
                    },
                ],
            }],
        }];
        let found = predefined_icons(&menus, &icons);
        assert_eq!(found.keys().collect::<Vec<_>>(), ["cut:"]);
    }

    #[test]
    fn check_items_find_their_icons_by_title_path() {
        let icons = BTreeMap::from([
            ("guidesVisible".to_string(), vec![5u8; 4]),
            ("origin".to_string(), vec![6u8; 4]),
        ]);
        let check = |label: &str, icon: Option<&str>| EntrySpec::Item {
            id: label.to_lowercase(),
            label: label.into(),
            accelerator: None,
            enabled: true,
            checked: Some(true),
            icon: icon.map(Into::into),
        };
        let menus = vec![MenuSpec {
            label: "View".into(),
            entries: vec![
                check("Draw Origin", Some("origin")),
                check("No Icon", None),
                check("Unsent Icon", Some("missing")),
                EntrySpec::Item {
                    id: "view.zoomToJob".into(),
                    label: "Zoom".into(),
                    accelerator: None,
                    enabled: true,
                    checked: None,
                    icon: Some("origin".into()),
                },
                EntrySpec::Submenu {
                    label: "Guide Lines".into(),
                    icon: None,
                    entries: vec![check("Guides Visible", Some("guidesVisible"))],
                },
            ],
        }];
        let found = check_icons(&menus, &icons);
        let keys: Vec<String> = found.keys().map(|k| k.replace(PATH_SEP, "/")).collect();
        assert_eq!(
            keys,
            ["View/Draw Origin", "View/Guide Lines/Guides Visible"]
        );
        assert_eq!(
            found["View\u{1f}Guide Lines\u{1f}Guides Visible"],
            vec![5u8; 4]
        );
    }

    fn encode(side: u32, color: png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, side, side);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header");
        writer.write_image_data(data).expect("data");
        writer.finish().expect("finish");
        out
    }

    #[test]
    fn icon_images_keep_their_colors_and_bad_ones_are_dropped() {
        let side = ICON_PX as usize;
        let mut pixels = vec![0u8; side * side * 4];
        pixels[4..8].copy_from_slice(&[31, 100, 200, 180]);
        let rgba = png_rgba(&encode(ICON_PX, png::ColorType::Rgba, &pixels)).expect("reads");
        assert_eq!(rgba, pixels);
        let gray = vec![77u8; side * side];
        let rgba = png_rgba(&encode(ICON_PX, png::ColorType::Grayscale, &gray)).expect("reads");
        assert_eq!(&rgba[..4], &[77, 77, 77, 255]);
        let small = encode(
            ICON_PX - 1,
            png::ColorType::Rgba,
            &pixels[..(side - 1) * (side - 1) * 4],
        );
        assert_eq!(png_rgba(&small), None);
        assert_eq!(png_rgba(&[1, 2, 3]), None);
        let icons = MenuIcons::default();
        let images = BTreeMap::from([
            (
                "a".to_string(),
                encode(ICON_PX, png::ColorType::Rgba, &pixels),
            ),
            ("b".to_string(), small),
        ]);
        assert_eq!(icons.insert(images), 1);
        assert_eq!(icons.snapshot().keys().collect::<Vec<_>>(), ["a"]);
        assert_eq!(icons.snapshot()["a"], pixels);
    }
}
