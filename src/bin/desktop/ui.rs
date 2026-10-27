use super::*;
mod chrome;
mod gallery;
use chrome::{Icon, bold, draw_icon, icon_button, nav, step};
use chrome::{menu, tree_row};

fn primary(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(bold(text.into(), 14.0).color(Color32::WHITE))
        .fill(ACCENT)
        .corner_radius(6)
        .min_size(Vec2::new(176.0, 48.0))
}

fn logo(ui: &mut egui::Ui, texture: &egui::TextureHandle, size: f32) {
    ui.add(egui::Image::new(texture).fit_to_exact_size(Vec2::splat(size)));
}

impl Desktop {
    pub(super) fn guided_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let p = Palette::new(self.dark_mode);
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Status(status) => self.status = status,
                Event::PreviewReset => self
                    .thumbnail_status
                    .retain(|_, v| *v != ThumbnailStatus::Loading),
                Event::Scanned(snapshot) => {
                    self.items = snapshot.items;
                    self.metadata = snapshot.assets;
                    self.metadata_status = snapshot.metadata_status;
                    self.albums_available = snapshot.albums_available;
                    self.favorites_available = snapshot.favorites_available;
                    self.options.expected_device = Some(snapshot.device_key);
                    self.status = format!(
                        "{} items in this scan · keep your iPhone connected",
                        self.items.len()
                    );
                    self.busy = false;
                    self.scanning = false;
                    self.update_months_index();
                    self.screen = Screen::Library;
                }
                Event::Preview(path, Some(image)) => self.add_texture(&ctx, path, image),
                Event::Preview(path, None) => {
                    self.thumbnail_status.insert(path, ThumbnailStatus::Failed);
                }
                Event::Progress(done, total, detail) => {
                    self.done = done;
                    self.total = total;
                    self.current_file_progress = detail;
                    self.status = "Saving your media…".into();
                }
                Event::Finished(outcome) => {
                    self.busy = false;
                    self.started = None;
                    self.status = if outcome.warnings.is_empty() {
                        "Your export is complete."
                    } else {
                        "Originals saved. Some optional steps need attention."
                    }
                    .into();
                    self.report = Some(outcome.folder.clone());
                    self.outcome = Some(outcome);
                    self.current_file_progress.clear();
                    self.screen = Screen::Results;
                }
                Event::Error(error) => {
                    self.busy = false;
                    self.started = None;
                    self.error = Some(error);
                    if self.scanning {
                        self.screen = Screen::Connect;
                    }
                    self.scanning = false;
                    self.status = "Operation stopped. Any completed files are preserved.".into();
                }
            }
        }
        if self.screen == Screen::Library && !self.busy && !ctx.egui_wants_keyboard_input() {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::A)) {
                let paths: Vec<_> = self
                    .filtered_items()
                    .into_iter()
                    .map(|i| i.path.clone())
                    .collect();
                self.selected.extend(paths);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.selected.clear();
                self.selected_only = false;
            }
        }
        egui::Panel::top("guided_header")
            .exact_size(60.)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .inner_margin(Margin::symmetric(20, 10)),
            )
            .show(ui, |ui| {
                let full = ui.max_rect();
                let drag = ui.interact(
                    full,
                    ui.scope_id().with("window_drag"),
                    egui::Sense::click_and_drag(),
                );
                if drag.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                }
                ui.horizontal(|ui| {
                    logo(ui, &self.logo, 28.);
                    ui.label(bold("ITool", 22.).color(p.text));
                });
                let nav_x = (full.center().x - 266.).max(full.left() + 130.);
                let nav_rect =
                    egui::Rect::from_min_size(egui::pos2(nav_x, full.top()), Vec2::new(450., 40.));
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(nav_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 0.;
                        for (n, label, screen) in [
                            (1, "Connect", Screen::Connect),
                            (2, "Choose media", Screen::Library),
                            (3, "Export", Screen::Export),
                            (4, "Results", Screen::Results),
                        ] {
                            let enabled = !self.busy
                                && match screen {
                                    Screen::Connect => true,
                                    Screen::Library => {
                                        !self.items.is_empty() || self.screen == Screen::Library
                                    }
                                    Screen::Export => !self.selected.is_empty(),
                                    Screen::Results => {
                                        self.outcome.is_some() || self.export_folder.is_some()
                                    }
                                };
                            if step(ui, n, label, self.screen == screen, enabled).clicked() {
                                self.screen = screen;
                            }
                        }
                    },
                );
                let controls =
                    egui::Rect::from_min_max(egui::pos2(full.right() - 194., full.top()), full.max);
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(controls)
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 4.;
                        if icon_button(ui, Icon::Close, "Close ITool").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if icon_button(ui, Icon::Maximize, "Maximize / restore").clicked() {
                            let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                        }
                        if icon_button(ui, Icon::Minimize, "Minimize").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                        if icon_button(ui, Icon::Settings, "Settings").clicked() {
                            self.settings_open = true;
                        }
                        if icon_button(
                            ui,
                            if self.dark_mode {
                                Icon::Moon
                            } else {
                                Icon::Sun
                            },
                            "Switch light / dark theme",
                        )
                        .clicked()
                        {
                            self.dark_mode = !self.dark_mode;
                            apply_theme(&ctx, self.dark_mode);
                            if let Err(e) = save_dark_preference(self.dark_mode) {
                                self.error = Some(format!("Could not save theme: {e}"));
                            }
                        }
                    },
                );
                let y = full.bottom() + 10.;
                ui.painter().line_segment(
                    [
                        egui::pos2(full.left() - 20., y),
                        egui::pos2(full.right() + 20., y),
                    ],
                    Stroke::new(1., p.border),
                );
            });

        if matches!(self.screen, Screen::Library | Screen::Export) {
            self.footer(ui);
        }
        if self.screen == Screen::Library && self.show_sidebar {
            self.library_sidebar(ui);
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.canvas).inner_margin(Margin::same(24))).show(ui, |ui| {
            match self.screen {
                Screen::Connect => self.connect_screen(ui),
                Screen::Library => {
                    self.library_toolbar(ui);
                    if self.items.is_empty() {
                        ui.add_space(48.0);
                        ui.heading("No accessible media found");
                        ui.label("Unlock your iPhone and rescan. Originals stored only in iCloud may not be available over USB.");
                        if ui.add_enabled(!self.demo, secondary("Rescan iPhone")).clicked() { self.scan(); }
                    } else { self.gallery(ui); }
                }
                Screen::Export => self.export_screen(ui),
                Screen::Results => self.results_screen(ui),
            }
        });
        self.settings(&ctx);
        // Explicitly requested demo capture only. This path never starts a device operation.
        self.capture_frames += 1;
        if self.demo
            && let Ok(path) = std::env::var("ITOOL_UI_CAPTURE")
        {
            if self.capture_frames == 8 {
                let context = ctx.clone();
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
                    context.send_viewport_cmd(egui::ViewportCommand::Close);
                });
            }
            ctx.request_repaint();
        }
        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn connect_screen(&mut self, ui: &mut egui::Ui) {
        let p = Palette::new(self.dark_mode);
        egui::ScrollArea::vertical().show(ui,|ui| {
            ui.add_space(16.);
            let width=ui.available_width();
            ui.columns(2,|columns| {
                columns[0].with_layout(egui::Layout::top_down(egui::Align::Min).with_cross_justify(false),|left| {
                    left.label(bold("Bring your originals home.",28.).color(p.text));
                    left.label(RichText::new("Copy photos, videos and Live Photos from your iPhone to your PC.").size(14.).color(p.muted));
                    left.add_space(20.);
                    for (n,text) in [(1,"Connect your iPhone with a USB cable"),(2,"Unlock your iPhone"),(3,"Tap Trust if your iPhone asks")] {
                        left.horizontal(|ui| {
                            let (rect,_)=ui.allocate_exact_size(Vec2::splat(36.),egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(),18.,p.panel);
                            ui.painter().circle_stroke(rect.center(),18.,Stroke::new(1.,p.border));
                            ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,n.to_string(),egui::FontId::proportional(16.),p.text);
                            ui.add_space(8.);ui.label(text);
                        });
                        left.add_space(10.);
                    }
                    left.add_space(10.);
                    let response=left.add_enabled(!self.busy,primary("").min_size(Vec2::new(330.,56.)));
                    left.painter().text(response.rect.center(),egui::Align2::CENTER_CENTER,if self.scanning {"Scanning…"} else {"Scan iPhone  →"},egui::FontId::new(16.,egui::FontFamily::Name("UI Semibold".into())),Color32::WHITE);
                    response.widget_info(||egui::WidgetInfo::labeled(egui::Role::Button,!self.busy,"Scan iPhone"));
                    if response.clicked() {if self.demo {self.screen=Screen::Library;} else {self.scan();}}
                    left.label(RichText::new("Ready when your iPhone is connected and unlocked.").size(12.).color(p.muted));
                    left.add_space(18.);
                    left.horizontal(|ui| {
                        let (r,_)=ui.allocate_exact_size(Vec2::splat(22.),egui::Sense::hover());
                        ui.painter().circle_stroke(r.center(),10.,Stroke::new(1.4,p.accent));
                        ui.painter().text(r.center(),egui::Align2::CENTER_CENTER,"?",egui::FontId::proportional(15.),p.accent);
                        if ui.link(RichText::new("iPhone not showing up?").color(p.accent)).clicked() {self.connection_help=!self.connection_help;}
                    });
                    if self.connection_help {left.label("Install Apple Devices or iTunes with Apple Mobile Device Support. Use a data cable, unlock the phone, confirm Trust and connect one iPhone at a time.");left.hyperlink_to("Apple Devices for Windows","https://apps.microsoft.com/detail/9np83lwlpz9k");}
                    if let Some(error)=&self.error {left.colored_label(p.danger,error);}
                });
                let right=&mut columns[1];
                let start=right.cursor().min;
                right.painter().line_segment([start+Vec2::new(-12.,0.),start+Vec2::new(-12.,400.)],Stroke::new(1.,p.border));
                right.vertical_centered(|ui| {
                    let texture=if self.dark_mode {&self.illustration_dark} else {&self.illustration};
                    ui.add(egui::Image::new(texture).uv(egui::Rect::from_min_max(egui::pos2(0.10,0.06),egui::pos2(0.97,0.94))).fit_to_exact_size(Vec2::new(ui.available_width().min(380.),375.)));
                    ui.label(RichText::new("Your phone stays unchanged.").size(12.).color(p.muted));
                });
            });
            ui.add_space(28.);ui.separator();ui.add_space(16.);
            let feature_start=ui.cursor().min;
            for n in [1.,2.] { let x=feature_start.x+width*n/3.;ui.painter().line_segment([egui::pos2(x,feature_start.y),egui::pos2(x,feature_start.y+40.)],Stroke::new(1.,p.border)); }
            ui.columns(3,|columns| {
                for (column,(icon,label)) in columns.iter_mut().zip([(Icon::Photo,"Original files"),(Icon::Live,"Live Photo pairs"),(Icon::Monitor,"Saved to your PC")]) {
                    column.horizontal(|ui| {
                        ui.add_space((width/3.-190.).max(0.)/2.);
                        let (r,_)=ui.allocate_exact_size(Vec2::splat(30.),egui::Sense::hover());
                        draw_icon(ui.painter(),r,icon,p.muted);ui.add_space(8.);ui.label(RichText::new(label).size(14.).color(p.muted));
                    });
                }
            });
        });
    }

    fn library_sidebar(&mut self, ui: &mut egui::Ui) {
        let p = Palette::new(self.dark_mode);
        egui::Panel::left("library_sidebar")
            .exact_size(188.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(p.sidebar)
                    .inner_margin(Margin::symmetric(12, 20)),
            )
            .show(ui, |ui| {
                let r = ui.max_rect();
                ui.painter().line_segment(
                    [
                        r.right_top() + Vec2::new(12., -20.),
                        r.right_bottom() + Vec2::new(12., 20.),
                    ],
                    Stroke::new(1., p.border),
                );
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.label(bold("Library", 14.0).color(p.text));
                    ui.add_space(8.0);
                    for (filter, label, icon) in [
                        (MediaFilter::All, "All media", Icon::Photo),
                        (MediaFilter::Photos, "Photos", Icon::Photo),
                        (MediaFilter::Videos, "Videos", Icon::Video),
                        (MediaFilter::LivePhotos, "Live Photos", Icon::Live),
                    ] {
                        let active = self.media_filter == filter
                            && !self.favorites_only
                            && self.album_filter.is_none()
                            && self.selected_month.is_none();
                        if nav(ui, icon, label, active).clicked() {
                            self.media_filter = filter;
                            self.favorites_only = false;
                            self.album_filter = None;
                            self.selected_month = None;
                            let _ = self.jobs.send(Job::CancelPreviews);
                        }
                    }
                    ui.add_space(20.0);
                    ui.separator();
                    ui.add_space(20.0);
                    ui.label(bold("By month", 13.0).color(p.muted));
                    ui.add_space(8.0);
                    for month in &self.months {
                        if nav(
                            ui,
                            Icon::Calendar,
                            &month.label,
                            self.selected_month.as_deref() == Some(&month.label),
                        )
                        .on_hover_text(format!(
                            "{} items · {}",
                            month.count,
                            format_size(month.bytes)
                        ))
                        .clicked()
                        {
                            self.selected_month =
                                if self.selected_month.as_deref() == Some(&month.label) {
                                    None
                                } else {
                                    Some(month.label.clone())
                                };
                        }
                    }
                    ui.add_space(24.0);
                    ui.collapsing("More collections", |ui| {
                        ui.add_enabled_ui(self.favorites_available, |ui| {
                            if nav(ui, Icon::Live, "Favorites", self.favorites_only)
                                .on_hover_text(&self.metadata_status)
                                .clicked()
                            {
                                self.favorites_only = !self.favorites_only;
                                self.album_filter = None;
                            }
                        });
                        if !self.albums_available {
                            ui.label(
                                RichText::new(if self.demo {
                                    "Albums appear when available"
                                } else {
                                    "Albums unavailable on this iPhone"
                                })
                                .small()
                                .color(p.muted),
                            )
                            .on_hover_text(&self.metadata_status);
                        } else {
                            let albums: std::collections::BTreeSet<_> = self
                                .metadata
                                .values()
                                .flat_map(|m| m.albums.iter().cloned())
                                .collect();
                            for album in albums {
                                if nav(
                                    ui,
                                    Icon::Folder,
                                    &album,
                                    self.album_filter.as_deref() == Some(&album),
                                )
                                .clicked()
                                {
                                    self.album_filter = Some(album);
                                    self.favorites_only = false;
                                }
                            }
                        }
                        if ui
                            .add_enabled(!self.demo && !self.busy, secondary("Rescan iPhone"))
                            .clicked()
                        {
                            self.scan();
                        }
                    });
                });
            });
    }

    fn library_toolbar(&mut self, ui: &mut egui::Ui) {
        let p = Palette::new(self.dark_mode);
        ui.label(bold("Choose your media", 24.0).color(p.text));
        ui.label(
            RichText::new("Select the photos and videos you want to copy.")
                .size(14.0)
                .color(p.muted),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 352.0).max(120.0);
            egui::Frame::new()
                .fill(p.panel)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(6)
                .inner_margin(Margin::symmetric(10, 7))
                .show(ui, |ui| {
                    ui.spacing_mut().interact_size.y = 20.;
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                        draw_icon(ui.painter(), rect, Icon::Search, p.muted);
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut self.search)
                                    .frame(egui::Frame::NONE)
                                    .hint_text("Search by name or date")
                                    .margin(Margin::ZERO)
                                    .min_size(Vec2::new(0., 20.))
                                    .desired_width((width - 50.0).max(80.0)),
                            )
                            .changed()
                        {
                            let _ = self.jobs.send(Job::CancelPreviews);
                        }
                    });
                });
            menu(
                ui,
                Icon::Grid,
                if self.list_view { "Details" } else { "Grid" },
                88.,
                |ui| {
                    ui.selectable_value(&mut self.list_view, false, "Grid");
                    ui.selectable_value(&mut self.list_view, true, "Details");
                },
            );
            menu(ui, Icon::Sort, "Sort", 88., |ui| {
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
            menu(ui, Icon::Options, "View options", 140., |ui| {
                ui.label(bold("Group the gallery", 13.0));
                for (value, label) in [
                    (GroupBy::Continuous, "No grouping"),
                    (GroupBy::Month, "Month"),
                    (GroupBy::Week, "Week"),
                    (GroupBy::Date, "Day"),
                    (GroupBy::MediaType, "Media type"),
                    (GroupBy::SourceFolder, "Source folder"),
                ] {
                    ui.selectable_value(&mut self.group_by, value, label);
                }
                ui.separator();
                ui.label("Thumbnail size");
                ui.add(egui::Slider::new(&mut self.tile_size, 128.0..=256.0).show_value(false));
                if ui.button("Latest day").clicked()
                    && let Some(item) = self.items.iter().max_by_key(|i| i.timestamp)
                {
                    self.search = item.date_str.clone();
                    self.selected_month = None;
                }
                ui.separator();
                if ui.button("Select shown").clicked() {
                    self.selected.extend(
                        self.filtered_items()
                            .into_iter()
                            .map(|i| i.path.clone())
                            .collect::<Vec<_>>(),
                    );
                }
                if ui.button("Invert shown").clicked() {
                    let paths: Vec<_> = self
                        .filtered_items()
                        .into_iter()
                        .map(|i| i.path.clone())
                        .collect();
                    for path in paths {
                        if !self.selected.remove(&path) {
                            self.selected.insert(path);
                        }
                    }
                }
                if ui.button("Clear selection").clicked() {
                    self.selected.clear();
                    self.selected_only = false;
                }
                ui.checkbox(&mut self.selected_only, "Selected only");
                if ui.button("Export new files from this view").clicked() {
                    self.selected = self
                        .filtered_items()
                        .into_iter()
                        .map(|i| i.path.clone())
                        .collect();
                    self.options.new_only = true;
                    self.screen = Screen::Export;
                    self.error = None;
                }
            });
        });
        if self.selected_month.is_some()
            || self.album_filter.is_some()
            || self.favorites_only
            || !self.search.is_empty()
        {
            ui.horizontal_wrapped(|ui| {
                if let Some(month) = &self.selected_month {
                    ui.label(RichText::new(month).color(p.accent));
                }
                if let Some(album) = &self.album_filter {
                    ui.label(RichText::new(album).color(p.accent));
                }
                if self.favorites_only {
                    ui.label("Favorites");
                }
                if ui.small_button("Clear filters").clicked() {
                    self.search.clear();
                    self.selected_month = None;
                    self.album_filter = None;
                    self.favorites_only = false;
                    self.media_filter = MediaFilter::All;
                }
            });
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        let p = Palette::new(self.dark_mode);
        egui::Panel::bottom("guided_footer")
            .exact_size(76.)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .inner_margin(Margin::symmetric(24, 14)),
            )
            .show(ui, |ui| {
                let r = ui.max_rect();
                ui.painter().line_segment(
                    [
                        r.left_top() - Vec2::new(24., 14.),
                        r.right_top() + Vec2::new(24., -14.),
                    ],
                    Stroke::new(1., p.border),
                );
                let library = self.screen == Screen::Library;
                let left = if library && self.show_sidebar {
                    r.left() + 188.
                } else {
                    r.left()
                };
                if library && self.show_sidebar {
                    draw_icon(
                        ui.painter(),
                        egui::Rect::from_center_size(
                            egui::pos2(r.left() + 8., r.center().y),
                            Vec2::splat(18.),
                        ),
                        Icon::Folder,
                        p.muted,
                    );
                    ui.painter().text(
                        egui::pos2(r.left() + 28., r.center().y),
                        egui::Align2::LEFT_CENTER,
                        if self.demo {
                            "Sample library"
                        } else {
                            "Scanned library"
                        },
                        egui::FontId::proportional(12.),
                        p.muted,
                    );
                    ui.painter().line_segment(
                        [
                            egui::pos2(r.left() + 164., r.top() - 14.),
                            egui::pos2(r.left() + 164., r.bottom() + 14.),
                        ],
                        Stroke::new(1., p.border),
                    );
                }
                let bytes: u64 = self
                    .items
                    .iter()
                    .filter(|i| self.selected.contains(&i.path))
                    .map(|i| i.size)
                    .sum();
                let originals: usize = self
                    .items
                    .iter()
                    .filter(|i| self.selected.contains(&i.path))
                    .map(|i| i.resources.len())
                    .sum();
                let summary = if library {
                    format!(
                        "{} items selected · {}",
                        self.selected.len(),
                        format_size(bytes)
                    )
                } else {
                    format!(
                        "{} items · {originals} original files · {}",
                        self.selected.len(),
                        format_size(bytes)
                    )
                };
                ui.painter().text(
                    egui::pos2(left, r.center().y),
                    egui::Align2::LEFT_CENTER,
                    summary,
                    egui::FontId::new(14., egui::FontFamily::Name("UI Semibold".into())),
                    p.text,
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(r)
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                    |ui| {
                        if library {
                            if ui
                                .add_enabled(
                                    !self.selected.is_empty() && !self.busy,
                                    primary("Continue to export  →"),
                                )
                                .clicked()
                            {
                                self.screen = Screen::Export;
                                self.error = None;
                            }
                            ui.add_space(12.);
                            if ui
                                .link(RichText::new("Review selection").color(p.accent))
                                .clicked()
                            {
                                self.selected_only = !self.selected_only;
                            }
                        } else {
                            let valid = itool::workflow::named_destination(
                                &self.destination,
                                &self.folder_name,
                            )
                            .is_ok()
                                && itool::validate_export_plan(&self.export_plan()).is_ok();
                            let tools_ok = !(self.options.jpeg_copies || self.options.mp4_copies)
                                || itool::workflow::tool_path("ffmpeg").is_some();
                            if ui
                                .add_enabled(
                                    valid && tools_ok && !self.busy,
                                    primary("Start export  →"),
                                )
                                .clicked()
                            {
                                if self.demo {
                                    self.screen = Screen::Results;
                                    self.status =
                                        "Sample export complete · no files were copied".into();
                                    self.outcome = Some(ExportOutcome {
                                        folder: PathBuf::from(&self.destination)
                                            .join(&self.folder_name),
                                        copied: originals,
                                        skipped: 0,
                                        converted: 0,
                                        copied_bytes: bytes,
                                        elapsed_seconds: 24.,
                                        warnings: vec![],
                                    });
                                } else {
                                    self.begin_export();
                                }
                            }
                            if ui
                                .add(secondary("Back").min_size(Vec2::new(102., 48.)))
                                .clicked()
                            {
                                self.screen = Screen::Library;
                            }
                        }
                    },
                );
            });
    }

    fn destination_fields(&mut self, ui: &mut egui::Ui) {
        ui.label(bold("Save location", 14.));
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.destination)
                    .desired_width((ui.available_width() - 110.0).max(100.0))
                    .margin(Vec2::new(10.0, 9.0)),
            );
            if ui.add(secondary("Browse…")).clicked()
                && let Some(path) = rfd::FileDialog::new().pick_folder()
            {
                self.destination = path.display().to_string();
            }
        });
        ui.add_space(20.0);
        ui.label(bold("Folder name", 14.));
        ui.add(
            egui::TextEdit::singleline(&mut self.folder_name)
                .desired_width(ui.available_width())
                .margin(Vec2::new(10.0, 9.0)),
        );
        ui.add_space(20.0);
        ui.label(bold("Organize files", 14.));
        egui::ComboBox::from_id_salt("output_layout")
            .width(ui.available_width())
            .selected_text(self.export_layout.label())
            .show_ui(ui, |ui| {
                for layout in [
                    ExportLayout::Flat,
                    ExportLayout::Day,
                    ExportLayout::Month,
                    ExportLayout::MediaType,
                    ExportLayout::SourceFolder,
                ] {
                    ui.selectable_value(&mut self.export_layout, layout, layout.label());
                }
            });
        ui.add_space(8.0);
        ui.label(
            RichText::new("Original filenames are kept. Live Photo pairs stay together.").small(),
        );
        ui.label(
            RichText::new("Day/month folders use device file dates. Choose an NTFS save location.")
                .small()
                .color(Palette::new(self.dark_mode).muted),
        );
    }

    fn folder_preview(&self, ui: &mut egui::Ui) {
        let p = Palette::new(self.dark_mode);
        egui::Frame::new()
            .fill(p.panel)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(6)
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.set_min_width((ui.available_width() - 8.0).max(150.0));
                ui.label(bold("Your export will look like this", 14.));
                ui.add_space(12.0);
                tree_row(
                    ui,
                    Icon::Folder,
                    &PathBuf::from(self.destination.trim())
                        .join(self.folder_name.trim())
                        .display()
                        .to_string(),
                    0.,
                );
                let plan = self.export_plan();
                let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for entry in &plan {
                    groups
                        .entry(entry.folder.clone())
                        .or_default()
                        .push(entry.source.rsplit('/').next().unwrap_or("original").into());
                }
                egui::ScrollArea::vertical()
                    .id_salt("folder_preview_tree")
                    .max_height(132.)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.;
                        for (folder, files) in groups.iter().rev() {
                            if !folder.is_empty() {
                                tree_row(ui, Icon::Folder, folder, 24.);
                            }
                            for name in files.iter().take(2) {
                                tree_row(
                                    ui,
                                    Icon::File,
                                    name,
                                    if folder.is_empty() { 24. } else { 52. },
                                );
                            }
                            if files.len() > 2 {
                                ui.label(
                                    RichText::new(format!(
                                        "             … and {} more",
                                        files.len() - 2
                                    ))
                                    .small()
                                    .color(p.muted),
                                );
                            }
                        }
                    });
                ui.separator();
                ui.collapsing(format!("View all {} original files", plan.len()), |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("all_export_files")
                        .max_height(180.0)
                        .show_rows(ui, 22.0, plan.len(), |ui, range| {
                            for entry in &plan[range] {
                                ui.label(format!(
                                    "{}/{}",
                                    entry.folder,
                                    entry.source.rsplit('/').next().unwrap_or("")
                                ))
                                .on_hover_text(&entry.source);
                            }
                        });
                });
            });
    }

    fn export_screen(&mut self, ui: &mut egui::Ui) {
        let p = Palette::new(self.dark_mode);
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(bold("Make it yours.",28.0).color(p.text));
            ui.label(RichText::new("Choose where your originals will be saved.").color(p.muted));
            ui.add_space(12.0);
            ui.spacing_mut().item_spacing.x=24.;
            if ui.available_width() >= 760.0 {
                ui.columns(2, |columns| { self.destination_fields(&mut columns[0]); self.folder_preview(&mut columns[1]); });
            } else { self.destination_fields(ui); ui.add_space(20.0); self.folder_preview(ui); }
            ui.add_space(12.0);
            egui::Frame::new().stroke(Stroke::new(1.,p.border)).corner_radius(6).inner_margin(Margin::same(12)).show(ui,|ui| {
            ui.set_width(ui.available_width());
            ui.collapsing("More options", |ui| {
                ui.checkbox(&mut self.options.new_only, "Only copy new files");
                ui.label(RichText::new("Uses export history in this save location and checks saved copies. Missing or changed files are copied again.").small().color(p.muted));
                ui.add_space(10.);

                ui.checkbox(&mut self.options.capture_timestamps, "Use embedded capture dates for saved file timestamps");
                ui.label(RichText::new("A date without an offset uses the PC timezone. Missing or ambiguous dates are left unchanged and reported.").small().color(p.muted));
                ui.add_space(8.0);
                let converter = itool::workflow::tool_path("ffmpeg").is_some();
                ui.add_enabled(converter, egui::Checkbox::new(&mut self.options.jpeg_copies, "Also make JPEG copies of HEIC/HEIF photos"));
                ui.add_enabled(converter && itool::workflow::tool_path("ffprobe").is_some(), egui::Checkbox::new(&mut self.options.mp4_copies, "Also make H.264 MP4 copies of SDR MOV/M4V videos"));
                ui.label(RichText::new("Originals are always kept. Compatible copies may lose metadata or HDR information; HDR videos are skipped. Decoder support depends on your FFmpeg build.").small().color(p.muted));
                if !converter { ui.hyperlink_to("Install FFmpeg to enable compatible copies", "https://ffmpeg.org/download.html"); }
            });
            });
            ui.add_space(16.0);
            if let Err(error) = itool::workflow::named_destination(&self.destination, &self.folder_name) { ui.colored_label(p.danger, error.to_string()); }
            if let Err(error) = itool::validate_export_plan(&self.export_plan()) { ui.colored_label(p.danger, error.to_string()); }
            let hidden = self.selected.len().saturating_sub(self.filtered_items().iter().filter(|i| self.selected.contains(&i.path)).count());
            if hidden > 0 { ui.label(format!("Includes {hidden} selected items hidden by your current filters.")); }
            if let Some(error) = &self.error { ui.colored_label(p.danger, error); }
        });
    }

    fn results_screen(&mut self, ui: &mut egui::Ui) {
        let p = Palette::new(self.dark_mode);
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(24.0);
            let title = if self.busy { "Saving your originals…" } else if self.error.is_some() { "Your export stopped" } else if self.outcome.as_ref().is_some_and(|r| !r.warnings.is_empty()) { "Saved, with a few notes" } else { "Your originals are home." };
            ui.label(bold(title,30.0).color(p.text)); ui.add_space(12.0);
            ui.label(&self.status); if self.demo {ui.label(RichText::new("All counts and rates below are simulated sample data.").small().color(p.muted));} ui.add_space(24.0);
            if self.busy {
                let ratio = if self.total == 0 { 0.0 } else { (self.done as f32 / self.total as f32).clamp(0.0, 1.0) };
                ui.add(egui::ProgressBar::new(ratio).show_percentage());
                ui.label(&self.current_file_progress);
                ui.label(format!("{} of {} processed", format_size(self.done), format_size(self.total)));
                ui.label(RichText::new("Checking saved files and making compatible copies can take additional time.").small().color(p.muted));
                if ui.add_enabled(!self.cancel.load(Ordering::Relaxed), secondary("Cancel export")).clicked() {
                    self.cancel.store(true, Ordering::Relaxed);
                    self.status = "Cancelling… completed originals will be kept.".into();
                }
            }
            if let Some(outcome) = &self.outcome {
                ui.label(RichText::new(format!("{} originals copied · {} already saved · {} compatible copies", outcome.copied, outcome.skipped, outcome.converted)).size(17.0).strong());
                if outcome.copied_bytes > 0 && outcome.elapsed_seconds > 0.0 {
                    ui.label(RichText::new(format!("{} copied in {:.1}s · {:.2} MiB/s effective rate, including checks and optional steps", format_size(outcome.copied_bytes), outcome.elapsed_seconds, outcome.copied_bytes as f64 / 1_048_576.0 / outcome.elapsed_seconds)).small().color(p.muted));
                }
                if !outcome.warnings.is_empty() {
                    ui.add_space(16.0);
                    ui.collapsing(format!("{} notes about optional steps", outcome.warnings.len()), |ui| {
                        for warning in &outcome.warnings { ui.colored_label(p.warning, warning); }
                    });
                }
            }
            if let Some(error) = &self.error { ui.colored_label(p.danger, error); ui.label("Completed originals are kept. Retry in a new folder with Only copy new files to reuse valid history."); }
            ui.add_space(24.0);
            let folder = self.report.as_ref().or(self.export_folder.as_ref());
            if let Some(folder) = folder {
                ui.label(folder.display().to_string());
                if ui.add_enabled(folder.is_dir() && !self.demo, primary(if self.busy { "Open export folder" } else { "Open folder" })).clicked() {
                    let _ = std::process::Command::new("explorer.exe").arg(folder).spawn();
                }
            }
            if !self.busy && ui.add(secondary("Back to library")).clicked() { self.screen = Screen::Library; self.error = None; }
            ui.add_space(20.0);
            ui.collapsing("Transfer details", |ui| {
                ui.label("Copied originals are hashed on receipt and checked against a local disk reread. This is not an independent digest from the iPhone.");
                ui.label("completed.json records originals copied in this batch; transfer.json records skipped files, compatible copies and notes. Private media paths remain local.");
                ui.label("New-file detection uses this device's path, size and filesystem dates plus a checked saved copy. It cannot detect source content changed without a metadata change.");
            });
        });
    }

    fn settings(&mut self, ctx: &egui::Context) {
        let mut open = self.settings_open;
        egui::Window::new("Settings").open(&mut open).collapsible(false).default_width(440.0).show(ctx, |ui| {
            ui.label("Appearance");
            if ui.checkbox(&mut self.dark_mode, "Dark mode").changed() {
                apply_theme(ctx, self.dark_mode);
                if let Err(e) = save_dark_preference(self.dark_mode) { self.error = Some(format!("Could not save appearance preference: {e}")); }
            }
            ui.checkbox(&mut self.show_sidebar, "Show library sidebar");
            ui.add_space(12.0); ui.separator(); ui.add_space(12.0);
            ui.label("Optional media tools");
            for tool in ["ffmpeg", "ffprobe"] {
                match itool::workflow::tool_path(tool) {
                    Some(path) => { ui.label(format!("{tool}: available")).on_hover_text(path.display().to_string()); }
                    None => { ui.label(format!("{tool}: not installed")); }
                }
            }
            ui.hyperlink_to("FFmpeg downloads", "https://ffmpeg.org/download.html");
            ui.label("Place tools beside the app in a tools folder, or add them to PATH. They are used locally; ITool does not download them.");
            ui.add_space(12.0);
            if !self.metadata_status.is_empty() { ui.label(&self.metadata_status); }
            ui.label("ITool · originals stay on your iPhone");
        });
        self.settings_open = open;
    }
}
