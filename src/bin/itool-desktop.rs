// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Stroke, Vec2};
use itool::MediaItem;
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
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GroupBy {
    Month,
    Week,
    Date,
    Continuous,
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
    Export(PathBuf, Vec<String>),
}

enum Event {
    Status(String),
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
    last_clicked_index: Option<usize>,

    // Filters and controls
    search: String,
    media_filter: MediaFilter,
    sort_order: SortOrder,
    group_by: GroupBy,
    selected_month: Option<String>,
    show_sidebar: bool,

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
                            preview_client = None;
                            let result = runtime.block_on(itool::export_media(
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
                            preview_client = None;
                            let result = runtime.block_on(itool::export_media(
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
                                if pending_previews.len() > 80 {
                                    pending_previews.pop_front();
                                }
                            }
                        }
                        Job::CancelPreviews => {
                            pending_previews.clear();
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

        // Configure visuals for clean Windows 11 desktop styling
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let mut style = (*cc.egui_ctx.style_of(egui::Theme::Light)).clone();
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 6.0);
        style.visuals.selection.bg_fill = Color32::from_rgb(22, 101, 52);
        style.visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        style.visuals.window_corner_radius = CornerRadius::same(8);
        cc.egui_ctx.set_style_of(egui::Theme::Light, style);

        Self {
            jobs,
            events,
            cancel,
            items: vec![],
            selected: HashSet::new(),
            textures: HashMap::new(),
            thumbnail_status: HashMap::new(),
            texture_lru: VecDeque::new(),
            last_clicked_index: None,

            search: String::new(),
            media_filter: MediaFilter::All,
            sort_order: SortOrder::NewestFirst,
            group_by: GroupBy::Month,
            selected_month: None,
            show_sidebar: true,
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
        }
    }

    fn scan(&mut self) {
        self.items.clear();
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
        self.last_clicked_index = None;
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
        if !self.busy && !self.thumbnail_status.contains_key(path) {
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
        }
        list
    }

    fn filtered_items(&self) -> Vec<&MediaItem> {
        Self::get_filtered_items(
            &self.items,
            &self.search,
            self.media_filter,
            self.sort_order,
            self.selected_month.as_deref(),
        )
    }
}

impl eframe::App for Desktop {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Process incoming background worker events
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Status(s) => self.status = s,
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
        egui::Panel::top("header_panel")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(255, 255, 255))
                    .inner_margin(Margin::symmetric(20, 12))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(229, 231, 235))),
            )
            .show(ui, |ui| {
                // Row 1: Brand title, connection pill, and primary actions
                ui.horizontal(|ui| {
                    // App Brand icon + title
                    let logo_rect = ui
                        .allocate_exact_size(Vec2::new(32.0, 32.0), egui::Sense::hover())
                        .0;
                    ui.painter().rect_filled(
                        logo_rect,
                        CornerRadius::same(6),
                        Color32::from_rgb(22, 101, 52),
                    );
                    ui.painter().text(
                        logo_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "iT",
                        egui::FontId::proportional(17.0),
                        Color32::WHITE,
                    );
                    ui.add_space(6.0);

                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("ITool")
                                    .size(20.0)
                                    .strong()
                                    .color(Color32::from_rgb(17, 24, 39)),
                            );
                            ui.label(
                                RichText::new("—")
                                    .size(14.0)
                                    .color(Color32::from_rgb(156, 163, 175)),
                            );
                            ui.label(
                                RichText::new("iPhone Original Media Exporter")
                                    .size(13.0)
                                    .color(Color32::from_rgb(107, 114, 128)),
                            );
                        });
                    });

                    // Device connection indicator pill
                    ui.add_space(16.0);
                    let (status_text, status_color, bg_pill) = if self.scanning {
                        (
                            "Scanning iPhone…",
                            Color32::from_rgb(180, 83, 9),
                            Color32::from_rgb(254, 243, 199),
                        )
                    } else if !self.items.is_empty() {
                        (
                            "● Connected · iPhone Library Ready",
                            Color32::from_rgb(22, 101, 52),
                            Color32::from_rgb(220, 252, 231),
                        )
                    } else {
                        (
                            "○ iPhone: Connect via USB & Unlock",
                            Color32::from_rgb(107, 114, 128),
                            Color32::from_rgb(243, 244, 246),
                        )
                    };

                    let pill_btn = egui::Button::new(
                        RichText::new(status_text)
                            .size(12.0)
                            .color(status_color)
                            .strong(),
                    )
                    .fill(bg_pill)
                    .stroke(Stroke::new(1.0, status_color))
                    .corner_radius(CornerRadius::same(12));
                    ui.add(pill_btn);

                    // Right-aligned actions
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Scan iPhone button (Primary)
                        let scan_text = if self.scanning {
                            "Scanning…"
                        } else {
                            "Scan iPhone"
                        };
                        let scan_btn = egui::Button::new(
                            RichText::new(scan_text)
                                .size(13.5)
                                .color(Color32::WHITE)
                                .strong(),
                        )
                        .fill(if self.busy {
                            Color32::from_rgb(156, 163, 175)
                        } else {
                            Color32::from_rgb(22, 101, 52)
                        })
                        .min_size(Vec2::new(120.0, 32.0))
                        .corner_radius(CornerRadius::same(6));

                        if ui.add_enabled(!self.busy, scan_btn).clicked() {
                            self.scan();
                        }

                        ui.add_space(8.0);

                        // Select All / Deselect All
                        if !self.items.is_empty() {
                            let (all_selected, filtered_len) = {
                                let f = self.filtered_items();
                                let sel = !f.is_empty() && f.iter().all(|i| self.selected.contains(&i.path));
                                (sel, f.len())
                            };

                            let (btn_label, label_color) = if all_selected {
                                (
                                    format!("Deselect All ({filtered_len})"),
                                    Color32::from_rgb(185, 28, 28),
                                )
                            } else {
                                (
                                    format!("Select All ({filtered_len})"),
                                    Color32::from_rgb(22, 101, 52),
                                )
                            };

                            let sel_btn = egui::Button::new(
                                RichText::new(btn_label).size(12.5).color(label_color),
                            )
                            .fill(Color32::from_rgb(243, 244, 246))
                            .corner_radius(CornerRadius::same(6));

                            if ui.add_enabled(!self.busy, sel_btn).clicked() {
                                let paths: Vec<String> = self.filtered_items().iter().map(|i| i.path.clone()).collect();
                                if all_selected {
                                    for p in paths {
                                        self.selected.remove(&p);
                                    }
                                } else {
                                    for p in paths {
                                        self.selected.insert(p);
                                    }
                                }
                            }

                            ui.add_space(8.0);

                            // Toggle Sidebar Button
                            let sidebar_label = if self.show_sidebar {
                                "Sidebar ⏴"
                            } else {
                                "Sidebar ⏵"
                            };
                            let sidebar_btn = egui::Button::new(
                                RichText::new(sidebar_label)
                                    .size(12.5)
                                    .color(Color32::from_rgb(75, 85, 99)),
                            )
                            .fill(Color32::from_rgb(243, 244, 246))
                            .corner_radius(CornerRadius::same(6));

                            if ui.add(sidebar_btn).clicked() {
                                self.show_sidebar = !self.show_sidebar;
                            }
                        }
                    });
                });

                // Row 2: Search, Type Filters, Grouping, Sort Order
                if !self.items.is_empty() {
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(8.0);

                    ui.horizontal_wrapped(|ui| {
                        // Search box
                        ui.label(
                            RichText::new("🔍")
                                .size(13.0)
                                .color(Color32::from_rgb(107, 114, 128)),
                        );
                        let search_edit = egui::TextEdit::singleline(&mut self.search)
                            .hint_text("Search name, date, folder…")
                            .desired_width(190.0);
                        if ui.add(search_edit).changed() {
                            let _ = self.jobs.send(Job::CancelPreviews);
                        }

                        if !self.search.is_empty() && ui.button("✕").clicked() {
                            self.search.clear();
                            let _ = self.jobs.send(Job::CancelPreviews);
                        }

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(12.0);

                        // Media Type Segmented Controls
                        ui.label(
                            RichText::new("Type:")
                                .size(12.5)
                                .strong()
                                .color(Color32::from_rgb(55, 65, 81)),
                        );

                        let counts = {
                            let mut photos = 0;
                            let mut videos = 0;
                            let mut live = 0;
                            for item in &self.items {
                                if item.kind == "Video" {
                                    videos += 1;
                                } else if item.kind == "Photo + MOV" {
                                    live += 1;
                                    photos += 1;
                                } else {
                                    photos += 1;
                                }
                            }
                            (self.items.len(), photos, videos, live)
                        };

                        for (filter, label) in [
                            (MediaFilter::All, format!("All ({})", counts.0)),
                            (MediaFilter::Photos, format!("Photos ({})", counts.1)),
                            (MediaFilter::Videos, format!("Videos ({})", counts.2)),
                            (MediaFilter::LivePhotos, format!("Live ({})", counts.3)),
                        ] {
                            let active = self.media_filter == filter;
                            let btn = egui::Button::new(
                                RichText::new(label)
                                    .size(12.0)
                                    .color(if active {
                                        Color32::WHITE
                                    } else {
                                        Color32::from_rgb(55, 65, 81)
                                    })
                                    .strong(),
                            )
                            .fill(if active {
                                Color32::from_rgb(22, 101, 52)
                            } else {
                                Color32::from_rgb(243, 244, 246)
                            })
                            .corner_radius(CornerRadius::same(12));

                            if ui.add(btn).clicked() {
                                self.media_filter = filter;
                                let _ = self.jobs.send(Job::CancelPreviews);
                            }
                            ui.add_space(3.0);
                        }

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(12.0);

                        // Group By Controls
                        ui.label(
                            RichText::new("Group:")
                                .size(12.5)
                                .strong()
                                .color(Color32::from_rgb(55, 65, 81)),
                        );
                        for (mode, label) in [
                            (GroupBy::Month, "Month"),
                            (GroupBy::Week, "Week"),
                            (GroupBy::Date, "Date"),
                            (GroupBy::Continuous, "Continuous"),
                        ] {
                            let active = self.group_by == mode;
                            let btn = egui::Button::new(
                                RichText::new(label)
                                    .size(12.0)
                                    .color(if active {
                                        Color32::WHITE
                                    } else {
                                        Color32::from_rgb(55, 65, 81)
                                    })
                                    .strong(),
                            )
                            .fill(if active {
                                Color32::from_rgb(55, 65, 81)
                            } else {
                                Color32::from_rgb(243, 244, 246)
                            })
                            .corner_radius(CornerRadius::same(12));

                            if ui.add(btn).clicked() {
                                self.group_by = mode;
                                let _ = self.jobs.send(Job::CancelPreviews);
                            }
                            ui.add_space(3.0);
                        }

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(12.0);

                        // Sort toggle button
                        let sort_label = match self.sort_order {
                            SortOrder::NewestFirst => "Newest First ↓",
                            SortOrder::OldestFirst => "Oldest First ↑",
                        };
                        let sort_btn = egui::Button::new(
                            RichText::new(sort_label)
                                .size(12.0)
                                .color(Color32::from_rgb(55, 65, 81)),
                        )
                        .fill(Color32::from_rgb(243, 244, 246))
                        .corner_radius(CornerRadius::same(12));

                        if ui.add(sort_btn).clicked() {
                            self.sort_order = match self.sort_order {
                                SortOrder::NewestFirst => SortOrder::OldestFirst,
                                SortOrder::OldestFirst => SortOrder::NewestFirst,
                            };
                            let _ = self.jobs.send(Job::CancelPreviews);
                        }
                    });

                    // Active month banner
                    if let Some(ref month) = self.selected_month.clone() {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let pill = egui::Button::new(
                                RichText::new(format!("Showing only: {month}  ✕"))
                                    .size(12.0)
                                    .color(Color32::from_rgb(22, 101, 52))
                                    .strong(),
                            )
                            .fill(Color32::from_rgb(220, 252, 231))
                            .stroke(Stroke::new(1.0, Color32::from_rgb(22, 101, 52)))
                            .corner_radius(CornerRadius::same(12));

                            if ui.add(pill).clicked() {
                                self.selected_month = None;
                                let _ = self.jobs.send(Job::CancelPreviews);
                            }
                        });
                    }
                }
            });

        // ================= BOTTOM EXPORT DOCK =================
        egui::Panel::bottom("transfer_panel")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(255, 255, 255))
                    .inner_margin(Margin::symmetric(20, 12))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(229, 231, 235))),
            )
            .show(ui, |ui| {
                // Row 1: Destination folder picker
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("📁 Save to:")
                            .size(13.0)
                            .strong()
                            .color(Color32::from_rgb(55, 65, 81)),
                    );
                    let max_width = (ui.available_width() - 110.0).max(180.0);
                    ui.add_enabled(
                        !self.busy,
                        egui::TextEdit::singleline(&mut self.destination).desired_width(max_width),
                    );
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("Browse…"))
                        .clicked()
                        && let Some(p) = rfd::FileDialog::new().pick_folder()
                    {
                        self.destination = p.display().to_string();
                    }
                });

                ui.add_space(8.0);

                // Row 2: Export primary action and metrics
                ui.horizontal(|ui| {
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
                            "Export {} Original{} ({})",
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
                    .fill(if can_export {
                        Color32::from_rgb(22, 101, 52)
                    } else {
                        Color32::from_rgb(156, 163, 175)
                    })
                    .min_size(Vec2::new(220.0, 36.0))
                    .corner_radius(CornerRadius::same(6));

                    if ui.add_enabled(can_export, export_btn).clicked() {
                        self.cancel.store(false, Ordering::Relaxed);
                        self.busy = true;
                        self.error = None;
                        self.report = None;
                        self.done = 0;
                        self.total = 0;
                        self.current_file_progress.clear();
                        self.started = Some(Instant::now());

                        let paths: Vec<String> = self
                            .items
                            .iter()
                            .filter(|i| self.selected.contains(&i.path))
                            .flat_map(|i| i.resources.clone())
                            .collect();

                        let stamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis();
                        let dest = PathBuf::from(&self.destination).join(format!("ITool-{stamp}"));
                        self.status = "Preparing original media files for export…".into();
                        let _ = self.jobs.send(Job::Export(dest, paths));
                    }

                    // Cancel button during active export
                    if self.busy && !self.scanning {
                        ui.add_space(8.0);
                        let cancel_btn = egui::Button::new(
                            RichText::new("Cancel Export")
                                .size(13.0)
                                .color(Color32::from_rgb(185, 28, 28))
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
                        RichText::new("✓ Bit-for-bit originals · Embedded EXIF & Live Photos preserved · Never writes to phone")
                            .size(11.5)
                            .color(Color32::from_rgb(107, 114, 128)),
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
                                    .color(Color32::from_rgb(31, 41, 55)),
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
                                .color(Color32::from_rgb(75, 85, 99)),
                            );
                        }
                    });
                }

                // Status & Notifications
                ui.add_space(4.0);
                ui.label(
                    RichText::new(&self.status)
                        .size(12.0)
                        .color(Color32::from_rgb(31, 41, 55)),
                );

                if let Some(ref e) = self.error {
                    ui.add_space(2.0);
                    ui.colored_label(Color32::from_rgb(185, 28, 28), format!("Error: {e}"));
                }

                if let Some(ref report) = self.report {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("✓ Saved into: {}", report.display()))
                                .strong()
                                .size(12.5)
                                .color(Color32::from_rgb(22, 101, 52)),
                        );
                        if ui
                            .button(
                                RichText::new("Open folder in Windows Explorer")
                                    .size(12.0)
                                    .color(Color32::from_rgb(22, 101, 52)),
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
                        .fill(Color32::from_rgb(249, 250, 251))
                        .inner_margin(Margin::symmetric(12, 14))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(229, 231, 235))),
                )
                .min_size(230.0)
                .max_size(280.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Months Overview")
                                .size(14.5)
                                .strong()
                                .color(Color32::from_rgb(17, 24, 39)),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let total_bytes: u64 = self.items.iter().map(|i| i.size).sum();
                            ui.label(
                                RichText::new(format_size(total_bytes))
                                    .size(11.0)
                                    .color(Color32::from_rgb(107, 114, 128)),
                            );
                        });
                    });
                    ui.label(
                        RichText::new(format!("{} items total", self.items.len()))
                            .size(11.5)
                            .color(Color32::from_rgb(107, 114, 128)),
                    );

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            // "All Media" top row
                            let all_active = self.selected_month.is_none();
                            let (all_rect, all_resp) = ui
                                .allocate_exact_size(
                                    Vec2::new(ui.available_width(), 32.0),
                                    egui::Sense::click(),
                                );
                            if all_resp.clicked() {
                                self.selected_month = None;
                                let _ = self.jobs.send(Job::CancelPreviews);
                            }

                            let bg_color = if all_active {
                                Color32::from_rgb(22, 101, 52)
                            } else if all_resp.hovered() {
                                Color32::from_rgb(243, 244, 246)
                            } else {
                                Color32::TRANSPARENT
                            };
                            ui.painter().rect_filled(all_rect, CornerRadius::same(6), bg_color);

                            let text_color = if all_active {
                                Color32::WHITE
                            } else {
                                Color32::from_rgb(17, 24, 39)
                            };
                            let sub_color = if all_active {
                                Color32::from_rgb(220, 252, 231)
                            } else {
                                Color32::from_rgb(107, 114, 128)
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
                                let is_active = self.selected_month.as_deref() == Some(&month.label);
                                let (row_rect, row_resp) = ui
                                    .allocate_exact_size(
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
                                    Color32::from_rgb(22, 101, 52)
                                } else if row_resp.hovered() {
                                    Color32::from_rgb(243, 244, 246)
                                } else {
                                    Color32::TRANSPARENT
                                };
                                ui.painter().rect_filled(row_rect, CornerRadius::same(6), bg);

                                let label_col = if is_active {
                                    Color32::WHITE
                                } else {
                                    Color32::from_rgb(31, 41, 55)
                                };
                                let meta_col = if is_active {
                                    Color32::from_rgb(220, 252, 231)
                                } else {
                                    Color32::from_rgb(107, 114, 128)
                                };

                                ui.painter().text(
                                    row_rect.left_center() + Vec2::new(10.0, 0.0),
                                    egui::Align2::LEFT_CENTER,
                                    &month.label,
                                    egui::FontId::proportional(12.5),
                                    label_col,
                                );

                                let meta_str = format!("{} · {}", month.count, format_size(month.bytes));
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
            .frame(egui::Frame::new().fill(Color32::from_rgb(243, 244, 246)))
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
                            Color32::from_rgb(220, 252, 231),
                        );
                        ui.painter().text(
                            icon_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "📱",
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
                            .color(Color32::from_rgb(17, 24, 39)),
                        );
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("Connect by USB, unlock your phone, and tap 'Trust' if asked.")
                                .size(13.5)
                                .color(Color32::from_rgb(107, 114, 128)),
                        );
                        ui.label(
                            RichText::new("Click 'Scan iPhone' to view your photos and videos without modifying the phone.")
                                .size(13.5)
                                .color(Color32::from_rgb(107, 114, 128)),
                        );
                        ui.add_space(22.0);

                        if self.scanning {
                            ui.spinner();
                            ui.add_space(8.0);
                            ui.label(RichText::new(&self.status).size(13.0).color(Color32::from_rgb(55, 65, 81)));
                        } else {
                            let scan_btn = egui::Button::new(
                                RichText::new("Scan iPhone Now")
                                    .size(15.0)
                                    .color(Color32::WHITE)
                                    .strong(),
                            )
                            .fill(Color32::from_rgb(22, 101, 52))
                            .min_size(Vec2::new(180.0, 42.0))
                            .corner_radius(CornerRadius::same(8));

                            if ui.add_enabled(!self.busy, scan_btn).clicked() {
                                self.scan();
                            }
                        }
                    });
                    return;
                }

                let filtered = Self::get_filtered_items(
                    &self.items,
                    &self.search,
                    self.media_filter,
                    self.sort_order,
                    self.selected_month.as_deref(),
                );
                let available_width = (ui.available_width() - 16.0).max(200.0);
                let gap = 10.0;
                let target_size = 168.0;
                let columns = ((available_width + gap) / (target_size + gap))
                    .floor()
                    .max(1.0) as usize;
                let card_width =
                    ((available_width - gap * (columns as f32 - 1.0)) / columns as f32).floor();
                let card_height = card_width;

                if filtered.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(80.0);
                        ui.heading(
                            RichText::new("No media matches the current filter")
                                .size(18.0)
                                .color(Color32::from_rgb(55, 65, 81)),
                        );
                        ui.add_space(8.0);
                        if ui.button("Clear search and filters").clicked() {
                            self.search.clear();
                            self.media_filter = MediaFilter::All;
                            self.selected_month = None;
                        }
                    });
                    return;
                }

                // Map paths to global filtered indices for Shift-click range selection
                let item_indices: HashMap<String, usize> = filtered
                    .iter()
                    .enumerate()
                    .map(|(idx, item)| (item.path.clone(), idx))
                    .collect();

                let mut card_click: Option<(usize, bool)> = None; // (index, is_shift)
                let mut retry_preview: Option<String> = None;
                let mut group_toggle: Option<(Vec<String>, bool)> = None; // (paths, select_all)
                let mut needed_previews: Vec<String> = Vec::new();

                let is_continuous =
                    self.group_by == GroupBy::Continuous || self.selected_month.is_some();

                if is_continuous {
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
                                                &mut card_click,
                                                &mut retry_preview,
                                                self.busy,
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
                    let mut groups: Vec<(String, Vec<&MediaItem>)> = Vec::new();
                    let mut current_key = String::new();

                    for item in &filtered {
                        let key = match self.group_by {
                            GroupBy::Month => item.month_label.clone(),
                            GroupBy::Week => item.week_label.clone(),
                            GroupBy::Date => item.date_label.clone(),
                            GroupBy::Continuous => unreachable!(),
                        };
                        if key != current_key {
                            current_key = key.clone();
                            groups.push((key, Vec::new()));
                        }
                        groups.last_mut().unwrap().1.push(item);
                    }

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
                                            .color(Color32::from_rgb(17, 24, 39)),
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
                                        .color(Color32::from_rgb(107, 114, 128)),
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
                                            let btn = egui::Button::new(
                                                RichText::new(btn_text).size(11.5).color(
                                                    if all_selected {
                                                        Color32::from_rgb(185, 28, 28)
                                                    } else {
                                                        Color32::from_rgb(22, 101, 52)
                                                    },
                                                ),
                                            )
                                            .fill(Color32::from_rgb(255, 255, 255))
                                            .stroke(Stroke::new(
                                                1.0,
                                                Color32::from_rgb(229, 231, 235),
                                            ))
                                            .corner_radius(CornerRadius::same(4));

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
                                                    &mut card_click,
                                                    &mut retry_preview,
                                                    self.busy,
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

                // Handle single-click and Shift-click range selection
                if let Some((curr_idx, is_shift)) = card_click {
                    if is_shift && self.last_clicked_index.is_some() {
                        let prev_idx = self.last_clicked_index.unwrap();
                        let start = prev_idx.min(curr_idx);
                        let end = prev_idx.max(curr_idx);
                        let paths_to_add: Vec<String> = filtered[start..=end].iter().map(|i| i.path.clone()).collect();
                        for path in paths_to_add {
                            self.selected.insert(path);
                        }
                    } else {
                        self.last_clicked_index = Some(curr_idx);
                        let path = filtered[curr_idx].path.clone();
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
        card_click: &mut Option<(usize, bool)>,
        retry_preview: &mut Option<String>,
        is_busy: bool,
    ) {
        let tile_size = Vec2::new(card_width, card_height);
        let (rect, response) = ui.allocate_exact_size(tile_size, egui::Sense::click());
        let is_hovered = response.hovered();
        let is_clicked = response.clicked();

        // Handle click event with Shift modifier detection
        if is_clicked && !is_busy {
            let is_shift = ui.input(|i| i.modifiers.shift);
            *card_click = Some((index, is_shift));
        }

        // Full metadata tooltip on hover
        response.on_hover_ui(|ui| {
            ui.set_max_width(280.0);
            ui.label(
                RichText::new(&item.name)
                    .strong()
                    .size(13.0)
                    .color(Color32::from_rgb(17, 24, 39)),
            );
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Type:")
                        .color(Color32::from_rgb(107, 114, 128))
                        .size(11.5),
                );
                ui.label(
                    RichText::new(item.kind)
                        .color(Color32::from_rgb(31, 41, 55))
                        .size(11.5),
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Size:")
                        .color(Color32::from_rgb(107, 114, 128))
                        .size(11.5),
                );
                ui.label(
                    RichText::new(format_size(item.size))
                        .color(Color32::from_rgb(31, 41, 55))
                        .size(11.5),
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Date:")
                        .color(Color32::from_rgb(107, 114, 128))
                        .size(11.5),
                );
                ui.label(
                    RichText::new(&item.date_label)
                        .color(Color32::from_rgb(31, 41, 55))
                        .size(11.5),
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Path:")
                        .color(Color32::from_rgb(107, 114, 128))
                        .size(11.0),
                );
                ui.label(
                    RichText::new(&item.path)
                        .color(Color32::from_rgb(75, 85, 99))
                        .size(11.0),
                );
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("Click to select · Shift+Click for range")
                    .size(10.5)
                    .color(Color32::from_rgb(156, 163, 175)),
            );
        });

        // 1. Card Base Background & Stroke
        let bg_color = if is_selected {
            Color32::from_rgba_premultiplied(22, 101, 52, 22)
        } else {
            Color32::from_rgb(255, 255, 255)
        };
        ui.painter().rect_filled(rect, CornerRadius::same(6), bg_color);

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
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(6),
                    Color32::from_rgb(243, 244, 246),
                );
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Loading…",
                    egui::FontId::proportional(12.0),
                    Color32::from_rgb(156, 163, 175),
                );
            }
            ThumbnailStatus::Failed => {
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(6),
                    Color32::from_rgb(249, 250, 251),
                );
                let (icon, label) = if item.kind == "Video" {
                    ("🎬", "Video")
                } else if item.kind == "Photo + MOV" {
                    ("🔴", "Live Photo")
                } else {
                    ("📷", "Photo")
                };

                ui.painter().text(
                    rect.center() + Vec2::new(0.0, -10.0),
                    egui::Align2::CENTER_CENTER,
                    icon,
                    egui::FontId::proportional(22.0),
                    Color32::from_rgb(156, 163, 175),
                );

                let label_text = if is_hovered { "Click to retry" } else { label };
                ui.painter().text(
                    rect.center() + Vec2::new(0.0, 14.0),
                    egui::Align2::CENTER_CENTER,
                    label_text,
                    egui::FontId::proportional(11.0),
                    if is_hovered {
                        Color32::from_rgb(22, 101, 52)
                    } else {
                        Color32::from_rgb(156, 163, 175)
                    },
                );

                if is_clicked && is_hovered {
                    *retry_preview = Some(item.path.clone());
                }
            }
        }

        // 3. Selection border / Hover stroke
        let stroke = if is_selected {
            Stroke::new(2.5, Color32::from_rgb(22, 101, 52))
        } else if is_hovered {
            Stroke::new(1.5, Color32::from_rgb(156, 163, 175))
        } else {
            Stroke::new(1.0, Color32::from_rgb(229, 231, 235))
        };
        ui.painter()
            .rect_stroke(rect, CornerRadius::same(6), stroke, egui::StrokeKind::Inside);

        // 4. Media type badge in top-left
        if item.kind == "Video" || item.kind == "Photo + MOV" {
            let badge_text = if item.kind == "Video" {
                "▶ VIDEO"
            } else {
                "● LIVE"
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
            ui.painter()
                .circle_filled(check_center, 10.0, Color32::from_rgb(22, 101, 52));
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
        let overlay_height = 24.0;
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
            egui::pos2(overlay_rect.max.x - 6.0, overlay_rect.min.y + 5.0),
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
            .with_inner_size([1240.0, 860.0])
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
        assert_eq!(safe_truncate("very_long_photo_name_2026.heic", 12), "very_long_p…");
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
        let photos = Desktop::get_filtered_items(&items, "", MediaFilter::Photos, SortOrder::NewestFirst, None);
        assert_eq!(photos.len(), 2);
        assert_eq!(photos[0].name, "IMG_0003.HEIC");
        assert_eq!(photos[1].name, "IMG_0001.JPG");

        // Filter Videos
        let videos = Desktop::get_filtered_items(&items, "", MediaFilter::Videos, SortOrder::NewestFirst, None);
        assert_eq!(videos.len(), 1);
        assert_eq!(videos[0].name, "IMG_0002.MOV");

        // Filter Live Photos
        let live = Desktop::get_filtered_items(&items, "", MediaFilter::LivePhotos, SortOrder::NewestFirst, None);
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].name, "IMG_0003.HEIC");

        // Filter by Month
        let sept = Desktop::get_filtered_items(&items, "", MediaFilter::All, SortOrder::NewestFirst, Some("September 2026"));
        assert_eq!(sept.len(), 1);
        assert_eq!(sept[0].name, "IMG_0003.HEIC");

        // Search text
        let search = Desktop::get_filtered_items(&items, "0002", MediaFilter::All, SortOrder::NewestFirst, None);
        assert_eq!(search.len(), 1);
        assert_eq!(search[0].name, "IMG_0002.MOV");
    }

    #[test]
    fn test_responsive_grid_math() {
        for width in [800.0f32, 900.0, 1366.0, 1600.0, 1920.0, 2560.0, 3840.0] {
            let available_width = (width - 16.0).max(200.0);
            let gap = 10.0;
            let target_size = 168.0;
            let columns = ((available_width + gap) / (target_size + gap)).floor().max(1.0) as usize;
            assert!(columns >= 1);
            let card_width = ((available_width - gap * (columns as f32 - 1.0)) / columns as f32).floor();
            assert!(card_width >= 100.0);
            assert!(card_width <= 250.0);
        }
    }
}
