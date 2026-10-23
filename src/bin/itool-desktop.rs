// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Stroke, Vec2};
use itool::{ExportEntry, MediaItem};
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
    Export(PathBuf, Vec<ExportEntry>),
}

enum Event {
    Status(String),
    PreviewReset,
    Scanned(Vec<MediaItem>),
    Preview(String, Option<egui::ColorImage>),
    Progress(u64, u64, String),
    Finished(PathBuf),
    Error(String),
}

#[derive(Clone, Debug)]
struct MonthSummary {
    key: String,
    label: String,
    count: usize,
    bytes: u64,
}

const GREEN: Color32 = Color32::from_rgb(22, 101, 52);
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
                text: Color32::from_rgb(231, 237, 233),
                muted: Color32::from_rgb(171, 185, 176),
                panel: Color32::from_rgb(28, 35, 31),
                sidebar: Color32::from_rgb(24, 30, 27),
                canvas: Color32::from_rgb(18, 24, 21),
                border: Color32::from_rgb(77, 94, 83),
                accent: Color32::from_rgb(137, 218, 165),
                tint: Color32::from_rgb(35, 63, 45),
                danger: Color32::from_rgb(255, 157, 148),
                warning: Color32::from_rgb(240, 193, 122),
            }
        } else {
            Self {
                text: Color32::from_rgb(30, 43, 36),
                muted: Color32::from_rgb(83, 99, 90),
                panel: Color32::WHITE,
                sidebar: Color32::from_rgb(249, 250, 251),
                canvas: Color32::from_rgb(243, 244, 246),
                border: Color32::from_rgb(183, 196, 187),
                accent: GREEN,
                tint: Color32::from_rgb(230, 242, 234),
                danger: Color32::from_rgb(158, 47, 39),
                warning: Color32::from_rgb(130, 70, 15),
            }
        }
    }
}
fn secondary(label: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(RichText::new(label.into()).size(13.0))
        .corner_radius(5)
        .min_size(Vec2::new(0.0, 28.0))
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
    .min_size(Vec2::new(0.0, 28.0))
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
    style.spacing.item_spacing = Vec2::new(8.0, 4.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.spacing.interact_size.y = 28.0;
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
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.5, p.accent);
    style.visuals.window_corner_radius = CornerRadius::same(8);
    style.visuals.widgets.inactive.corner_radius = CornerRadius::same(5);
    style.visuals.widgets.hovered.corner_radius = CornerRadius::same(5);
    style.visuals.widgets.active.corner_radius = CornerRadius::same(5);
    for (kind, size) in [
        (egui::TextStyle::Body, 13.0),
        (egui::TextStyle::Button, 13.0),
        (egui::TextStyle::Small, 11.5),
        (egui::TextStyle::Heading, 19.0),
    ] {
        style
            .text_styles
            .insert(kind, egui::FontId::proportional(size));
    }
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
    review_open: bool,
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
                            let result = runtime.block_on(itool::scan_media(|s| {
                                let _ = tx.send(Event::Status(s));
                                ctx.request_repaint();
                            }));
                            let _ = tx.send(match result {
                                Ok(items) => Event::Scanned(items),
                                Err(e) => Event::Error(format!("{e:#}")),
                            });
                            ctx.request_repaint();
                            continue;
                        }
                        Ok(Job::Export(dest, paths)) => {
                            pending_previews.clear();
                            let _ = tx.send(Event::PreviewReset);
                            preview_client = None;
                            let result = runtime.block_on(itool::export_planned(
                                &dest,
                                paths,
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
                            let result = runtime.block_on(itool::scan_media(|s| {
                                let _ = tx.send(Event::Status(s));
                                ctx.request_repaint();
                            }));
                            let _ = tx.send(match result {
                                Ok(items) => Event::Scanned(items),
                                Err(e) => Event::Error(format!("{e:#}")),
                            });
                            ctx.request_repaint();
                            break;
                        }
                        Job::Export(dest, paths) => {
                            pending_previews.clear();
                            let _ = tx.send(Event::PreviewReset);
                            preview_client = None;
                            let result = runtime.block_on(itool::export_planned(
                                &dest,
                                paths,
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

        // Use Windows' installed UI font when available; retain egui's portable fallback.
        if let Ok(windir) = std::env::var("WINDIR") {
            if let Ok(bytes) = std::fs::read(PathBuf::from(windir).join("Fonts/segoeui.ttf")) {
                let mut fonts = egui::FontDefinitions::default();
                fonts.font_data.insert(
                    "Segoe UI".into(),
                    Arc::new(egui::FontData::from_owned(bytes)),
                );
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "Segoe UI".into());
                cc.egui_ctx.set_fonts(fonts);
            }
        }
        let dark_mode = if std::env::args().any(|a| a == "--demo-dark") {
            true
        } else {
            read_dark_preference()
        };
        apply_theme(&cc.egui_ctx, dark_mode);

        let mut app = Self {
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
            group_by: GroupBy::Month,
            selected_month: None,
            show_sidebar: true,
            selected_only: false,
            tile_size: 168.0,
            export_layout: ExportLayout::Day,
            review_open: false,
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
        for index in 0..24 {
            let day = if index < 14 {
                "2026-10-23"
            } else {
                "2026-10-22"
            };
            let kind = match index % 4 {
                0 => "Video",
                1 => "Photo + MOV",
                _ => "Photo",
            };
            let ext = if kind == "Video" { "MOV" } else { "HEIC" };
            let name = format!("IMG_{:04}.{ext}", 1040 + index);
            let path = format!("/DCIM/100APPLE/{name}");
            let mut resources = vec![path.clone()];
            if kind == "Photo + MOV" {
                resources.push(path.replace(".HEIC", ".MOV"));
            }
            let image = egui::ColorImage::filled(
                [160, 120],
                Color32::from_rgb(95 + (index * 5) as u8, 125, 140),
            );
            self.add_texture(ctx, path.clone(), image);
            if index < 5 {
                self.selected.insert(path.clone());
            }
            self.items.push(MediaItem {
                path,
                name,
                resources,
                kind,
                size: if kind == "Video" {
                    420_000_000
                } else {
                    6_000_000
                },
                timestamp: 1792713600 - index * 3600,
                date_str: day.into(),
                month_key: "2026-10".into(),
                month_label: "October 2026".into(),
                week_label: "Week 43, 2026".into(),
                date_label: day.into(),
            });
        }
        self.group_by = GroupBy::Date;
        self.update_months_index();
        if std::env::args().any(|a| a == "--demo-review") {
            self.review_open = true;
            self.export_folder = Some(PathBuf::from(&self.destination).join("ITool-demo"));
        }
        self.list_view = std::env::args().any(|a| a == "--demo-list");
    }

    fn scan(&mut self) {
        if self.demo {
            return;
        }
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
        items
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
        let Some(dest) = self.export_folder.clone() else {
            return;
        };
        let parent = dest.parent().unwrap_or(std::path::Path::new(""));
        if !parent.is_dir() {
            self.error = Some("Choose an existing destination folder first.".into());
            return;
        }
        self.cancel.store(false, Ordering::Relaxed);
        self.busy = true;
        self.error = None;
        self.report = None;
        self.done = 0;
        self.total = 0;
        self.current_file_progress.clear();
        self.started = Some(Instant::now());
        self.review_open = false;
        self.status = "Preparing original media files for export…".into();
        if self.jobs.send(Job::Export(dest, entries)).is_err() {
            self.busy = false;
            self.error = Some("Transfer worker stopped. Restart ITool and retry.".into());
        }
    }
}

impl eframe::App for Desktop {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let p = Palette::new(self.dark_mode);

        // Process incoming background worker events
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Status(s) => self.status = s,
                Event::PreviewReset => self
                    .thumbnail_status
                    .retain(|_, status| *status != ThumbnailStatus::Loading),
                Event::Scanned(items) => {
                    self.status = format!("{} media items ready on iPhone", items.len());
                    self.items = items;
                    self.busy = false;
                    self.scanning = false;
                    self.update_months_index();
                }
                Event::Preview(path, Some(image)) => {
                    self.add_texture(&ctx, path, image);
                }
                Event::Preview(path, None) => {
                    self.thumbnail_status.insert(path, ThumbnailStatus::Failed);
                }
                Event::Progress(done, total, file_info) => {
                    self.done = done;
                    self.total = total;
                    self.current_file_progress = file_info;
                    self.status = "Exporting media files…".into();
                }
                Event::Finished(report) => {
                    self.busy = false;
                    self.started = None;
                    self.status = "Export complete. All files passed SHA-256 verification.".into();
                    self.current_file_progress.clear();
                    self.report = Some(report);
                }
                Event::Error(e) => {
                    self.busy = false;
                    self.scanning = false;
                    self.started = None;
                    self.error = Some(e);
                    self.status = "Operation stopped. Any completed files are preserved.".into();
                }
            }
        }

        // ================= TOP HEADER PANEL =================
        if !self.busy && !ctx.egui_wants_keyboard_input() && !self.review_open {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::A)) {
                let paths: Vec<_> = self
                    .filtered_items()
                    .iter()
                    .map(|i| i.path.clone())
                    .collect();
                self.selected.extend(paths);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.selected.clear();
                self.selected_only = false;
            }
        }
        egui::Panel::top("header_panel")
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .inner_margin(Margin::symmetric(20, 8))
                    .stroke(Stroke::new(1.0, p.border)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("ITool").size(24.0).strong().color(p.text));
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("iPhone media workspace").size(13.0).strong());
                        let status = if self.demo {
                            "Demo library · transfers disabled"
                        } else if self.scanning {
                            "Scanning your iPhone…"
                        } else if self.items.is_empty() {
                            "Connect by USB, unlock, then scan"
                        } else {
                            "Library scanned · keep iPhone connected"
                        };
                        ui.label(RichText::new(status).size(12.0).color(if self.demo {
                            p.warning
                        } else {
                            p.muted
                        }));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(secondary(if self.dark_mode {
                                "Light mode"
                            } else {
                                "Dark mode"
                            }))
                            .clicked()
                        {
                            self.dark_mode = !self.dark_mode;
                            apply_theme(&ctx, self.dark_mode);
                            if let Err(e) = save_dark_preference(self.dark_mode) {
                                self.status =
                                    format!("Theme changed; preference could not be saved: {e}");
                            }
                        }

                        if ui
                            .add_enabled(
                                !self.busy && !self.demo,
                                secondary(if self.scanning {
                                    "Scanning…"
                                } else if self.items.is_empty() {
                                    "Scan iPhone"
                                } else {
                                    "Rescan iPhone"
                                }),
                            )
                            .clicked()
                        {
                            self.scan();
                        }
                        if !self.items.is_empty()
                            && ui
                                .add(secondary(if self.show_sidebar {
                                    "Hide sidebar"
                                } else {
                                    "Show sidebar"
                                }))
                                .clicked()
                        {
                            self.show_sidebar = !self.show_sidebar;
                        }
                    });
                });
                ui.add_space(6.0);
                let step = if self.review_open {
                    2
                } else if self.busy && !self.scanning || self.report.is_some() {
                    3
                } else if !self.selected.is_empty() {
                    2
                } else if !self.items.is_empty() {
                    1
                } else {
                    0
                };
                ui.horizontal_wrapped(|ui| {
                    for (index, label) in [
                        "Connect & scan",
                        "Choose media",
                        "Review export",
                        "Transfer & verify",
                    ]
                    .iter()
                    .enumerate()
                    {
                        let current = step == index;
                        egui::Frame::new()
                            .fill(if current {
                                p.tint
                            } else {
                                Color32::TRANSPARENT
                            })
                            .corner_radius(5)
                            .inner_margin(Margin::symmetric(8, 4))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(format!("{}  {label}", index + 1))
                                        .size(12.0)
                                        .color(if current { p.accent } else { p.muted }),
                                );
                            });
                        if index < 3 {
                            ui.label(RichText::new("/").color(p.muted));
                        }
                    }
                });
                if !self.items.is_empty() {
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        let edit = egui::TextEdit::singleline(&mut self.search)
                            .hint_text("Search files, dates or folders")
                            .desired_width(206.0)
                            .margin(Vec2::new(10.0, 7.0));
                        if ui.add(edit).changed() {
                            let _ = self.jobs.send(Job::CancelPreviews);
                        }
                        if !self.search.is_empty() && ui.add(secondary("Clear search")).clicked() {
                            self.search.clear();
                        }
                        let photos = self.items.iter().filter(|i| i.kind != "Video").count();
                        let videos = self.items.iter().filter(|i| i.kind == "Video").count();
                        let live = self
                            .items
                            .iter()
                            .filter(|i| i.kind == "Photo + MOV")
                            .count();
                        for (filter, label) in [
                            (MediaFilter::All, format!("All  {}", self.items.len())),
                            (MediaFilter::Photos, format!("Photos  {photos}")),
                            (MediaFilter::Videos, format!("Videos  {videos}")),
                            (MediaFilter::LivePhotos, format!("Live Photos  {live}")),
                        ] {
                            if ui
                                .add(toggle(label, self.media_filter == filter, self.dark_mode))
                                .clicked()
                            {
                                self.media_filter = filter;
                                let _ = self.jobs.send(Job::CancelPreviews);
                            }
                        }
                        if ui
                            .add(secondary("Latest day"))
                            .on_hover_text("Filter to the newest device file date in this scan")
                            .clicked()
                            && let Some(item) = self.items.iter().max_by_key(|i| i.timestamp)
                        {
                            self.search = item.date_str.clone();
                            self.selected_month = None;
                            self.selected_only = false;
                        }
                    });
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("View").size(12.0).color(p.muted));
                        if ui
                            .add(toggle("Grid", !self.list_view, self.dark_mode))
                            .clicked()
                        {
                            self.list_view = false;
                        }
                        if ui
                            .add(toggle("Details", self.list_view, self.dark_mode))
                            .clicked()
                        {
                            self.list_view = true;
                        }
                        ui.add_space(8.0);
                        egui::ComboBox::from_id_salt("group_mode")
                            .width(155.0)
                            .selected_text(format!(
                                "Group: {}",
                                match self.group_by {
                                    GroupBy::Month => "Month",
                                    GroupBy::Week => "Week",
                                    GroupBy::Date => "Day",
                                    GroupBy::Continuous => "None",
                                    GroupBy::MediaType => "Media type",
                                    GroupBy::SourceFolder => "Source folder",
                                }
                            ))
                            .show_ui(ui, |ui| {
                                for (value, label) in [
                                    (GroupBy::Month, "Month"),
                                    (GroupBy::Week, "Week"),
                                    (GroupBy::Date, "Day"),
                                    (GroupBy::MediaType, "Media type"),
                                    (GroupBy::SourceFolder, "Source folder"),
                                    (GroupBy::Continuous, "No grouping"),
                                ] {
                                    ui.selectable_value(&mut self.group_by, value, label);
                                }
                            });
                        egui::ComboBox::from_id_salt("sort_order")
                            .width(165.0)
                            .selected_text(format!(
                                "Order: {}",
                                match self.sort_order {
                                    SortOrder::NewestFirst => "Newest first",
                                    SortOrder::OldestFirst => "Oldest first",
                                    SortOrder::LargestFirst => "Largest first",
                                    SortOrder::SmallestFirst => "Smallest first",
                                    SortOrder::NameAscending => "Name A–Z",
                                    SortOrder::NameDescending => "Name Z–A",
                                }
                            ))
                            .show_ui(ui, |ui| {
                                for (value, label) in [
                                    (SortOrder::NewestFirst, "Newest first"),
                                    (SortOrder::OldestFirst, "Oldest first"),
                                    (SortOrder::LargestFirst, "Largest first"),
                                    (SortOrder::SmallestFirst, "Smallest first"),
                                    (SortOrder::NameAscending, "Name A–Z"),
                                    (SortOrder::NameDescending, "Name Z–A"),
                                ] {
                                    ui.selectable_value(&mut self.sort_order, value, label);
                                }
                            });
                        if !self.list_view {
                            ui.label(RichText::new("Thumbnail size").size(12.0).color(p.muted));
                            ui.spacing_mut().slider_width = 70.0;
                            ui.add(
                                egui::Slider::new(&mut self.tile_size, 128.0..=256.0)
                                    .show_value(false),
                            );
                        }
                    });
                    if let Some(month) = self.selected_month.clone() {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("Month filter: {month}"))
                                    .size(12.0)
                                    .color(p.accent),
                            );
                            if ui.small_button("Remove filter").clicked() {
                                self.selected_month = None;
                            }
                        });
                    }
                }
            });

        // ================= BOTTOM EXPORT DOCK =================
        egui::Panel::bottom("transfer_panel")
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .inner_margin(Margin::symmetric(20, 12))
                    .stroke(Stroke::new(1.0, p.border)),
            )
            .show(ui, |ui| {
                // Row 1: Destination folder picker
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Export to").size(13.0).strong().color(p.text));
                    let max_width = (ui.available_width() - 155.0).max(100.0);
                    ui.add_enabled(
                        !self.busy,
                        egui::TextEdit::singleline(&mut self.destination)
                            .desired_width(max_width)
                            .margin(Vec2::new(10.0, 7.0)),
                    );
                    if ui
                        .add_enabled(!self.busy, secondary("Choose folder…"))
                        .clicked()
                        && let Some(p) = rfd::FileDialog::new().pick_folder()
                    {
                        self.destination = p.display().to_string();
                    }
                });

                ui.add_space(8.0);

                // Row 2: Export primary action and metrics
                ui.horizontal_wrapped(|ui| {
                    let selected_count = self.selected.len();
                    let selected_bytes: u64 = self
                        .items
                        .iter()
                        .filter(|i| self.selected.contains(&i.path))
                        .map(|i| i.size)
                        .sum();

                    let can_export =
                        !self.busy && selected_count > 0 && !self.destination.trim().is_empty();

                    let export_text = if selected_count > 0 {
                        format!(
                            "Review {} item{} ({})",
                            selected_count,
                            if selected_count == 1 { "" } else { "s" },
                            format_size(selected_bytes)
                        )
                    } else {
                        "Select photos or videos to export".to_string()
                    };

                    let export_btn = egui::Button::new(
                        RichText::new(export_text)
                            .size(14.0)
                            .color(Color32::WHITE)
                            .strong(),
                    )
                    .fill(if can_export { GREEN } else { p.muted })
                    .min_size(Vec2::new(220.0, 36.0))
                    .corner_radius(CornerRadius::same(6));

                    if ui.add_enabled(can_export, export_btn).clicked() {
                        let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S-%3f");
                        self.export_folder = Some(
                            PathBuf::from(self.destination.trim()).join(format!("ITool-{stamp}")),
                        );
                        self.review_open = true;
                    }

                    // Cancel button during active export
                    if self.busy && !self.scanning {
                        ui.add_space(8.0);
                        let cancel_btn = egui::Button::new(
                            RichText::new("Cancel Export")
                                .size(13.0)
                                .color(p.danger)
                                .strong(),
                        )
                        .corner_radius(CornerRadius::same(6));

                        if ui.add(cancel_btn).clicked() {
                            self.cancel.store(true, Ordering::Relaxed);
                            self.status = "Cancelling… Completed files are preserved.".into();
                        }
                    }

                    ui.add_space(14.0);

                    // Integrity guarantee note
                    ui.label(
                        RichText::new("Originals + Live Photo pairs · Phone unchanged")
                            .size(11.5)
                            .color(p.muted),
                    );
                });

                // Active transfer progress bar & metrics
                if self.busy && !self.scanning {
                    ui.add_space(8.0);
                    let ratio = if self.total > 0 {
                        (self.done as f32 / self.total as f32).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    ui.add(egui::ProgressBar::new(ratio).show_percentage());

                    ui.horizontal(|ui| {
                        if !self.current_file_progress.is_empty() {
                            ui.label(
                                RichText::new(format!("Copying: {}", self.current_file_progress))
                                    .size(12.0)
                                    .strong()
                                    .color(p.text),
                            );
                        }

                        if let Some(start) = self.started {
                            let elapsed = start.elapsed().as_secs_f64();
                            let speed = (self.done as f64 / 1_048_576.0) / elapsed.max(0.001);
                            ui.add_space(10.0);
                            ui.label(
                                RichText::new(format!(
                                    "· {} / {} ({:.2} MiB/s)",
                                    format_size(self.done),
                                    format_size(self.total),
                                    speed
                                ))
                                .size(12.0)
                                .color(p.muted),
                            );
                        }
                    });
                }

                // Status & Notifications
                ui.add_space(4.0);
                ui.label(RichText::new(&self.status).size(12.0).color(p.text));

                if let Some(ref e) = self.error {
                    ui.add_space(2.0);
                    ui.colored_label(p.danger, format!("Error: {e}"));
                    if let Some(folder) = &self.export_folder
                        && folder.is_dir()
                        && ui.button("Open incomplete export").clicked()
                    {
                        let _ = std::process::Command::new("explorer.exe")
                            .arg(folder)
                            .spawn();
                    }
                }

                if let Some(ref report) = self.report {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("✓ Saved into: {}", report.display()))
                                .strong()
                                .size(12.5)
                                .color(p.accent),
                        );
                        if ui
                            .button(
                                RichText::new("Open folder in Windows Explorer")
                                    .size(12.0)
                                    .color(p.accent),
                            )
                            .clicked()
                        {
                            let _ = std::process::Command::new("explorer.exe")
                                .arg(report.as_os_str())
                                .spawn();
                        }
                    });
                }
            });

        // ================= MONTH OVERVIEW SIDEBAR =================
        if self.show_sidebar && !self.items.is_empty() {
            egui::Panel::left("month_overview_sidebar")
                .frame(
                    egui::Frame::new()
                        .fill(p.sidebar)
                        .inner_margin(Margin::symmetric(12, 14))
                        .stroke(Stroke::new(1.0, p.border)),
                )
                .min_size(230.0)
                .max_size(280.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Months Overview")
                                .size(14.5)
                                .strong()
                                .color(p.text),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let total_bytes: u64 = self.items.iter().map(|i| i.size).sum();
                            ui.label(
                                RichText::new(format_size(total_bytes))
                                    .size(11.0)
                                    .color(p.muted),
                            );
                        });
                    });
                    ui.label(
                        RichText::new(format!("{} items total", self.items.len()))
                            .size(11.5)
                            .color(p.muted),
                    );

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            // "All Media" top row
                            let all_active = self.selected_month.is_none();
                            let (all_rect, all_resp) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 32.0),
                                egui::Sense::click(),
                            );
                            if all_resp.clicked() {
                                self.selected_month = None;
                                let _ = self.jobs.send(Job::CancelPreviews);
                            }

                            let bg_color = if all_active {
                                GREEN
                            } else if all_resp.hovered() {
                                p.canvas
                            } else {
                                Color32::TRANSPARENT
                            };
                            ui.painter()
                                .rect_filled(all_rect, CornerRadius::same(6), bg_color);

                            let text_color = if all_active { Color32::WHITE } else { p.text };
                            let sub_color = if all_active {
                                Color32::from_rgb(220, 252, 231)
                            } else {
                                p.muted
                            };

                            ui.painter().text(
                                all_rect.left_center() + Vec2::new(10.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                "All Media",
                                egui::FontId::proportional(13.0),
                                text_color,
                            );
                            ui.painter().text(
                                all_rect.right_center() + Vec2::new(-10.0, 0.0),
                                egui::Align2::RIGHT_CENTER,
                                format!("{}", self.items.len()),
                                egui::FontId::proportional(11.5),
                                sub_color,
                            );

                            ui.add_space(4.0);

                            // Individual Month rows
                            for month in &self.months {
                                let is_active =
                                    self.selected_month.as_deref() == Some(&month.label);
                                let (row_rect, row_resp) = ui.allocate_exact_size(
                                    Vec2::new(ui.available_width(), 30.0),
                                    egui::Sense::click(),
                                );

                                if row_resp.clicked() {
                                    if is_active {
                                        self.selected_month = None;
                                    } else {
                                        self.selected_month = Some(month.label.clone());
                                    }
                                    let _ = self.jobs.send(Job::CancelPreviews);
                                }

                                let bg = if is_active {
                                    GREEN
                                } else if row_resp.hovered() {
                                    p.canvas
                                } else {
                                    Color32::TRANSPARENT
                                };
                                ui.painter()
                                    .rect_filled(row_rect, CornerRadius::same(6), bg);

                                let label_col = if is_active { Color32::WHITE } else { p.text };
                                let meta_col = if is_active {
                                    Color32::from_rgb(220, 252, 231)
                                } else {
                                    p.muted
                                };

                                ui.painter().text(
                                    row_rect.left_center() + Vec2::new(10.0, 0.0),
                                    egui::Align2::LEFT_CENTER,
                                    &month.label,
                                    egui::FontId::proportional(12.5),
                                    label_col,
                                );

                                let meta_str =
                                    format!("{} · {}", month.count, format_size(month.bytes));
                                ui.painter().text(
                                    row_rect.right_center() + Vec2::new(-10.0, 0.0),
                                    egui::Align2::RIGHT_CENTER,
                                    &meta_str,
                                    egui::FontId::proportional(11.0),
                                    meta_col,
                                );

                                ui.add_space(2.0);
                            }
                        });
                });
        }

        // ================= MAIN MEDIA AREA =================
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.canvas))
            .show(ui, |ui| {
                // If no items loaded, show welcome empty state
                if self.items.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(100.0);
                        let icon_rect = ui
                            .allocate_exact_size(Vec2::new(64.0, 64.0), egui::Sense::hover())
                            .0;
                        ui.painter().circle_filled(
                            icon_rect.center(),
                            32.0,
                            p.tint,
                        );
                        ui.painter().text(
                            icon_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "USB",
                            egui::FontId::proportional(32.0),
                            Color32::WHITE,
                        );

                        ui.add_space(16.0);
                        ui.heading(
                            RichText::new(if self.scanning {
                                "Cataloging your iPhone media…"
                            } else {
                                "Ready to export your iPhone media"
                            })
                            .size(22.0)
                            .strong()
                            .color(p.text),
                        );
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("Connect by USB, unlock your phone, and tap 'Trust' if asked.")
                                .size(13.5)
                                .color(p.muted),
                        );
                        ui.label(
                            RichText::new("Click 'Scan iPhone' to view your photos and videos without modifying the phone.")
                                .size(13.5)
                                .color(p.muted),
                        );
                        ui.add_space(22.0);

                        if self.scanning {
                            ui.spinner();
                            ui.add_space(8.0);
                            ui.label(RichText::new(&self.status).size(13.0).color(p.text));
                        } else {
                            let scan_btn = egui::Button::new(
                                RichText::new("Scan iPhone Now")
                                    .size(15.0)
                                    .color(Color32::WHITE)
                                    .strong(),
                            )
                            .fill(GREEN)
                            .min_size(Vec2::new(180.0, 42.0))
                            .corner_radius(CornerRadius::same(8));

                            if ui.add_enabled(!self.busy, scan_btn).clicked() {
                                self.scan();
                            }
                        }
                    });
                    return;
                }

                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(8.0);
                    let shown = self.filtered_items();
                    let visible = shown.len();
                    let all_selected = !shown.is_empty() && shown.iter().all(|i| self.selected.contains(&i.path));
                    ui.label(RichText::new(format!("{visible} items")).strong().size(14.0))
                        .on_hover_text("Click to select · Shift-click a range · Drag across the grid · Ctrl+A selects shown items");
                    ui.label(RichText::new(format!("{} selected",self.selected.len())).size(13.0).color(p.accent));
                    ui.add_space(8.0);
                    if ui.add_enabled(!self.busy && visible > 0,secondary(if all_selected { "Deselect shown" } else { "Select shown" })).clicked() {
                        let paths: Vec<_> = self.filtered_items().iter().map(|i| i.path.clone()).collect();
                        for p in paths { if all_selected { self.selected.remove(&p); } else { self.selected.insert(p); } }
                    }
                    if ui.add_enabled(!self.busy && visible > 0,secondary("Invert shown")).clicked() {
                        let paths: Vec<_> = self.filtered_items().iter().map(|i| i.path.clone()).collect();
                        for p in paths { if !self.selected.remove(&p) { self.selected.insert(p); } }
                    }
                    let clear = egui::Button::new(RichText::new("Clear selection").size(13.0).color(p.danger))
                        .fill(Color32::TRANSPARENT).stroke(Stroke::NONE).min_size(Vec2::new(0.0,30.0));
                    if ui.add_enabled(!self.busy && !self.selected.is_empty(),clear).clicked() { self.selected.clear(); self.selected_only = false; }
                    ui.checkbox(&mut self.selected_only,"Selected only");
                });
                ui.add_space(6.0);

                let mut filtered = Self::get_filtered_items(
                    &self.items,
                    &self.search,
                    self.media_filter,
                    self.sort_order,
                    self.selected_month.as_deref(),
                );
                if self.selected_only { filtered.retain(|i| self.selected.contains(&i.path)); }
                let available_width = (ui.available_width() - 16.0).max(200.0);
                let gap = 10.0;
                let target_size = self.tile_size;
                let columns = ((available_width + gap) / (target_size + gap))
                    .floor()
                    .max(1.0) as usize;
                let card_width =
                    ((available_width - gap * (columns as f32 - 1.0)) / columns as f32).floor();
                let card_height = card_width.min((ui.available_height() - 64.0).max(120.0));

                if filtered.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(80.0);
                        ui.heading(
                            RichText::new("No media matches the current filter")
                                .size(18.0)
                                .color(p.text),
                        );
                        ui.add_space(8.0);
                        if ui.button("Clear search and filters").clicked() {
                            self.search.clear();
                            self.media_filter = MediaFilter::All;
                            self.selected_month = None;
                            self.selected_only = false;
                        }
                    });
                    return;
                }

                if self.group_by != GroupBy::Continuous {
                    filtered = grouped_items(&filtered, self.group_by).into_iter().flat_map(|(_,items)| items).collect();
                }

                // Map paths to global filtered indices for Shift-click range selection
                let item_indices: HashMap<String, usize> = filtered
                    .iter()
                    .enumerate()
                    .map(|(idx, item)| (item.path.clone(), idx))
                    .collect();

                let gallery_rect = ui.max_rect();
                let is_primary_down = ui.input(|i| i.pointer.primary_down());
                let is_dragging = ui.input(|i| i.pointer.is_decidedly_dragging());
                let press_origin = ui.input(|i| i.pointer.press_origin());
                let latest_pos = ui.input(|i| i.pointer.latest_pos());
                let is_shift = ui.input(|i| i.modifiers.shift);
                let is_ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);

                // Detect drag-to-select marquee
                let mut active_marquee: Option<egui::Rect> = None;
                if !self.list_view && is_dragging && is_primary_down {
                    if let (Some(origin), Some(pos)) = (press_origin, latest_pos) {
                        if gallery_rect.contains(origin) {
                            if !self.drag_active {
                                self.drag_active = true;
                                self.drag_base_selected = if is_shift || is_ctrl {
                                    self.selected.clone()
                                } else {
                                    HashSet::new()
                                };
                            }
                            active_marquee = Some(egui::Rect::from_two_pos(origin, pos));
                            ctx.request_repaint();

                            // Auto-scroll when dragging near top or bottom edges
                            if pos.y < gallery_rect.min.y + 45.0 {
                                ui.scroll_with_delta(Vec2::new(0.0, 20.0));
                            } else if pos.y > gallery_rect.max.y - 45.0 {
                                ui.scroll_with_delta(Vec2::new(0.0, -20.0));
                            }
                        }
                    }
                } else if !is_primary_down && self.drag_active {
                    self.drag_active = false;
                    self.drag_base_selected.clear();
                }

                let mut intersected_in_drag: HashSet<String> = HashSet::new();
                let mut card_click: Option<(usize, bool)> = None; // (index, is_shift)
                let mut retry_preview: Option<String> = None;
                let mut group_toggle: Option<(Vec<String>, bool)> = None; // (paths, select_all)
                let mut needed_previews: Vec<String> = Vec::new();

                let is_continuous = self.group_by == GroupBy::Continuous;

                if self.list_view {
                    egui::ScrollArea::vertical().auto_shrink([false,false]).show(ui, |ui| {
                        let groups = grouped_items(&filtered, self.group_by);
                        for (title,items) in groups {
                            if self.group_by != GroupBy::Continuous { ui.heading(title); }
                            ui.horizontal(|ui| {
                                ui.add_space(30.0);
                                ui.add_sized([210.0,24.0],egui::Label::new(RichText::new("File").strong()));
                                ui.add_sized([100.0,24.0],egui::Label::new("Device date"));
                                ui.add_sized([85.0,24.0],egui::Label::new("Size"));
                                ui.label("Type / resources");
                            });
                            let row_height = 32.0;
                            for item in items {
                                let rect = egui::Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(),row_height));
                                if !ui.is_rect_visible(rect) { ui.allocate_space(rect.size()); continue; }
                                ui.horizontal(|ui| {
                                    let mut checked = self.selected.contains(&item.path);
                                    if ui.add_enabled(!self.busy,egui::Checkbox::without_text(&mut checked)).clicked() {
                                        card_click = Some((item_indices[&item.path],ui.input(|i| i.modifiers.shift)));
                                    }
                                    let label = egui::Label::new(RichText::new(safe_truncate(&item.name,27)).strong()).sense(egui::Sense::click());
                                    if ui.add_sized([210.0,24.0],label).on_hover_text(&item.path).clicked() && !self.busy {
                                        card_click = Some((item_indices[&item.path],ui.input(|i| i.modifiers.shift)));
                                    }
                                    ui.add_sized([100.0,24.0],egui::Label::new(&item.date_str));
                                    ui.add_sized([85.0,24.0],egui::Label::new(format_size(item.size)));
                                    ui.label(format!("{} · {} files",media_label(item),item.resources.len()));
                                });
                            }
                            ui.add_space(12.0);
                        }
                    });
                } else if is_continuous {
                    // Continuous virtualized gallery
                    let total_rows = filtered.len().div_ceil(columns);
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add_space(8.0);
                            for row_idx in 0..total_rows {
                                let row_rect = egui::Rect::from_min_size(
                                    ui.cursor().min,
                                    Vec2::new(available_width, card_height + gap),
                                );

                                if ui.is_rect_visible(row_rect) {
                                    let start = row_idx * columns;
                                    let end = (start + columns).min(filtered.len());
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = gap;
                                        for item in &filtered[start..end] {
                                            let idx = item_indices[&item.path];
                                            let status = self
                                                .thumbnail_status
                                                .get(&item.path)
                                                .copied()
                                                .unwrap_or(ThumbnailStatus::Loading);
                                            let has_status =
                                                self.thumbnail_status.contains_key(&item.path);

                                            Self::render_media_card(
                                                ui,
                                                item,
                                                idx,
                                                card_width,
                                                card_height,
                                                self.selected.contains(&item.path),
                                                self.textures.get(&item.path),
                                                status,
                                                active_marquee,
                                                &mut intersected_in_drag,
                                                &mut card_click,
                                                &mut retry_preview,
                                                self.busy,
                                                self.drag_active,
                                            );

                                            if !has_status {
                                                needed_previews.push(item.path.clone());
                                            }
                                        }
                                    });
                                    ui.add_space(gap);
                                } else {
                                    ui.allocate_space(Vec2::new(
                                        available_width,
                                        card_height + gap,
                                    ));
                                }
                            }
                            ui.add_space(16.0);
                        });
                } else {
                    // Grouped virtualized gallery (Month, Week, or Date)
                    let groups = grouped_items(&filtered, self.group_by);

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for (group_title, group_items) in &groups {
                                ui.add_space(8.0);
                                // Group Header
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(group_title)
                                            .size(16.0)
                                            .strong()
                                            .color(p.text),
                                    );
                                    let group_bytes: u64 =
                                        group_items.iter().map(|i| i.size).sum();
                                    ui.label(
                                        RichText::new(format!(
                                            "· {} item{} ({})",
                                            group_items.len(),
                                            if group_items.len() == 1 { "" } else { "s" },
                                            format_size(group_bytes)
                                        ))
                                        .size(12.0)
                                        .color(p.muted),
                                    );

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let all_selected = group_items
                                                .iter()
                                                .all(|i| self.selected.contains(&i.path));
                                            let btn_text = if all_selected {
                                                "Deselect group"
                                            } else {
                                                "Select group"
                                            };
                                            let btn = secondary(btn_text);

                                            if ui.add_enabled(!self.busy, btn).clicked() {
                                                let paths: Vec<String> = group_items
                                                    .iter()
                                                    .map(|i| i.path.clone())
                                                    .collect();
                                                group_toggle = Some((paths, !all_selected));
                                            }
                                        },
                                    );
                                });

                                ui.add_space(4.0);
                                ui.separator();
                                ui.add_space(6.0);

                                // Group Rows Virtualization
                                let total_group_rows = group_items.len().div_ceil(columns);
                                for row_idx in 0..total_group_rows {
                                    let row_rect = egui::Rect::from_min_size(
                                        ui.cursor().min,
                                        Vec2::new(available_width, card_height + gap),
                                    );

                                    if ui.is_rect_visible(row_rect) {
                                        let start = row_idx * columns;
                                        let end = (start + columns).min(group_items.len());
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = gap;
                                            for item in &group_items[start..end] {
                                                let idx = item_indices[&item.path];
                                                let status = self
                                                    .thumbnail_status
                                                    .get(&item.path)
                                                    .copied()
                                                    .unwrap_or(ThumbnailStatus::Loading);
                                                let has_status =
                                                    self.thumbnail_status.contains_key(&item.path);

                                                Self::render_media_card(
                                                    ui,
                                                    item,
                                                    idx,
                                                    card_width,
                                                    card_height,
                                                    self.selected.contains(&item.path),
                                                    self.textures.get(&item.path),
                                                    status,
                                                    active_marquee,
                                                    &mut intersected_in_drag,
                                                    &mut card_click,
                                                    &mut retry_preview,
                                                    self.busy,
                                                    self.drag_active,
                                                );

                                                if !has_status {
                                                    needed_previews.push(item.path.clone());
                                                }
                                            }
                                        });
                                        ui.add_space(gap);
                                    } else {
                                        ui.allocate_space(Vec2::new(
                                            available_width,
                                            card_height + gap,
                                        ));
                                    }
                                }
                                ui.add_space(14.0);
                            }
                        });
                }

                // If drag selection was active, update selection and paint selection box
                if let Some(marquee) = active_marquee {
                    let mut new_selection = self.drag_base_selected.clone();
                    for p in &intersected_in_drag {
                        new_selection.insert(p.clone());
                    }
                    self.selected = new_selection;

                    // Paint green selection marquee rectangle overlay
                    ui.painter().rect_filled(
                        marquee,
                        CornerRadius::same(3),
                        Color32::from_rgba_premultiplied(22, 101, 52, 35),
                    );
                    ui.painter().rect_stroke(
                        marquee,
                        CornerRadius::same(3),
                        Stroke::new(1.5, p.accent),
                        egui::StrokeKind::Inside,
                    );
                }

                // Handle single-click and Shift-click range selection
                if let Some((curr_idx, is_shift)) = card_click {
                    let anchor = self.selection_anchor.as_ref().and_then(|p| item_indices.get(p)).copied();
                    if let Some(prev_idx) = anchor.filter(|_| is_shift) {
                        let start = prev_idx.min(curr_idx);
                        let end = prev_idx.max(curr_idx);
                        let paths_to_add: Vec<String> = filtered[start..=end].iter().map(|i| i.path.clone()).collect();
                        for path in paths_to_add {
                            self.selected.insert(path);
                        }
                    } else {
                        let path = filtered[curr_idx].path.clone();
                        self.selection_anchor = Some(path.clone());
                        if self.selected.contains(&path) {
                            self.selected.remove(&path);
                        } else {
                            self.selected.insert(path);
                        }
                    }
                }

                // Handle group select/deselect
                if let Some((paths, do_select)) = group_toggle {
                    for path in paths {
                        if do_select {
                            self.selected.insert(path);
                        } else {
                            self.selected.remove(&path);
                        }
                    }
                }

                // Handle thumbnail retry
                if let Some(path) = retry_preview {
                    self.thumbnail_status.remove(&path);
                    self.request_preview_if_needed(&path);
                }

                // Request previews only for visible cards
                for path in needed_previews {
                    self.request_preview_if_needed(&path);
                }
            });

        self.capture_frames += 1;
        if let Ok(path) = std::env::var("ITOOL_UI_CAPTURE") {
            if self.capture_frames == 8 {
                let capture_ctx = ctx.clone();
                ctx.request_screenshot(move |img| {
                    let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                    if let Err(e) = image::save_buffer(
                        &path,
                        &bytes,
                        img.size[0] as u32,
                        img.size[1] as u32,
                        image::ColorType::Rgba8,
                    ) {
                        eprintln!("Screenshot failed: {e}");
                    }
                    capture_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                });
            }
            ctx.request_repaint();
        }
        if self.review_open {
            let mut open = true;
            egui::Window::new("Review export")
                .open(&mut open).collapsible(false).resizable(true)
                .default_width(640.0).max_height(480.0).anchor(egui::Align2::CENTER_CENTER,Vec2::ZERO)
                .show(&ctx, |ui| {
                    ui.heading("Ready for your edit");
                    let count = self.selected.len();
                    let bytes: u64 = self.items.iter().filter(|i| self.selected.contains(&i.path)).map(|i| i.size).sum();
                    ui.label(format!("{count} selected items · {} · Live Photo stills and clips stay together", format_size(bytes)));
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Organize files:");
                        egui::ComboBox::from_id_salt("export_layout").selected_text(self.export_layout.label()).show_ui(ui, |ui| {
                            for layout in [ExportLayout::Flat,ExportLayout::Day,ExportLayout::Month,ExportLayout::MediaType,ExportLayout::SourceFolder] {
                                ui.selectable_value(&mut self.export_layout,layout,layout.label());
                            }
                        });
                    });
                    if let Some(folder) = &self.export_folder { ui.label(format!("New folder: {}", folder.display())); }
                    ui.label("Folder dates use device file dates, not EXIF capture dates. Originals keep their names.");
                    let plan = self.export_plan();
                    ui.label(format!("{} original files · Copy order follows grouping and sorting", plan.len()));
                    let shown_selected = self.filtered_items().iter().filter(|i| self.selected.contains(&i.path)).count();
                    if shown_selected < count { ui.label(format!("Includes {} selected items hidden by current filters.",count - shown_selected)); }
                    ui.separator();
                    egui::ScrollArea::vertical().max_height(190.0).show_rows(ui, 20.0, plan.len(), |ui, range| {
                        for entry in &plan[range] {
                            let name = entry.source.rsplit('/').next().unwrap_or("");
                            let relative = if entry.folder.is_empty() { name.into() } else { format!("{}/{name}",entry.folder) };
                            ui.label(relative).on_hover_text(&entry.source);
                        }
                    });
                    ui.separator();
                    let validation = itool::validate_export_plan(&plan);
                    let parent_exists = self.export_folder.as_ref().and_then(|p| p.parent()).is_some_and(|p| p.is_dir());
                    if let Err(e) = &validation { ui.colored_label(p.danger,e.to_string()); }
                    if !parent_exists { ui.colored_label(p.danger,"Choose an existing destination folder."); }
                    ui.label("Each file is reread and checked against its transfer SHA-256. Existing exports are never overwritten.");
                    ui.horizontal(|ui| {
                        if ui.add(secondary("Back to selection")).clicked() { self.review_open = false; }
                        if ui.add_enabled(!self.busy && !self.demo && validation.is_ok() && parent_exists,
                            egui::Button::new(RichText::new("Start transfer").color(Color32::WHITE)).fill(GREEN))
                            .clicked() { self.begin_export(); }
                    });
                });
            if !open {
                self.review_open = false;
            }
        }

        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
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
        let (rect, response) = ui.allocate_exact_size(tile_size, egui::Sense::click_and_drag());
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
            Color32::from_rgba_premultiplied(22, 101, 52, 22)
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

                    // Fit uniformly within the card while preserving original aspect ratio
                    let (draw_w, draw_h) = if rect.width() / rect.height() > aspect {
                        (rect.height() * aspect, rect.height())
                    } else {
                        (rect.width(), rect.width() / aspect)
                    };
                    let draw_rect =
                        egui::Rect::from_center_size(rect.center(), Vec2::new(draw_w, draw_h));

                    ui.painter().image(
                        tex.id(),
                        draw_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
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
                rect.min + Vec2::new(8.0, 8.0),
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

        // 5. Checkmark selection circle in top-right
        let check_center = egui::pos2(rect.max.x - 16.0, rect.min.y + 16.0);
        if is_selected {
            ui.painter().circle_filled(check_center, 10.0, GREEN);
            // Crisp white checkmark ✓
            let p1 = check_center + Vec2::new(-3.5, 0.0);
            let p2 = check_center + Vec2::new(-0.8, 3.2);
            let p3 = check_center + Vec2::new(4.2, -3.5);
            ui.painter()
                .line_segment([p1, p2], Stroke::new(2.0, Color32::WHITE));
            ui.painter()
                .line_segment([p2, p3], Stroke::new(2.0, Color32::WHITE));
        } else if is_hovered {
            ui.painter().circle_filled(
                check_center,
                10.0,
                Color32::from_rgba_premultiplied(0, 0, 0, 80),
            );
            ui.painter()
                .circle_stroke(check_center, 9.5, Stroke::new(1.5, Color32::WHITE));
        }

        // 6. Frosted bottom metadata overlay (Filename + Size)
        let overlay_height = 42.0;
        let overlay_rect = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, rect.max.y - overlay_height),
            Vec2::new(rect.width(), overlay_height),
        );
        ui.painter().rect_filled(
            overlay_rect,
            CornerRadius {
                nw: 0,
                ne: 0,
                sw: 5,
                se: 5,
            },
            Color32::from_rgba_premultiplied(0, 0, 0, 160),
        );

        let short_name = safe_truncate(&item.name, 13);
        ui.painter().text(
            overlay_rect.min + Vec2::new(6.0, 5.0),
            egui::Align2::LEFT_TOP,
            &short_name,
            egui::FontId::proportional(10.5),
            Color32::WHITE,
        );

        let size_str = format_size(item.size);
        ui.painter().text(
            egui::pos2(overlay_rect.max.x - 6.0, overlay_rect.min.y + 24.0),
            egui::Align2::RIGHT_TOP,
            &size_str,
            egui::FontId::proportional(9.5),
            Color32::from_rgb(209, 213, 219),
        );
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(if std::env::var_os("ITOOL_UI_COMPACT").is_some() {
                [900.0, 600.0]
            } else {
                [1240.0, 860.0]
            })
            .with_min_inner_size([900.0, 600.0])
            .with_title("ITool — iPhone Original Media Exporter"),
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
