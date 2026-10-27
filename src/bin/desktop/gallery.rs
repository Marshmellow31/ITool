use super::super::*;
impl Desktop {
    pub(super) fn gallery(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let p = Palette::new(self.dark_mode);
        ui.add_space(8.0);
        let mut filtered = Self::get_filtered_items(
            &self.items,
            &self.search,
            self.media_filter,
            self.sort_order,
            self.selected_month.as_deref(),
        );
        filtered.retain(|item| {
            let metadata = self.metadata.get(&item.path);
            (!self.favorites_only || metadata.is_some_and(|m| m.favorite == Some(true)))
                && self
                    .album_filter
                    .as_ref()
                    .is_none_or(|album| metadata.is_some_and(|m| m.albums.contains(album)))
        });
        if self.selected_only {
            filtered.retain(|i| self.selected.contains(&i.path));
        }
        let available_width = (ui.available_width() - 4.0).max(200.0);
        let gap = 12.0;
        let target_size = self.tile_size;
        let columns = ((available_width + gap) / (target_size + gap))
            .floor()
            .max(1.0) as usize;
        let card_width =
            ((available_width - gap * (columns as f32 - 1.0)) / columns as f32).floor();
        let mut card_height = card_width * 0.75 + 40.0;
        if self.group_by == GroupBy::Continuous
            && !self.list_view
            && filtered.len() <= 8
            && !filtered.is_empty()
        {
            let rows = filtered.len().div_ceil(columns) as f32;
            let fitted = (ui.available_height() - 24. - rows * (gap + 8.)) / rows;
            card_height = card_height.min(fitted.max(108.));
        }

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
                    self.favorites_only = false;
                    self.album_filter = None;
                }
            });
            return;
        }

        if self.group_by != GroupBy::Continuous {
            filtered = grouped_items(&filtered, self.group_by)
                .into_iter()
                .flat_map(|(_, items)| items)
                .collect();
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
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let groups = grouped_items(&filtered, self.group_by);
                    for (title, items) in groups {
                        if self.group_by != GroupBy::Continuous {
                            ui.heading(title);
                        }
                        ui.horizontal(|ui| {
                            ui.add_space(30.0);
                            ui.add_sized(
                                [210.0, 24.0],
                                egui::Label::new(RichText::new("File").strong()),
                            );
                            ui.add_sized([100.0, 24.0], egui::Label::new("Device date"));
                            ui.add_sized([85.0, 24.0], egui::Label::new("Size"));
                            ui.label("Type / resources");
                        });
                        let row_height = 32.0;
                        for item in items {
                            let rect = egui::Rect::from_min_size(
                                ui.cursor().min,
                                Vec2::new(ui.available_width(), row_height),
                            );
                            if !ui.is_rect_visible(rect) {
                                ui.allocate_space(rect.size());
                                continue;
                            }
                            ui.horizontal(|ui| {
                                let mut checked = self.selected.contains(&item.path);
                                if ui
                                    .add_enabled(
                                        !self.busy,
                                        egui::Checkbox::without_text(&mut checked),
                                    )
                                    .clicked()
                                {
                                    card_click = Some((
                                        item_indices[&item.path],
                                        ui.input(|i| i.modifiers.shift),
                                    ));
                                }
                                let label = egui::Label::new(
                                    RichText::new(safe_truncate(&item.name, 27)).strong(),
                                )
                                .sense(egui::Sense::click());
                                if ui
                                    .add_sized([210.0, 24.0], label)
                                    .on_hover_text(&item.path)
                                    .clicked()
                                    && !self.busy
                                {
                                    card_click = Some((
                                        item_indices[&item.path],
                                        ui.input(|i| i.modifiers.shift),
                                    ));
                                }
                                ui.add_sized([100.0, 24.0], egui::Label::new(&item.date_str));
                                ui.add_sized(
                                    [85.0, 24.0],
                                    egui::Label::new(format_size(item.size)),
                                );
                                ui.label(format!(
                                    "{} · {} files",
                                    media_label(item),
                                    item.resources.len()
                                ));
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
                                    let has_status = self.thumbnail_status.contains_key(&item.path);

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
                            ui.allocate_space(Vec2::new(available_width, card_height + gap));
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
                            ui.label(RichText::new(group_title).size(16.0).strong().color(p.text));
                            let group_bytes: u64 = group_items.iter().map(|i| i.size).sum();
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
                                    let all_selected =
                                        group_items.iter().all(|i| self.selected.contains(&i.path));
                                    let btn_text = if all_selected {
                                        "Deselect group"
                                    } else {
                                        "Select group"
                                    };
                                    let btn = secondary(btn_text);

                                    if ui.add_enabled(!self.busy, btn).clicked() {
                                        let paths: Vec<String> =
                                            group_items.iter().map(|i| i.path.clone()).collect();
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
                                ui.allocate_space(Vec2::new(available_width, card_height + gap));
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
                Color32::from_rgba_premultiplied(66, 91, 220, 35),
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
            let anchor = self
                .selection_anchor
                .as_ref()
                .and_then(|p| item_indices.get(p))
                .copied();
            if let Some(prev_idx) = anchor.filter(|_| is_shift) {
                let start = prev_idx.min(curr_idx);
                let end = prev_idx.max(curr_idx);
                let paths_to_add: Vec<String> = filtered[start..=end]
                    .iter()
                    .map(|i| i.path.clone())
                    .collect();
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
    }
}
