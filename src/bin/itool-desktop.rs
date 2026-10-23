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

enum Job {
    Scan,
    Preview(String),
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

struct Desktop {
    jobs: mpsc::Sender<Job>,
    events: mpsc::Receiver<Event>,
    cancel: Arc<AtomicBool>,
    items: Vec<MediaItem>,
    selected: HashSet<String>,
    textures: HashMap<String, egui::TextureHandle>,
    texture_lru: VecDeque<String>,
    requested: HashSet<String>,

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
            let runtime = tokio::runtime::Runtime::new().expect("Cannot start device worker");
            let mut preview_client = None;
            let mut pending = std::collections::VecDeque::new();
            loop {
                if pending.is_empty() {
                    match rx.recv() {
                        Ok(job) => pending.push_back(job),
                        Err(_) => break,
                    }
                }
                pending.extend(rx.try_iter());
                let index = pending
                    .iter()
                    .position(|j| !matches!(j, Job::Preview(_)))
                    .unwrap_or(0);
                let job = pending.remove(index).unwrap();
                if !matches!(job, Job::Preview(_)) {
                    pending.retain(|j| !matches!(j, Job::Preview(_)));
                }
                match job {
                    Job::Scan => {
                        preview_client = None;
                        let result = runtime.block_on(itool::scan_media(|s| {
                            let _ = tx.send(Event::Status(s));
                            ctx.request_repaint();
                        }));
                        let _ = tx.send(match result {
                            Ok(items) => Event::Scanned(items),
                            Err(e) => Event::Error(format!("{e:#}")),
                        });
                    }
                    Job::Preview(path) => {
                        let result = runtime.block_on(async {
                            if preview_client.is_none() {
                                preview_client = Some(itool::afc().await?);
                            }
                            itool::read_thumbnail(preview_client.as_mut().unwrap(), &path).await
                        });
                        if result.is_err() {
                            preview_client = None;
                        }
                        let image = result
                            .ok()
                            .and_then(|bytes| {
                                image::load_from_memory_with_format(
                                    &bytes,
                                    image::ImageFormat::Jpeg,
                                )
                                .ok()
                            })
                            .map(|image| {
                                let rgba = image.thumbnail(320, 240).to_rgba8();
                                egui::ColorImage::from_rgba_unmultiplied(
                                    [rgba.width() as usize, rgba.height() as usize],
                                    rgba.as_raw(),
                                )
                            });
                        let _ = tx.send(Event::Preview(path, image));
                    }
                    Job::Export(dest, paths) => {
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
                    }
                }
                ctx.request_repaint();
            }
        });

        // Configure visuals for clean, distinctive, readable light design
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let mut style = (*cc.egui_ctx.style_of(egui::Theme::Light)).clone();
        style.spacing.item_spacing = Vec2::new(10.0, 10.0);
        style.spacing.button_padding = Vec2::new(14.0, 8.0);
        style.visuals.selection.bg_fill = Color32::from_rgb(26, 105, 76);
        style.visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        cc.egui_ctx.set_style_of(egui::Theme::Light, style);

        Self {
            jobs,
            events,
            cancel,
            items: vec![],
            selected: HashSet::new(),
            textures: HashMap::new(),
            texture_lru: VecDeque::new(),
            requested: HashSet::new(),

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
            status: "Connect your iPhone by USB, unlock it, then click Scan.".into(),
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
        self.status = "Connecting to iPhone…".into();
        self.textures.clear();
        self.texture_lru.clear();
        self.requested.clear();
        self.selected.clear();
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

        // Ordered from newest month to oldest
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
        if self.requested.contains(&path) {
            let texture = ctx.load_texture(&path, image, egui::TextureOptions::LINEAR);
            self.textures.insert(path.clone(), texture);
            self.texture_lru.push_back(path);

            // Bounded memory cache: keep at most 300 textures in memory
            while self.texture_lru.len() > 300 {
                if let Some(old_path) = self.texture_lru.pop_front() {
                    self.textures.remove(&old_path);
                    self.requested.remove(&old_path);
                }
            }
        }
    }

    fn request_preview_if_needed(&mut self, path: &str) {
        if !self.busy && self.requested.insert(path.to_owned()) {
            let _ = self.jobs.send(Job::Preview(path.to_owned()));
        }
    }

    fn filtered_items(&self) -> Vec<&MediaItem> {
        let needle = self.search.trim().to_lowercase();
        let mut list: Vec<&MediaItem> = self
            .items
            .iter()
            .filter(|item| {
                // Month filter from Month Overview sidebar
                if let Some(ref m) = self.selected_month {
                    if &item.month_label != m {
                        return false;
                    }
                }
                // Media Type filter
                match self.media_filter {
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
                        || item.month_label.to_lowercase().contains(&needle);
                    if !matches_name && !matches_path && !matches_date {
                        return false;
                    }
                }
                true
            })
            .collect();

        // Sort order
        match self.sort_order {
            SortOrder::NewestFirst => {
                list.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| b.path.cmp(&a.path)));
            }
            SortOrder::OldestFirst => {
                list.sort_by(|a, b| a.timestamp.cmp(&b.timestamp).then_with(|| a.path.cmp(&b.path)));
            }
        }
        list
    }
}

impl eframe::App for Desktop {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Process incoming events
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
                Event::Preview(_, None) => {}
                Event::Progress(done, total, name) => {
                    self.done = done;
                    self.total = total;
                    self.status = format!("Copying {name}");
                }
                Event::Finished(report) => {
                    self.busy = false;
                    self.started = None;
                    self.status = "Export complete. All files passed disk verification.".into();
                    self.report = Some(report);
                }
                Event::Error(e) => {
                    self.busy = false;
                    self.scanning = false;
                    self.started = None;
                    self.error = Some(e);
                    self.status = "Operation stopped. Completed files are preserved.".into();
                }
            }
        }

        // ================= TOP TOOLBAR =================
        egui::Panel::top("header_panel")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(255, 255, 255))
                    .inner_margin(Margin::symmetric(20, 12))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(229, 231, 235))),
            )
            .show(ui, |ui| {
                // Row 1: Brand title, connection status pill, and primary action buttons
                ui.horizontal(|ui| {
                    ui.label(RichText::new("ITool").size(24.0).strong().color(Color32::from_rgb(17, 24, 39)));
                    ui.add_space(8.0);
                    ui.label(RichText::new("iPhone Original Media Exporter").size(13.0).color(Color32::from_rgb(107, 114, 128)));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Scan iPhone button (Primary)
                        let scan_btn = egui::Button::new(
                            RichText::new(if self.scanning { "Scanning…" } else { "Scan iPhone" })
                                .size(14.0)
                                .color(Color32::WHITE)
                                .strong(),
                        )
                        .fill(if self.busy {
                            Color32::from_rgb(156, 163, 175)
                        } else {
                            Color32::from_rgb(26, 105, 76)
                        })
                        .corner_radius(CornerRadius::same(6));

                        if ui.add_enabled(!self.busy, scan_btn).clicked() {
                            self.scan();
                        }

                        ui.add_space(12.0);

                        // Clear selection button
                        if !self.selected.is_empty() {
                            let clear_btn = egui::Button::new(
                                RichText::new("Clear selection")
                                    .size(13.0)
                                    .color(Color32::from_rgb(75, 85, 99)),
                            )
                            .corner_radius(CornerRadius::same(6));
                            if ui.add_enabled(!self.busy, clear_btn).clicked() {
                                self.selected.clear();
                            }
                            ui.add_space(8.0);
                        }

                        // Select All button
                        if !self.items.is_empty() {
                            let select_all_btn = egui::Button::new(
                                RichText::new(format!("Select all ({})", self.items.len()))
                                    .size(13.0)
                                    .color(Color32::from_rgb(26, 105, 76)),
                            )
                            .corner_radius(CornerRadius::same(6));
                            if ui.add_enabled(!self.busy, select_all_btn).clicked() {
                                for item in &self.items {
                                    self.selected.insert(item.path.clone());
                                }
                            }
                            ui.add_space(8.0);
                        }

                        // Toggle Month Overview sidebar button
                        if !self.items.is_empty() {
                            let sidebar_text = if self.show_sidebar {
                                "Hide Months Overview"
                            } else {
                                "Show Months Overview"
                            };
                            if ui
                                .button(RichText::new(sidebar_text).size(13.0).color(Color32::from_rgb(75, 85, 99)))
                                .clicked()
                            {
                                self.show_sidebar = !self.show_sidebar;
                            }
                        }
                    });
                });

                if !self.items.is_empty() {
                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Row 2: Search, Type Filters, Sort Order, Grouping
                    ui.horizontal(|ui| {
                        // Search box
                        ui.label(RichText::new("Find:").strong().color(Color32::from_rgb(55, 65, 81)));
                        let search_edit = egui::TextEdit::singleline(&mut self.search)
                            .hint_text("Search filename, folder, or date…")
                            .desired_width(220.0);
                        if ui.add(search_edit).changed() {
                            self.textures.clear();
                            self.texture_lru.clear();
                            self.requested.clear();
                        }
                        if !self.search.is_empty() && ui.button("✕").clicked() {
                            self.search.clear();
                        }

                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(16.0);

                        // Media Type Segmented Pills
                        ui.label(RichText::new("Type:").strong().color(Color32::from_rgb(55, 65, 81)));
                        for (filter, label) in [
                            (MediaFilter::All, "All"),
                            (MediaFilter::Photos, "Photos"),
                            (MediaFilter::Videos, "Videos"),
                            (MediaFilter::LivePhotos, "Live Photos"),
                        ] {
                            let active = self.media_filter == filter;
                            let btn = egui::Button::new(
                                RichText::new(label)
                                    .size(12.5)
                                    .color(if active { Color32::WHITE } else { Color32::from_rgb(55, 65, 81) })
                                    .strong(),
                            )
                            .fill(if active {
                                Color32::from_rgb(26, 105, 76)
                            } else {
                                Color32::from_rgb(243, 244, 246)
                            })
                            .corner_radius(CornerRadius::same(14));

                            if ui.add(btn).clicked() {
                                self.media_filter = filter;
                            }
                            ui.add_space(4.0);
                        }

                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(16.0);

                        // Group by selector
                        ui.label(RichText::new("Group:").strong().color(Color32::from_rgb(55, 65, 81)));
                        for (mode, label) in [
                            (GroupBy::Month, "Month"),
                            (GroupBy::Week, "Week"),
                            (GroupBy::Date, "Date"),
                            (GroupBy::Continuous, "Continuous"),
                        ] {
                            let active = self.group_by == mode;
                            let btn = egui::Button::new(
                                RichText::new(label)
                                    .size(12.5)
                                    .color(if active { Color32::WHITE } else { Color32::from_rgb(55, 65, 81) }),
                            )
                            .fill(if active {
                                Color32::from_rgb(55, 65, 81)
                            } else {
                                Color32::from_rgb(243, 244, 246)
                            })
                            .corner_radius(CornerRadius::same(14));

                            if ui.add(btn).clicked() {
                                self.group_by = mode;
                            }
                            ui.add_space(4.0);
                        }

                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(16.0);

                        // Sort toggle button
                        let sort_label = match self.sort_order {
                            SortOrder::NewestFirst => "Newest First ↓",
                            SortOrder::OldestFirst => "Oldest First ↑",
                        };
                        if ui
                            .button(RichText::new(sort_label).size(12.5).color(Color32::from_rgb(55, 65, 81)))
                            .clicked()
                        {
                            self.sort_order = match self.sort_order {
                                SortOrder::NewestFirst => SortOrder::OldestFirst,
                                SortOrder::OldestFirst => SortOrder::NewestFirst,
                            };
                        }
                    });

                    // Active Month Filter Banner
                    let mut clear_month = false;
                    if let Some(ref month) = self.selected_month {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let badge = format!("Showing only: {month}");
                            ui.label(
                                RichText::new(badge)
                                    .size(13.0)
                                    .color(Color32::from_rgb(26, 105, 76))
                                    .strong(),
                            );
                            if ui
                                .button(
                                    RichText::new("Show all months ✕")
                                        .size(12.0)
                                        .color(Color32::from_rgb(185, 28, 28)),
                                )
                                .clicked()
                            {
                                clear_month = true;
                            }
                        });
                    }
                    if clear_month {
                        self.selected_month = None;
                    }
                }
            });

        // ================= BOTTOM TRANSFER DOCK =================
        egui::Panel::bottom("transfer_panel")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(255, 255, 255))
                    .inner_margin(Margin::symmetric(20, 14))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(229, 231, 235))),
            )
            .show(ui, |ui| {
                // Destination folder selection row
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Save into:").strong().color(Color32::from_rgb(55, 65, 81)));
                    ui.add_enabled(
                        !self.busy,
                        egui::TextEdit::singleline(&mut self.destination)
                            .desired_width((ui.available_width() - 170.0).max(180.0)),
                    );
                    if ui.add_enabled(!self.busy, egui::Button::new("Choose folder…")).clicked()
                        && let Some(p) = rfd::FileDialog::new().pick_folder()
                    {
                        self.destination = p.display().to_string();
                    }
                });

                ui.add_space(10.0);

                // Export action row
                ui.horizontal(|ui| {
                    let selected_count = self.selected.len();
                    let selected_bytes: u64 = self
                        .items
                        .iter()
                        .filter(|i| self.selected.contains(&i.path))
                        .map(|i| i.size)
                        .sum();

                    let export_text = if selected_count > 0 {
                        format!(
                            "Export {} original{} ({:.1} MB)",
                            selected_count,
                            if selected_count == 1 { "" } else { "s" },
                            selected_bytes as f64 / 1_048_576.0
                        )
                    } else {
                        "Select photos to export".to_string()
                    };

                    let can_export = !self.busy && selected_count > 0 && !self.destination.is_empty();
                    let export_btn = egui::Button::new(
                        RichText::new(export_text)
                            .size(14.0)
                            .color(Color32::WHITE)
                            .strong(),
                    )
                    .fill(if can_export {
                        Color32::from_rgb(26, 105, 76)
                    } else {
                        Color32::from_rgb(156, 163, 175)
                    })
                    .min_size(Vec2::new(200.0, 36.0))
                    .corner_radius(CornerRadius::same(6));

                    if ui.add_enabled(can_export, export_btn).clicked() {
                        self.cancel.store(false, Ordering::Relaxed);
                        self.busy = true;
                        self.error = None;
                        self.report = None;
                        self.done = 0;
                        self.total = 0;
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
                        self.status = "Preparing original files for export…".into();
                        let _ = self.jobs.send(Job::Export(dest, paths));
                    }

                    if self.busy && !self.scanning {
                        ui.add_space(10.0);
                        let cancel_btn = egui::Button::new(
                            RichText::new("Cancel export")
                                .size(13.0)
                                .color(Color32::from_rgb(185, 28, 28)),
                        )
                        .corner_radius(CornerRadius::same(6));
                        if ui.add(cancel_btn).clicked() {
                            self.cancel.store(true, Ordering::Relaxed);
                            self.status = "Cancelling after current file finishes…".into();
                        }
                    }

                    ui.add_space(16.0);
                    ui.label(
                        RichText::new("Bit-for-bit originals: HEIC preserved, Live Photo MOV paired, embedded EXIF intact.")
                            .size(12.0)
                            .color(Color32::from_rgb(107, 114, 128)),
                    );
                });

                // Active progress display
                if self.busy && !self.scanning {
                    ui.add_space(8.0);
                    let ratio = if self.total > 0 {
                        self.done as f32 / self.total as f32
                    } else {
                        0.0
                    };
                    ui.add(egui::ProgressBar::new(ratio).show_percentage());
                    if let Some(start) = self.started {
                        let elapsed = start.elapsed().as_secs_f64();
                        let speed = (self.done as f64 / 1_048_576.0) / elapsed.max(0.001);
                        ui.label(
                            RichText::new(format!(
                                "{:.1} / {:.1} MiB · {:.2} MiB/s effective throughput",
                                self.done as f64 / 1_048_576.0,
                                self.total as f64 / 1_048_576.0,
                                speed
                            ))
                            .size(12.0)
                            .color(Color32::from_rgb(75, 85, 99)),
                        );
                    }
                }

                ui.add_space(6.0);
                ui.label(RichText::new(&self.status).size(12.5).color(Color32::from_rgb(31, 41, 55)));

                if let Some(ref e) = self.error {
                    ui.colored_label(Color32::from_rgb(185, 28, 28), e);
                }

                if let Some(ref report) = self.report {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("Saved into: {}", report.display()))
                                .strong()
                                .color(Color32::from_rgb(26, 105, 76)),
                        );
                        if ui.button("Open folder in Explorer").clicked() {
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
                        .inner_margin(Margin::symmetric(14, 16))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(229, 231, 235))),
                )
                .min_size(220.0)
                .max_size(260.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("Months Overview").size(15.0).strong().color(Color32::from_rgb(17, 24, 39)));
                    ui.label(
                        RichText::new(format!("{} items total", self.items.len()))
                            .size(12.0)
                            .color(Color32::from_rgb(107, 114, 128)),
                    );
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(8.0);

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        // "All Photos" row
                        let all_active = self.selected_month.is_none();
                        let all_btn = egui::Button::new(
                            RichText::new(format!("All Months ({})", self.items.len()))
                                .size(13.0)
                                .color(if all_active { Color32::WHITE } else { Color32::from_rgb(17, 24, 39) })
                                .strong(),
                        )
                        .fill(if all_active {
                            Color32::from_rgb(26, 105, 76)
                        } else {
                            Color32::TRANSPARENT
                        })
                        .corner_radius(CornerRadius::same(6))
                        .min_size(Vec2::new(ui.available_width(), 32.0));

                        if ui.add(all_btn).clicked() {
                            self.selected_month = None;
                        }

                        ui.add_space(6.0);

                        // Individual month rows
                        for month in &self.months {
                            let is_active = self.selected_month.as_deref() == Some(&month.label);
                            let text = format!(
                                "{} ({} · {:.1} MB)",
                                month.label,
                                month.count,
                                month.bytes as f64 / 1_048_576.0
                            );
                            let btn = egui::Button::new(
                                RichText::new(text)
                                    .size(12.5)
                                    .color(if is_active { Color32::WHITE } else { Color32::from_rgb(31, 41, 55) })
                                    .strong(),
                            )
                            .fill(if is_active {
                                Color32::from_rgb(26, 105, 76)
                            } else {
                                Color32::TRANSPARENT
                            })
                            .corner_radius(CornerRadius::same(6))
                            .min_size(Vec2::new(ui.available_width(), 30.0));

                            if ui.add(btn).clicked() {
                                if is_active {
                                    self.selected_month = None;
                                } else {
                                    self.selected_month = Some(month.label.clone());
                                }
                            }
                            ui.add_space(2.0);
                        }
                    });
                });
        }

        // ================= MAIN MEDIA AREA =================
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::from_rgb(243, 244, 246)))
            .show(ui, |ui| {
                if self.items.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(110.0);
                        ui.heading(
                            RichText::new(if self.scanning {
                                "Cataloging your iPhone media…"
                            } else {
                                "Ready to export your iPhone media"
                            })
                            .size(24.0)
                            .strong()
                            .color(Color32::from_rgb(17, 24, 39)),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Connect by USB, unlock your phone, and tap Trust if asked.")
                                .size(14.0)
                                .color(Color32::from_rgb(107, 114, 128)),
                        );
                        ui.label(
                            RichText::new("Click 'Scan iPhone' to catalog photos and videos without modifying the device.")
                                .size(14.0)
                                .color(Color32::from_rgb(107, 114, 128)),
                        );
                        ui.add_space(24.0);

                        if self.scanning {
                            ui.spinner();
                        } else {
                            let scan_big = egui::Button::new(
                                RichText::new("Scan iPhone Now")
                                    .size(15.0)
                                    .color(Color32::WHITE)
                                    .strong(),
                            )
                            .fill(Color32::from_rgb(26, 105, 76))
                            .min_size(Vec2::new(180.0, 42.0))
                            .corner_radius(CornerRadius::same(8));

                            if ui.add_enabled(!self.busy, scan_big).clicked() {
                                self.scan();
                            }
                        }
                    });
                    return;
                }

                let filtered = self.filtered_items();
                let available_width = ui.available_width() - 16.0;
                let gap = 4.0;
                // Target ~160px per tile, but stretch to fill the row exactly
                let columns = ((available_width + gap) / (160.0 + gap)).floor().max(1.0) as usize;
                let card_width = (available_width - gap * (columns as f32 - 1.0)) / columns as f32;
                let card_height = card_width; // square tiles

                if filtered.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(80.0);
                        ui.heading(RichText::new("No media matches the current filter").size(18.0));
                        ui.add_space(8.0);
                        if ui.button("Clear search and filters").clicked() {
                            self.search.clear();
                            self.media_filter = MediaFilter::All;
                            self.selected_month = None;
                        }
                    });
                    return;
                }

                let mut toggled_paths = Vec::new();
                let mut group_select = Vec::new();
                let mut group_deselect = Vec::new();
                let mut to_preview = Vec::new();

                // If Continuous mode OR filtered to a single month: render single virtualized grid
                let is_continuous = self.group_by == GroupBy::Continuous || self.selected_month.is_some();

                if is_continuous {
                    let total_rows = filtered.len().div_ceil(columns);
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show_rows(ui, card_height + gap, total_rows, |ui, row_range| {
                            for row_idx in row_range {
                                let start = row_idx * columns;
                                let end = (start + columns).min(filtered.len());
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = gap;
                                    for item in &filtered[start..end] {
                                        Self::render_media_card(
                                            ui,
                                            item,
                                            card_width,
                                            card_height,
                                            self.selected.contains(&item.path),
                                            self.textures.get(&item.path),
                                            self.requested.contains(&item.path),
                                            &mut toggled_paths,
                                            self.busy,
                                        );
                                        if !self.requested.contains(&item.path) {
                                            to_preview.push(item.path.clone());
                                        }
                                    }
                                });
                                ui.add_space(gap);
                            }
                        });
                } else {
                    // Grouped rendering (Month, Week, or Date)
                    let mut groups: Vec<(String, Vec<&MediaItem>)> = Vec::new();
                    let mut current_key = String::new();

                    for item in filtered {
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
                                            .size(17.0)
                                            .strong()
                                            .color(Color32::from_rgb(17, 24, 39)),
                                    );
                                    let group_bytes: u64 = group_items.iter().map(|i| i.size).sum();
                                    ui.label(
                                        RichText::new(format!(
                                            "· {} item{} ({:.1} MB)",
                                            group_items.len(),
                                            if group_items.len() == 1 { "" } else { "s" },
                                            group_bytes as f64 / 1_048_576.0
                                        ))
                                        .size(12.5)
                                        .color(Color32::from_rgb(107, 114, 128)),
                                    );

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let all_selected = group_items.iter().all(|i| self.selected.contains(&i.path));
                                        let btn_text = if all_selected { "Deselect group" } else { "Select group" };
                                        let btn = egui::Button::new(
                                            RichText::new(btn_text)
                                                .size(12.0)
                                                .color(if all_selected {
                                                    Color32::from_rgb(185, 28, 28)
                                                } else {
                                                    Color32::from_rgb(26, 105, 76)
                                                }),
                                        )
                                        .corner_radius(CornerRadius::same(4));

                                        if ui.add_enabled(!self.busy, btn).clicked() {
                                            if all_selected {
                                                for item in group_items {
                                                    group_deselect.push(item.path.clone());
                                                }
                                            } else {
                                                for item in group_items {
                                                    group_select.push(item.path.clone());
                                                }
                                            }
                                        }
                                    });
                                });

                                ui.add_space(4.0);
                                ui.separator();
                                ui.add_space(6.0);

                                // Group Rows
                                let total_group_rows = group_items.len().div_ceil(columns);
                                for row_idx in 0..total_group_rows {
                                    let start = row_idx * columns;
                                    let end = (start + columns).min(group_items.len());
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = gap;
                                        for item in &group_items[start..end] {
                                            Self::render_media_card(
                                                ui,
                                                item,
                                                card_width,
                                                card_height,
                                                self.selected.contains(&item.path),
                                                self.textures.get(&item.path),
                                                self.requested.contains(&item.path),
                                                &mut toggled_paths,
                                                self.busy,
                                            );
                                            if !self.requested.contains(&item.path) {
                                                to_preview.push(item.path.clone());
                                            }
                                        }
                                    });
                                    ui.add_space(gap);
                                }
                                ui.add_space(14.0);
                            }
                        });
                }

                for path in toggled_paths {
                    if !self.selected.remove(&path) {
                        self.selected.insert(path);
                    }
                }
                for path in group_select {
                    self.selected.insert(path);
                }
                for path in group_deselect {
                    self.selected.remove(&path);
                }
                for path in to_preview {
                    self.request_preview_if_needed(&path);
                }
            });

        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }
    }
}

impl Desktop {
    #[allow(clippy::too_many_arguments)]
    fn render_media_card(
        ui: &mut egui::Ui,
        item: &MediaItem,
        card_width: f32,
        card_height: f32,
        is_selected: bool,
        texture: Option<&egui::TextureHandle>,
        is_requested: bool,
        toggled_paths: &mut Vec<String>,
        is_busy: bool,
    ) {
        let tile_size = Vec2::new(card_width, card_height);

        // Allocate a fixed rect for this tile
        let (rect, _response) = ui.allocate_exact_size(tile_size, egui::Sense::hover());

        // Selection highlight: draw a border around the tile
        if is_selected {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(4),
                Stroke::new(3.0, Color32::from_rgb(26, 105, 76)),
                egui::StrokeKind::Inside,
            );
        }

        // Thumbnail fills the entire tile
        if let Some(tex) = texture {
            let img_resp = ui.put(
                rect,
                egui::Image::new(tex)
                    .fit_to_exact_size(tile_size)
                    .corner_radius(CornerRadius::same(4))
                    .sense(egui::Sense::click()),
            );
            if img_resp.clicked() && !is_busy {
                toggled_paths.push(item.path.clone());
            }
        } else {
            // Placeholder background
            ui.painter().rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(234, 236, 240));
            let placeholder_text = if is_requested { "Loading…" } else { "" };
            if !placeholder_text.is_empty() {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    placeholder_text,
                    egui::FontId::proportional(11.0),
                    Color32::from_rgb(156, 163, 175),
                );
            }
            // Make the whole placeholder clickable
            let click_resp = ui.interact(rect, ui.scope_id().with(&item.path), egui::Sense::click());
            if click_resp.clicked() && !is_busy {
                toggled_paths.push(item.path.clone());
            }
        }

        // Semi-transparent overlay bar at the bottom with filename + type badge
        let overlay_height = 22.0;
        let overlay_rect = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, rect.max.y - overlay_height),
            egui::vec2(rect.width(), overlay_height),
        );
        ui.painter().rect_filled(
            overlay_rect,
            CornerRadius {
                nw: 0, ne: 0,
                sw: 4, se: 4,
            },
            Color32::from_rgba_premultiplied(0, 0, 0, 160),
        );

        // Checkbox in the top-left corner with a dark backing circle for visibility
        let checkbox_center = rect.min + egui::vec2(14.0, 14.0);
        ui.painter().circle_filled(checkbox_center, 11.0, Color32::from_rgba_premultiplied(0, 0, 0, 120));
        let checkbox_rect = egui::Rect::from_center_size(checkbox_center, egui::vec2(20.0, 20.0));
        let mut checked = is_selected;
        let checkbox_resp = ui.put(
            checkbox_rect,
            egui::Checkbox::new(&mut checked, ""),
        );
        if checkbox_resp.changed() && !is_busy {
            toggled_paths.push(item.path.clone());
        }

        // Filename text in overlay — left aligned
        let short_name = if item.name.len() > 18 {
            format!("{}…", &item.name[..16])
        } else {
            item.name.clone()
        };
        let text_y = overlay_rect.min.y + 4.0;
        ui.painter().text(
            egui::pos2(overlay_rect.min.x + 5.0, text_y),
            egui::Align2::LEFT_TOP,
            &short_name,
            egui::FontId::proportional(10.0),
            Color32::WHITE,
        );

        // Size on the right side of overlay
        let size_text = format!("{:.1}MB", item.size as f64 / 1_048_576.0);
        ui.painter().text(
            egui::pos2(overlay_rect.max.x - 5.0, text_y),
            egui::Align2::RIGHT_TOP,
            &size_text,
            egui::FontId::proportional(9.5),
            Color32::from_rgb(190, 190, 190),
        );
    }
}

fn main() -> eframe::Result {
    eprintln!("=== itool-desktop main starting ===");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1240.0, 860.0])
            .with_min_inner_size([880.0, 620.0])
            .with_title("ITool — iPhone Original Media Exporter"),
        ..Default::default()
    };
    let res = eframe::run_native(
        "ITool — iPhone originals",
        options,
        Box::new(|cc| {
            eprintln!("=== itool-desktop calling Desktop::new ===");
            Ok(Box::new(Desktop::new(cc)))
        }),
    );
    eprintln!("=== itool-desktop run_native finished: {res:?} ===");
    res
}
