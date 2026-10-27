#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Stroke, Vec2};
use itool::workflow::{ExportOptions, ExportOutcome};
use itool::{ExportEntry, MediaItem};
#[path = "desktop/ui.rs"]
mod ui;
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Instant,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MediaFilter {
    All,
    Photos,
    Videos,
    LivePhotos,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SortOrder {
    NewestFirst,
    OldestFirst,
    LargestFirst,
    SmallestFirst,
    NameAscending,
    NameDescending,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GroupBy {
    Month,
    Week,
    Date,
    Continuous,
    MediaType,
    SourceFolder,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ExportLayout {
    Flat,
    Day,
    Month,
    MediaType,
    SourceFolder,
}

impl ExportLayout {
    fn label(self) -> &'static str {
        match self {
            Self::Flat => "One folder",
            Self::Day => "By day",
            Self::Month => "By month",
            Self::MediaType => "Photos / Videos / Live Photos",
            Self::SourceFolder => "Source folders",
        }
    }
    fn folder(self, item: &MediaItem) -> String {
        match self {
            Self::Flat => String::new(),
            Self::Day => item.date_str.clone(),
            Self::Month => item.month_key.clone(),
            Self::MediaType => media_label(item).into(),
            Self::SourceFolder => source_folder(item).into(),
        }
    }
}

fn media_label(item: &MediaItem) -> &'static str {
    match item.kind {
        "Video" => "Videos",
        "Photo + MOV" => "Live Photos",
        _ => "Photos",
    }
}
fn source_folder(item: &MediaItem) -> &str {
    item.path.rsplit('/').nth(1).unwrap_or("Unknown")
}
fn grouped_items<'a>(items: &[&'a MediaItem], mode: GroupBy) -> Vec<(String, Vec<&'a MediaItem>)> {
    // Keep the requested sort inside each group, and never split a group when sorting by size/name.
    let mut groups: Vec<(String, Vec<&MediaItem>)> = Vec::new();
    let mut positions = HashMap::new();
    for item in items {
        let key = match mode {
            GroupBy::Month => item.month_label.clone(),
            GroupBy::Week => item.week_label.clone(),
            GroupBy::Date => item.date_label.clone(),
            GroupBy::MediaType => media_label(item).into(),
            GroupBy::SourceFolder => source_folder(item).into(),
            GroupBy::Continuous => "All media".into(),
        };
        let index = *positions.entry(key.clone()).or_insert_with(|| {
            groups.push((key, Vec::new()));
            groups.len() - 1
        });
        groups[index].1.push(*item);
    }
    groups
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ThumbnailStatus {
    Loading,
    Loaded,
    Failed,
}

enum Job {
    Scan,
    Preview(String),
    CancelPreviews,
    Export(PathBuf, Vec<ExportEntry>, ExportOptions),
}

enum Event {
    Status(String),
    PreviewReset,
    Scanned(Box<itool::library::LibrarySnapshot>),
    Preview(String, Option<egui::ColorImage>),
    Progress(u64, u64, String),
    Finished(ExportOutcome),
    Error(String),
}

#[derive(Clone, Debug)]
struct MonthSummary {
    key: String,
    label: String,
    count: usize,
    bytes: u64,
}

const ACCENT: Color32 = Color32::from_rgb(59, 106, 250);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Connect,
    Library,
    Export,
    Results,
}
#[derive(Clone, Copy)]
struct Palette {
    text: Color32,
    muted: Color32,
    panel: Color32,
    sidebar: Color32,
    canvas: Color32,
    border: Color32,
    accent: Color32,
    tint: Color32,
    danger: Color32,
    warning: Color32,
}
impl Palette {
    fn new(dark: bool) -> Self {
        if dark {
            Self {
                text: Color32::from_rgb(240, 242, 247),
                muted: Color32::from_rgb(171, 181, 199),
                panel: Color32::from_rgb(28, 33, 42),
                sidebar: Color32::from_rgb(30, 36, 45),
                canvas: Color32::from_rgb(24, 30, 37),
                border: Color32::from_rgb(53, 62, 78),
                accent: Color32::from_rgb(130, 158, 255),
                tint: Color32::from_rgb(38, 49, 80),
                danger: Color32::from_rgb(255, 157, 148),
                warning: Color32::from_rgb(240, 193, 122),
            }
        } else {
            Self {
                text: Color32::from_rgb(25, 29, 39),
                muted: Color32::from_rgb(89, 99, 119),
                panel: Color32::WHITE,
                sidebar: Color32::from_rgb(252, 252, 254),
                canvas: Color32::from_rgb(250, 251, 253),
                border: Color32::from_rgb(221, 226, 236),
                accent: ACCENT,
                tint: Color32::from_rgb(232, 237, 255),
                danger: Color32::from_rgb(158, 47, 39),
                warning: Color32::from_rgb(130, 70, 15),
            }
        }
    }
}
fn secondary(label: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(RichText::new(label.into()).size(13.0))
        .corner_radius(5)
        .min_size(Vec2::new(0.0, 36.0))
}
fn toggle(label: impl Into<String>, active: bool, dark: bool) -> egui::Button<'static> {
    let p = Palette::new(dark);
    egui::Button::new(RichText::new(label.into()).size(13.0).color(if active {
        p.accent
    } else {
        p.muted
    }))
    .fill(if active { p.tint } else { p.panel })
    .stroke(Stroke::new(
        if active { 1.5 } else { 1.0 },
        if active { p.accent } else { p.border },
    ))
    .corner_radius(5)
    .min_size(Vec2::new(0.0, 36.0))
    .selected(active)
}
fn theme_file() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("ITool/preferences.json"))
}
fn read_dark_preference() -> bool {
    theme_file()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|v| v["dark_mode"].as_bool())
        .unwrap_or(false)
}
fn save_dark_preference(dark: bool) -> std::io::Result<()> {
    if let Some(path) = theme_file() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            path,
            if dark {
                b"{\"dark_mode\":true}".as_slice()
            } else {
                b"{\"dark_mode\":false}".as_slice()
            },
        )?;
    }
    Ok(())
}
fn apply_theme(ctx: &egui::Context, dark: bool) {
    let theme = if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    let p = Palette::new(dark);
    ctx.set_theme(theme);
    let mut style = (*ctx.style_of(theme)).clone();
    style.spacing.item_spacing = Vec2::new(12.0, 8.0);
    style.spacing.button_padding = Vec2::new(14.0, 9.0);
    style.spacing.interact_size.y = 36.0;
    style.visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    style.visuals.override_text_color = Some(p.text);
    style.visuals.weak_text_color = Some(p.muted);
    style.visuals.text_edit_bg_color = Some(p.panel);
    style.visuals.panel_fill = p.panel;
    style.visuals.window_fill = p.panel;
    style.visuals.window_stroke = Stroke::new(1.0, p.border);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    style.visuals.selection.bg_fill = p.tint;
    style.visuals.selection.stroke = Stroke::new(1.5, p.accent);
    style.visuals.widgets.inactive.bg_fill = p.border;
    style.visuals.widgets.inactive.weak_bg_fill = p.panel;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border);
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    style.visuals.widgets.hovered.bg_fill = p.tint;
    style.visuals.widgets.hovered.weak_bg_fill = p.tint;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.5, p.accent);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, p.accent);
    style.visuals.widgets.active.bg_fill = p.accent;
    style.visuals.widgets.active.weak_bg_fill = p.tint;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.5, p.accent);
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.5, p.text);
    style.visuals.window_corner_radius = CornerRadius::same(8);
    style.visuals.widgets.inactive.corner_radius = CornerRadius::same(5);
    style.visuals.widgets.hovered.corner_radius = CornerRadius::same(5);
    style.visuals.widgets.active.corner_radius = CornerRadius::same(5);
    for (kind, size) in [
        (egui::TextStyle::Body, 14.0),
        (egui::TextStyle::Button, 14.0),
        (egui::TextStyle::Small, 12.0),
        (egui::TextStyle::Heading, 19.0),
    ] {
        style
            .text_styles
            .insert(kind, egui::FontId::proportional(size));
    }
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(21.0, egui::FontFamily::Name("UI Semibold".into())),
    );
    ctx.set_style_of(theme, style);
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn safe_truncate(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

struct Desktop {
    logo: egui::TextureHandle,
    illustration: egui::TextureHandle,
    illustration_dark: egui::TextureHandle,
    screen: Screen,
    settings_open: bool,
    connection_help: bool,
    folder_name: String,
    options: ExportOptions,
    outcome: Option<ExportOutcome>,
    metadata: BTreeMap<String, itool::library::AssetMetadata>,
    metadata_status: String,
    albums_available: bool,
    favorites_available: bool,
    album_filter: Option<String>,
    favorites_only: bool,
    jobs: mpsc::Sender<Job>,
    events: mpsc::Receiver<Event>,
    cancel: Arc<AtomicBool>,
    items: Vec<MediaItem>,
    selected: HashSet<String>,
    textures: HashMap<String, egui::TextureHandle>,
    thumbnail_status: HashMap<String, ThumbnailStatus>,
    texture_lru: VecDeque<String>,
    drag_active: bool,
    drag_base_selected: HashSet<String>,

    // Filters and controls
    search: String,
    media_filter: MediaFilter,
    sort_order: SortOrder,
    group_by: GroupBy,
    selected_month: Option<String>,
    show_sidebar: bool,
    selected_only: bool,
    tile_size: f32,
    export_layout: ExportLayout,
    export_folder: Option<PathBuf>,
    selection_anchor: Option<String>,
    demo: bool,
    list_view: bool,
    dark_mode: bool,
    capture_frames: u32,

    // Month index
    months: Vec<MonthSummary>,

    // App state
    destination: String,
    status: String,
    current_file_progress: String,
    error: Option<String>,
    busy: bool,
    scanning: bool,
    done: u64,
    total: u64,
    started: Option<Instant>,
    report: Option<PathBuf>,
}

impl Desktop {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (jobs, rx) = mpsc::channel();
        let (tx, events) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_worker = cancel.clone();
        let ctx = cc.egui_ctx.clone();

        std::thread::spawn(move || {
            let runtime = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = tx.send(Event::Error(format!("Cannot initialize runtime: {e}")));
                    return;
                }
            };

            let mut preview_client = None;
            let mut pending_previews: VecDeque<String> = VecDeque::new();

            loop {
                // If there are no pending preview requests, wait for incoming jobs
                if pending_previews.is_empty() {
                    match rx.recv() {
                        Ok(Job::Scan) => {
                            pending_previews.clear();
                            let _ = tx.send(Event::PreviewReset);
                            preview_client = None;
                            let result = runtime.block_on(itool::library::scan(|s| {
                                let _ = tx.send(Event::Status(s));
                                ctx.request_repaint();
                            }));
                            let _ = tx.send(match result {
                                Ok(items) => Event::Scanned(Box::new(items)),
                                Err(e) => Event::Error(format!("{e:#}")),
                            });
                            ctx.request_repaint();
                            continue;
                        }
                        Ok(Job::Export(dest, paths, options)) => {
                            pending_previews.clear();
                            let _ = tx.send(Event::PreviewReset);
                            preview_client = None;
                            let result = runtime.block_on(itool::workflow::export(
                                &dest,
                                paths,
                                options,
                                &cancel_worker,
                                |done, total, name| {
                                    let _ = tx.send(Event::Progress(done, total, name));
                                    ctx.request_repaint();
                                },
                            ));
                            let _ = tx.send(match result {
                                Ok(report) => Event::Finished(report),
                                Err(e) => Event::Error(format!("{e:#}")),
                            });
                            ctx.request_repaint();
                            continue;
                        }
                        Ok(Job::Preview(path)) => {
                            pending_previews.push_back(path);
                        }
                        Ok(Job::CancelPreviews) => {
                            pending_previews.clear();
                            let _ = tx.send(Event::PreviewReset);
                            continue;
                        }
                        Err(_) => break,
                    }
                }

                // Drain any additional messages currently waiting in the channel
                while let Ok(job) = rx.try_recv() {
                    match job {
                        Job::Scan => {
                            pending_previews.clear();
                            let _ = tx.send(Event::PreviewReset);
                            preview_client = None;
                            let result = runtime.block_on(itool::library::scan(|s| {
                                let _ = tx.send(Event::Status(s));
                                ctx.request_repaint();
                            }));
                            let _ = tx.send(match result {
                                Ok(items) => Event::Scanned(Box::new(items)),
                                Err(e) => Event::Error(format!("{e:#}")),
                            });
                            ctx.request_repaint();
                            break;
                        }
                        Job::Export(dest, paths, options) => {
                            pending_previews.clear();
                            let _ = tx.send(Event::PreviewReset);
                            preview_client = None;
                            let result = runtime.block_on(itool::workflow::export(
                                &dest,
                                paths,
                                options,
                                &cancel_worker,
                                |done, total, name| {
                                    let _ = tx.send(Event::Progress(done, total, name));
                                    ctx.request_repaint();
                                },
                            ));
                            let _ = tx.send(match result {
                                Ok(report) => Event::Finished(report),
                                Err(e) => Event::Error(format!("{e:#}")),
                            });
                            ctx.request_repaint();
                            break;
                        }
                        Job::Preview(p) => {
                            if !pending_previews.contains(&p) {
                                pending_previews.push_back(p);
                                // Bound pending queue to prevent memory growth during rapid scrolling
                                if pending_previews.len() > 80
                                    && let Some(path) = pending_previews.pop_front()
                                {
                                    let _ = tx.send(Event::Preview(path, None));
                                }
                            }
                        }
                        Job::CancelPreviews => {
                            pending_previews.clear();
                            let _ = tx.send(Event::PreviewReset);
                        }
                    }
                }

                // Pop from the back of the queue (LIFO): prioritize thumbnails visible in the current viewport!
                let path = match pending_previews.pop_back() {
                    Some(p) => p,
                    None => continue,
                };

                let result = runtime.block_on(async {
                    if preview_client.is_none() {
                        match itool::afc().await {
                            Ok(c) => preview_client = Some(c),
                            Err(e) => return Err(e),
                        }
                    }
                    itool::read_thumbnail(preview_client.as_mut().unwrap(), &path).await
                });

                // Only invalidate preview_client if transport/socket failed
                if let Err(ref e) = result {
                    let err_str = format!("{e:#}").to_lowercase();
                    if err_str.contains("closed")
                        || err_str.contains("connection reset")
                        || err_str.contains("broken pipe")
                        || err_str.contains("timed out")
                    {
                        preview_client = None;
                    }
                }

                let image = result
                    .ok()
                    .and_then(|bytes| image::load_from_memory(&bytes).ok())
                    .map(|image| {
                        // Maintain true image aspect ratio inside 320x320 bounding box
                        let rgba = image.thumbnail(320, 320).to_rgba8();
                        egui::ColorImage::from_rgba_unmultiplied(
                            [rgba.width() as usize, rgba.height() as usize],
                            rgba.as_raw(),
                        )
                    });

                let _ = tx.send(Event::Preview(path, image));
                ctx.request_repaint();
            }
        });

        let mut fonts = egui::FontDefinitions::default();
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        for (name, file) in [
            ("Segoe UI", "segoeui.ttf"),
            ("Segoe Semibold", "seguisb.ttf"),
        ] {
            if let Ok(bytes) = std::fs::read(PathBuf::from(&windir).join("Fonts").join(file)) {
                fonts
                    .font_data
                    .insert(name.into(), Arc::new(egui::FontData::from_owned(bytes)));
            }
        }
        if fonts.font_data.contains_key("Segoe UI") {
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "Segoe UI".into());
        }
        let semibold = if fonts.font_data.contains_key("Segoe Semibold") {
            vec!["Segoe Semibold".into()]
        } else {
            fonts.families[&egui::FontFamily::Proportional].clone()
        };
        fonts
            .families
            .insert(egui::FontFamily::Name("UI Semibold".into()), semibold);
        cc.egui_ctx.set_fonts(fonts);
        let dark_mode = if std::env::args().any(|a| a == "--demo-dark") {
            true
        } else if std::env::args().any(|a| a == "--demo-light") {
            false
        } else {
            read_dark_preference()
        };
        apply_theme(&cc.egui_ctx, dark_mode);

        let illustration =
            image::load_from_memory(include_bytes!("../../assets/connect-illustration.png"))
                .expect("Bundled illustration")
                .to_rgba8();
        let mut dark_art = illustration.clone();
        for pixel in dark_art.pixels_mut() {
            if pixel[0] > 50 {
                pixel[0] = 205;
                pixel[1] = 217;
                pixel[2] = 236;
            }
        }
        let illustration_dark = cc.egui_ctx.load_texture(
            "connect-dark",
            egui::ColorImage::from_rgba_unmultiplied(
                [dark_art.width() as usize, dark_art.height() as usize],
                dark_art.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        );
        let illustration = cc.egui_ctx.load_texture(
            "connect-illustration",
            egui::ColorImage::from_rgba_unmultiplied(
                [
                    illustration.width() as usize,
                    illustration.height() as usize,
                ],
                illustration.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        );
        let approved_logo = image::load_from_memory(include_bytes!(
            "../../design/mockups/itool-logo-concept.png"
        ))
        .expect("Approved logo")
        .crop_imm(200, 170, 910, 870)
        .to_rgba8();
        let logo = cc.egui_ctx.load_texture(
            "approved-logo",
            egui::ColorImage::from_rgba_unmultiplied(
                [
                    approved_logo.width() as usize,
                    approved_logo.height() as usize,
                ],
                approved_logo.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        );
        let mut app = Self {
            logo,
            illustration,
            illustration_dark,
            screen: Screen::Connect,
            settings_open: false,
            connection_help: false,
            folder_name: chrono::Local::now()
                .format("iPhone photos %Y-%m-%d")
                .to_string(),
            options: ExportOptions::default(),
            outcome: None,
            metadata: BTreeMap::new(),
            metadata_status: String::new(),
            albums_available: false,
            favorites_available: false,
            album_filter: None,
            favorites_only: false,
            jobs,
            events,
            cancel,
            items: vec![],
            selected: HashSet::new(),
            textures: HashMap::new(),
            thumbnail_status: HashMap::new(),
            texture_lru: VecDeque::new(),
            drag_active: false,
            drag_base_selected: HashSet::new(),

            search: String::new(),
            media_filter: MediaFilter::All,
            sort_order: SortOrder::NewestFirst,
            group_by: GroupBy::Continuous,
            selected_month: None,
            show_sidebar: true,
            selected_only: false,
            tile_size: 156.0,
            export_layout: ExportLayout::Month,
            export_folder: None,
            selection_anchor: None,
            demo: std::env::args().any(|a| a == "--demo"),
            list_view: false,
            dark_mode,
            capture_frames: 0,
            months: vec![],

            destination: std::env::var("USERPROFILE")
                .map(|p| format!("{p}\\Pictures"))
                .unwrap_or_default(),
            status: "Connect your iPhone via USB, unlock it, then click 'Scan iPhone'.".into(),
            current_file_progress: String::new(),
            error: None,
            busy: false,
            scanning: false,
            done: 0,
            total: 0,
            started: None,
            report: None,
        };
        if app.demo {
            app.load_demo(&cc.egui_ctx);
        }
        app
    }

    fn load_demo(&mut self, ctx: &egui::Context) {
        // Explicitly synthetic, device-free fixture for UI checks. Never export these paths.
        self.status =
            "DEMO · Sample media only. Device scanning and transfers are disabled.".into();
        let atlas = image::load_from_memory(include_bytes!("../../assets/demo-atlas.png"))
            .expect("Bundled synthetic sample photographs");
        let names = [
            "IMG_1023.JPG",
            "IMG_1042.JPG",
            "IMG_0871.JPG",
            "IMG_1201.MOV",
            "IMG_0996.JPG",
            "IMG_0764.JPG",
            "IMG_1188.JPG",
            "IMG_0921.JPG",
        ];
        let days = [
            "2026-10-14",
            "2026-10-14",
            "2026-10-13",
            "2026-10-12",
            "2026-09-28",
            "2026-09-26",
            "2026-09-22",
            "2026-09-20",
        ];
        for index in 0..8 {
            let day = days[index];
            let kind = if index == 3 {
                "Video"
            } else if index == 6 {
                "Photo + MOV"
            } else {
                "Photo"
            };
            let name = names[index].to_owned();
            let path = format!("/DCIM/100APPLE/{name}");
            let mut resources = vec![path.clone()];
            if kind == "Photo + MOV" {
                resources.push(path.replace(".JPG", ".MOV"));
            }
            let cell = atlas
                .crop_imm(
                    (index as u32 % 4) * atlas.width() / 4,
                    (index as u32 / 4) * atlas.height() / 2,
                    atlas.width() / 4,
                    atlas.height() / 2,
                )
                .to_rgba8();
            self.add_texture(
                ctx,
                path.clone(),
                egui::ColorImage::from_rgba_unmultiplied(
                    [cell.width() as usize, cell.height() as usize],
                    cell.as_raw(),
                ),
            );
            if [0, 2, 5, 6].contains(&index) {
                self.selected.insert(path.clone());
            }
            let date = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").expect("Sample date");
            self.items.push(MediaItem {
                path,
                name,
                resources,
                kind,
                size: if index == 6 {
                    430_790_272
                } else if index == 3 {
                    72_000_000
                } else {
                    6_000_000
                },
                timestamp: date
                    .and_hms_opt(if index == 1 { 3 } else { 12 }, 0, 0)
                    .expect("Sample time")
                    .and_utc()
                    .timestamp(),
                date_str: day.into(),
                month_key: date.format("%Y-%m").to_string(),
                month_label: date.format("%B %Y").to_string(),
                week_label: date.format("Week %U, %Y").to_string(),
                date_label: date.format("%b %e, %Y").to_string(),
            });
        }
        self.screen = Screen::Library;
        self.group_by = GroupBy::Continuous;
        self.folder_name = "October memories".into();
        self.update_months_index();
        if std::env::args().any(|a| a == "--demo-review") {
            self.screen = Screen::Export;
            self.export_folder = Some(PathBuf::from(&self.destination).join("ITool-demo"));
        }
        if std::env::args().any(|a| a == "--demo-connect") {
            self.screen = Screen::Connect;
        }
        if std::env::args().any(|a| a == "--demo-results") {
            self.screen = Screen::Results;
            self.outcome = Some(ExportOutcome {
                folder: PathBuf::from(&self.destination).join("Sample export"),
                copied: 6,
                skipped: 2,
                converted: 1,
                copied_bytes: 428_000_000,
                elapsed_seconds: 24.0,
                warnings: vec![],
            });
            self.status = "Sample export complete · no files were copied".into();
        }
        self.list_view = std::env::args().any(|a| a == "--demo-list");
    }

    fn scan(&mut self) {
        if self.demo {
            return;
        }
        self.screen = Screen::Connect;
        self.metadata.clear();
        self.metadata_status.clear();
        self.albums_available = false;
        self.favorites_available = false;
        self.album_filter = None;
        self.favorites_only = false;
        self.items.clear();
        self.selected_only = false;
        self.selection_anchor = None;
        self.months.clear();
        self.selected_month = None;
        self.busy = true;
        self.scanning = true;
        self.error = None;
        self.report = None;
        self.status = "Connecting to iPhone…".into();
        self.textures.clear();
        self.thumbnail_status.clear();
        self.texture_lru.clear();
        self.selected.clear();
        self.drag_active = false;
        self.drag_base_selected.clear();
        let _ = self.jobs.send(Job::Scan);
    }

    fn update_months_index(&mut self) {
        let mut map: BTreeMap<String, (String, usize, u64)> = BTreeMap::new();
        for item in &self.items {
            let entry = map
                .entry(item.month_key.clone())
                .or_insert_with(|| (item.month_label.clone(), 0, 0));
            entry.1 += 1;
            entry.2 += item.size;
        }

        let mut list: Vec<MonthSummary> = map
            .into_iter()
            .map(|(key, (label, count, bytes))| MonthSummary {
                key,
                label,
                count,
                bytes,
            })
            .collect();
        list.sort_by(|a, b| b.key.cmp(&a.key));
        self.months = list;
    }

    fn add_texture(&mut self, ctx: &egui::Context, path: String, image: egui::ColorImage) {
        let texture = ctx.load_texture(&path, image, egui::TextureOptions::LINEAR);
        self.textures.insert(path.clone(), texture);
        self.thumbnail_status
            .insert(path.clone(), ThumbnailStatus::Loaded);
        self.texture_lru.push_back(path);

        // Bounded memory cache: keep up to 600 textures in memory
        while self.texture_lru.len() > 600 {
            if let Some(old_path) = self.texture_lru.pop_front() {
                self.textures.remove(&old_path);
                self.thumbnail_status.remove(&old_path);
            }
        }
    }

    fn request_preview_if_needed(&mut self, path: &str) {
        if !self.demo && !self.busy && !self.thumbnail_status.contains_key(path) {
            self.thumbnail_status
                .insert(path.to_owned(), ThumbnailStatus::Loading);
            let _ = self.jobs.send(Job::Preview(path.to_owned()));
        }
    }

    fn get_filtered_items<'a>(
        items: &'a [MediaItem],
        search: &str,
        media_filter: MediaFilter,
        sort_order: SortOrder,
        selected_month: Option<&str>,
    ) -> Vec<&'a MediaItem> {
        let needle = search.trim().to_lowercase();
        let mut list: Vec<&'a MediaItem> = items
            .iter()
            .filter(|item| {
                // Month filter from Month Overview sidebar
                if let Some(m) = selected_month {
                    if item.month_label != m {
                        return false;
                    }
                }
                // Media Type filter
                match media_filter {
                    MediaFilter::All => {}
                    MediaFilter::Photos => {
                        if item.kind == "Video" {
                            return false;
                        }
                    }
                    MediaFilter::Videos => {
                        if item.kind != "Video" {
                            return false;
                        }
                    }
                    MediaFilter::LivePhotos => {
                        if item.kind != "Photo + MOV" {
                            return false;
                        }
                    }
                }
                // Text search
                if !needle.is_empty() {
                    let matches_name = item.name.to_lowercase().contains(&needle);
                    let matches_path = item.path.to_lowercase().contains(&needle);
                    let matches_date = item.date_str.contains(&needle)
                        || item.month_label.to_lowercase().contains(&needle)
                        || item.date_label.to_lowercase().contains(&needle);
                    if !matches_name && !matches_path && !matches_date {
                        return false;
                    }
                }
                true
            })
            .collect();

        // Sort order
        match sort_order {
            SortOrder::NewestFirst => {
                list.sort_by(|a, b| {
                    b.timestamp
                        .cmp(&a.timestamp)
                        .then_with(|| b.path.cmp(&a.path))
                });
            }
            SortOrder::OldestFirst => {
                list.sort_by(|a, b| {
                    a.timestamp
                        .cmp(&b.timestamp)
                        .then_with(|| a.path.cmp(&b.path))
                });
            }
            SortOrder::LargestFirst => {
                list.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)))
            }
            SortOrder::SmallestFirst => {
                list.sort_by(|a, b| a.size.cmp(&b.size).then_with(|| a.path.cmp(&b.path)))
            }
            SortOrder::NameAscending => {
                list.sort_by_cached_key(|a| (a.name.to_lowercase(), a.path.clone()))
            }
            SortOrder::NameDescending => list
                .sort_by_cached_key(|a| std::cmp::Reverse((a.name.to_lowercase(), a.path.clone()))),
        }
        list
    }

    fn filtered_items(&self) -> Vec<&MediaItem> {
        let mut items = Self::get_filtered_items(
            &self.items,
            &self.search,
            self.media_filter,
            self.sort_order,
            self.selected_month.as_deref(),
        );
        if self.selected_only {
            items.retain(|i| self.selected.contains(&i.path));
        }
        items.retain(|i| self.matches_collection(i));
        items
    }

    fn matches_collection(&self, item: &MediaItem) -> bool {
        let metadata = self.metadata.get(&item.path);
        (!self.favorites_only || metadata.is_some_and(|m| m.favorite == Some(true)))
            && self
                .album_filter
                .as_ref()
                .is_none_or(|album| metadata.is_some_and(|m| m.albums.contains(album)))
    }

    fn export_plan(&self) -> Vec<ExportEntry> {
        let sorted =
            Self::get_filtered_items(&self.items, "", MediaFilter::All, self.sort_order, None);
        grouped_items(&sorted, self.group_by)
            .into_iter()
            .flat_map(|(_, items)| items)
            .filter(|i| self.selected.contains(&i.path))
            .flat_map(|i| {
                i.resources.iter().map(move |source| ExportEntry {
                    source: source.clone(),
                    folder: self.export_layout.folder(i),
                })
            })
            .collect()
    }

    fn begin_export(&mut self) {
        let entries = self.export_plan();
        if let Err(e) = itool::validate_export_plan(&entries) {
            self.error = Some(e.to_string());
            return;
        }
        let dest = match itool::workflow::named_destination(&self.destination, &self.folder_name) {
            Ok(path) => path,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        if self.demo {
            return;
        }
        self.export_folder = Some(dest.clone());
        self.cancel.store(false, Ordering::Relaxed);
        self.busy = true;
        self.error = None;
        self.report = None;
        self.outcome = None;
        self.done = 0;
        self.total = 0;
        self.current_file_progress.clear();
        self.started = Some(Instant::now());
        self.screen = Screen::Results;
        self.status = "Preparing original files…".into();
        if self
            .jobs
            .send(Job::Export(dest, entries, self.options.clone()))
            .is_err()
        {
            self.busy = false;
            self.error = Some("Transfer worker stopped. Restart ITool and retry.".into());
        }
    }
}

impl eframe::App for Desktop {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.guided_ui(ui);
    }
}

impl Desktop {
    #[allow(clippy::too_many_arguments)]
    fn render_media_card(
        ui: &mut egui::Ui,
        item: &MediaItem,
        index: usize,
        card_width: f32,
        card_height: f32,
        is_selected: bool,
        texture: Option<&egui::TextureHandle>,
        thumb_status: ThumbnailStatus,
        active_marquee: Option<egui::Rect>,
        intersected_in_drag: &mut HashSet<String>,
        card_click: &mut Option<(usize, bool)>,
        retry_preview: &mut Option<String>,
        is_busy: bool,
        is_drag_active: bool,
    ) {
        let p = Palette::new(ui.visuals().dark_mode);
        let tile_size = Vec2::new(card_width, card_height);
        let (tile_rect, response) =
            ui.allocate_exact_size(tile_size, egui::Sense::click_and_drag());
        let rect = egui::Rect::from_min_max(tile_rect.min, tile_rect.max - Vec2::new(0.0, 40.0));
        let is_hovered = response.hovered();
        let is_clicked = response.clicked();

        let is_in_marquee = if let Some(marquee) = active_marquee {
            rect.intersects(marquee)
        } else {
            false
        };
        if is_in_marquee {
            intersected_in_drag.insert(item.path.clone());
        }
        let is_selected = is_selected || is_in_marquee;

        // Handle click event with Shift modifier detection (suppressed during dragging)
        if is_clicked && !is_busy && !is_drag_active {
            let is_shift = ui.input(|i| i.modifiers.shift);
            *card_click = Some((index, is_shift));
        }

        // Full metadata tooltip on hover
        response.on_hover_ui(|ui| {
            ui.set_max_width(280.0);
            ui.label(RichText::new(&item.name).strong().size(13.0).color(p.text));
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Type:").color(p.muted).size(11.5));
                ui.label(RichText::new(item.kind).color(p.text).size(11.5));
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Size:").color(p.muted).size(11.5));
                ui.label(
                    RichText::new(format_size(item.size))
                        .color(p.text)
                        .size(11.5),
                );
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Date:").color(p.muted).size(11.5));
                ui.label(RichText::new(&item.date_label).color(p.text).size(11.5));
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Path:").color(p.muted).size(11.0));
                ui.label(RichText::new(&item.path).color(p.muted).size(11.0));
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("Click to select · Shift+Click for range · Drag to select")
                    .size(10.5)
                    .color(p.muted),
            );
        });

        // 1. Card Base Background & Stroke
        let bg_color = if is_selected {
            Color32::from_rgba_premultiplied(66, 91, 220, 22)
        } else {
            p.panel
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), bg_color);

        // 2. Render Image (preserving aspect ratio) or Placeholder
        match thumb_status {
            ThumbnailStatus::Loaded => {
                if let Some(tex) = texture {
                    let tex_w = tex.size()[0] as f32;
                    let tex_h = tex.size()[1] as f32;
                    let aspect = (tex_w / tex_h.max(1.0)).clamp(0.25, 4.0);

                    // Cover-crop only the gallery preview; exported original bytes stay unchanged.
                    let tile_aspect = rect.width() / rect.height().max(1.);
                    let uv = if aspect > tile_aspect {
                        let width = tile_aspect / aspect;
                        egui::Rect::from_min_max(
                            egui::pos2((1. - width) * 0.5, 0.),
                            egui::pos2((1. + width) * 0.5, 1.),
                        )
                    } else {
                        let height = aspect / tile_aspect;
                        egui::Rect::from_min_max(
                            egui::pos2(0., (1. - height) * 0.5),
                            egui::pos2(1., (1. + height) * 0.5),
                        )
                    };
                    ui.painter().image(tex.id(), rect, uv, Color32::WHITE);
                }
            }
            ThumbnailStatus::Loading => {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(6), p.canvas);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Loading…",
                    egui::FontId::proportional(12.0),
                    p.muted,
                );
            }
            ThumbnailStatus::Failed => {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(6), p.sidebar);
                let (icon, label) = if item.kind == "Video" {
                    ("MOV", "Video preview unavailable")
                } else if item.kind == "Photo + MOV" {
                    ("LIVE", "Live Photo preview unavailable")
                } else {
                    ("PHOTO", "Photo preview unavailable")
                };

                ui.painter().text(
                    rect.center() + Vec2::new(0.0, -10.0),
                    egui::Align2::CENTER_CENTER,
                    icon,
                    egui::FontId::proportional(22.0),
                    p.muted,
                );

                let label_text = if is_hovered { "Click to retry" } else { label };
                ui.painter().text(
                    rect.center() + Vec2::new(0.0, 14.0),
                    egui::Align2::CENTER_CENTER,
                    label_text,
                    egui::FontId::proportional(11.0),
                    if is_hovered { p.accent } else { p.muted },
                );

                if is_clicked && is_hovered && !is_drag_active {
                    *retry_preview = Some(item.path.clone());
                }
            }
        }

        // 3. Selection border / Hover stroke
        let stroke = if is_selected {
            Stroke::new(2.5, p.accent)
        } else if is_hovered {
            Stroke::new(1.5, p.muted)
        } else {
            Stroke::new(1.0, p.border)
        };
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(6),
            stroke,
            egui::StrokeKind::Inside,
        );

        // 4. Media type badge in top-left
        if item.kind == "Video" || item.kind == "Photo + MOV" {
            let badge_text = if item.kind == "Video" {
                "VIDEO"
            } else {
                "LIVE"
            };
            let badge_rect = egui::Rect::from_min_size(
                rect.max - Vec2::new(if item.kind == "Video" { 66.0 } else { 62.0 }, 30.0),
                Vec2::new(if item.kind == "Video" { 46.0 } else { 42.0 }, 18.0),
            );
            ui.painter().rect_filled(
                badge_rect,
                CornerRadius::same(4),
                Color32::from_rgba_premultiplied(0, 0, 0, 160),
            );
            ui.painter().text(
                badge_rect.center(),
                egui::Align2::CENTER_CENTER,
                badge_text,
                egui::FontId::proportional(9.0),
                Color32::WHITE,
            );
        }

        // Consistent square selection control, separate from the media-type badge.
        let check = egui::Rect::from_min_size(rect.min + Vec2::new(12.0, 12.0), Vec2::splat(21.0));
        ui.painter().rect_filled(
            check,
            4,
            if is_selected {
                ACCENT
            } else {
                Color32::from_black_alpha(36)
            },
        );
        ui.painter().rect_stroke(
            check,
            4,
            Stroke::new(1.3, Color32::WHITE),
            egui::StrokeKind::Inside,
        );
        if is_selected {
            let c = check.center();
            ui.painter().line_segment(
                [c + Vec2::new(-5., 0.), c + Vec2::new(-1., 4.)],
                Stroke::new(1.8, Color32::WHITE),
            );
            ui.painter().line_segment(
                [c + Vec2::new(-1., 4.), c + Vec2::new(6., -5.)],
                Stroke::new(1.8, Color32::WHITE),
            );
        }

        // Labels belong below the preview so originals remain unobscured.
        let text_top = egui::pos2(tile_rect.min.x, rect.max.y + 7.0);
        let characters = ((tile_rect.width() - 8.0) / 6.5).max(12.0) as usize;
        ui.painter().text(
            text_top,
            egui::Align2::LEFT_TOP,
            safe_truncate(&item.name, characters),
            egui::FontId::proportional(12.0),
            p.text,
        );
        ui.painter().text(
            text_top + Vec2::new(0.0, 20.0),
            egui::Align2::LEFT_TOP,
            chrono::DateTime::from_timestamp(item.timestamp, 0)
                .map(|date| date.format("%b %e, %Y  %l:%M %p").to_string())
                .unwrap_or_else(|| item.date_str.clone()),
            egui::FontId::proportional(11.0),
            p.muted,
        );
    }
}

fn main() -> eframe::Result {
    let icon = image::load_from_memory(include_bytes!("../../assets/itool.png"))
        .expect("Bundled app icon must decode")
        .to_rgba8();
    let icon = egui::IconData {
        width: icon.width(),
        height: icon.height(),
        rgba: icon.into_raw(),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(if std::env::var_os("ITOOL_UI_COMPACT").is_some() {
                [900.0, 600.0]
            } else {
                [940.0, 664.0]
            })
            .with_min_inner_size([900.0, 600.0])
            .with_decorations(false)
            .with_icon(Arc::new(icon))
            .with_title("ITool — iPhone originals"),
        ..Default::default()
    };
    eframe::run_native(
        "ITool — iPhone originals",
        options,
        Box::new(|cc| Ok(Box::new(Desktop::new(cc)))),
    )
}

#[cfg(test)]
mod desktop_tests {
    use super::*;

    #[test]
    fn size_name_orders_are_deterministic() {
        let mut items = vec![
            sample_item("B.MOV", "Video", 100, "October 2026"),
            sample_item("A.JPG", "Photo", 200, "October 2026"),
        ];
        items[0].size = 500;
        items[1].size = 50;
        for (order, expected) in [
            (SortOrder::LargestFirst, "B.MOV"),
            (SortOrder::SmallestFirst, "A.JPG"),
            (SortOrder::NameAscending, "A.JPG"),
            (SortOrder::NameDescending, "B.MOV"),
        ] {
            assert_eq!(
                Desktop::get_filtered_items(&items, "", MediaFilter::All, order, None)[0].name,
                expected
            );
        }
    }

    #[test]
    fn groups_do_not_split_when_sort_interleaves_dates() {
        let mut items = vec![
            sample_item("A.JPG", "Photo", 300, "October 2026"),
            sample_item("B.MOV", "Video", 200, "September 2026"),
            sample_item("C.JPG", "Photo", 100, "October 2026"),
        ];
        items[0].size = 300;
        items[1].size = 200;
        items[2].size = 100;
        let sorted = Desktop::get_filtered_items(
            &items,
            "",
            MediaFilter::All,
            SortOrder::LargestFirst,
            None,
        );
        let groups = grouped_items(&sorted, GroupBy::Month);
        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups[0]
                .1
                .iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>(),
            vec!["A.JPG", "C.JPG"]
        );
        assert_eq!(grouped_items(&sorted, GroupBy::MediaType).len(), 2);
        assert_eq!(grouped_items(&sorted, GroupBy::SourceFolder).len(), 1);
        let in_month = Desktop::get_filtered_items(
            &items,
            "",
            MediaFilter::All,
            SortOrder::LargestFirst,
            Some("October 2026"),
        );
        assert_eq!(grouped_items(&in_month, GroupBy::MediaType)[0].1.len(), 2);
    }

    #[test]
    fn live_photo_resources_share_export_folder() {
        let mut item = sample_item("A.HEIC", "Photo + MOV", 100, "October 2026");
        item.resources.push("/DCIM/100APPLE/A.MOV".into());
        for layout in [
            ExportLayout::Flat,
            ExportLayout::Day,
            ExportLayout::Month,
            ExportLayout::MediaType,
            ExportLayout::SourceFolder,
        ] {
            let plan: Vec<_> = item
                .resources
                .iter()
                .map(|p| ExportEntry {
                    source: p.clone(),
                    folder: layout.folder(&item),
                })
                .collect();
            assert_eq!(plan.len(), 2);
            assert_eq!(plan[0].folder, plan[1].folder);
            assert!(itool::validate_export_plan(&plan).is_ok());
        }
        assert_eq!(ExportLayout::Day.folder(&item), "2026-10-23");
        assert_eq!(ExportLayout::MediaType.folder(&item), "Live Photos");
        assert_eq!(ExportLayout::SourceFolder.folder(&item), "100APPLE");
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
        assert_eq!(format_size(15728640), "15.0 MB");
        assert_eq!(format_size(1073741824), "1.00 GB");
    }

    #[test]
    fn test_safe_truncate_utf8() {
        assert_eq!(safe_truncate("simple.jpg", 15), "simple.jpg");
        assert_eq!(
            safe_truncate("very_long_photo_name_2026.heic", 12),
            "very_long_p…"
        );
        // Multi-byte UTF-8 testing (e.g. Japanese, emoji)
        let utf8_name = "写真_2026_東京_旅行.heic";
        let truncated = safe_truncate(utf8_name, 8);
        assert!(truncated.ends_with('…'));
        assert_eq!(truncated.chars().count(), 8);
    }

    fn sample_item(name: &str, kind: &'static str, timestamp: i64, month: &str) -> MediaItem {
        MediaItem {
            path: format!("/DCIM/100APPLE/{name}"),
            name: name.to_string(),
            resources: vec![format!("/DCIM/100APPLE/{name}")],
            kind,
            size: 2048,
            timestamp,
            date_str: "2026-10-23".into(),
            month_key: "2026-10".into(),
            month_label: month.into(),
            week_label: "Week 42".into(),
            date_label: "Oct 23, 2026".into(),
        }
    }

    #[test]
    fn test_filtering_and_sorting() {
        let items = vec![
            sample_item("IMG_0001.JPG", "Photo", 100, "October 2026"),
            sample_item("IMG_0002.MOV", "Video", 200, "October 2026"),
            sample_item("IMG_0003.HEIC", "Photo + MOV", 300, "September 2026"),
        ];

        // Filter Photos
        let photos = Desktop::get_filtered_items(
            &items,
            "",
            MediaFilter::Photos,
            SortOrder::NewestFirst,
            None,
        );
        assert_eq!(photos.len(), 2);
        assert_eq!(photos[0].name, "IMG_0003.HEIC");
        assert_eq!(photos[1].name, "IMG_0001.JPG");

        // Filter Videos
        let videos = Desktop::get_filtered_items(
            &items,
            "",
            MediaFilter::Videos,
            SortOrder::NewestFirst,
            None,
        );
        assert_eq!(videos.len(), 1);
        assert_eq!(videos[0].name, "IMG_0002.MOV");

        // Filter Live Photos
        let live = Desktop::get_filtered_items(
            &items,
            "",
            MediaFilter::LivePhotos,
            SortOrder::NewestFirst,
            None,
        );
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].name, "IMG_0003.HEIC");

        // Filter by Month
        let sept = Desktop::get_filtered_items(
            &items,
            "",
            MediaFilter::All,
            SortOrder::NewestFirst,
            Some("September 2026"),
        );
        assert_eq!(sept.len(), 1);
        assert_eq!(sept[0].name, "IMG_0003.HEIC");

        // Search text
        let search = Desktop::get_filtered_items(
            &items,
            "0002",
            MediaFilter::All,
            SortOrder::NewestFirst,
            None,
        );
        assert_eq!(search.len(), 1);
        assert_eq!(search[0].name, "IMG_0002.MOV");
    }

    #[test]
    fn test_responsive_grid_math() {
        for width in [800.0f32, 900.0, 1366.0, 1600.0, 1920.0, 2560.0, 3840.0] {
            let available_width = (width - 16.0).max(200.0);
            let gap = 10.0;
            let target_size = 168.0;
            let columns = ((available_width + gap) / (target_size + gap))
                .floor()
                .max(1.0) as usize;
            assert!(columns >= 1);
            let card_width =
                ((available_width - gap * (columns as f32 - 1.0)) / columns as f32).floor();
            assert!(card_width >= 100.0);
            assert!(card_width <= 250.0);
        }
    }

    #[test]
    fn test_drag_selection_marquee_intersection() {
        // Test rectangle intersection math used by drag marquee selection
        let marquee = egui::Rect::from_min_max(egui::pos2(100.0, 100.0), egui::pos2(300.0, 300.0));

        let card_inside =
            egui::Rect::from_min_max(egui::pos2(120.0, 120.0), egui::pos2(200.0, 200.0));
        let card_overlapping =
            egui::Rect::from_min_max(egui::pos2(250.0, 250.0), egui::pos2(350.0, 350.0));
        let card_outside =
            egui::Rect::from_min_max(egui::pos2(400.0, 400.0), egui::pos2(500.0, 500.0));
        let card_adjacent =
            egui::Rect::from_min_max(egui::pos2(300.0, 100.0), egui::pos2(400.0, 200.0));

        assert!(card_inside.intersects(marquee));
        assert!(card_overlapping.intersects(marquee));
        assert!(!card_outside.intersects(marquee));
        assert!(card_adjacent.intersects(marquee));
    }
}
