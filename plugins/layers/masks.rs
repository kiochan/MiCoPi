//! Mask presentation only; mutations use the shared, idle-guarded action registry.
use super::*;
use crate::engine::raster_masks::{MaskMetadata, MaskOwner, PaintTarget};

#[derive(Clone)]
struct PropertyDraft {
    density: u8,
    feather: u16,
    revision: u64,
    dragging: bool,
    submitted: bool,
}

fn draft_key(document: u64, mask: u64) -> Id {
    Id::new(("mask-property-draft", document, mask))
}

impl Editor {
    pub(super) fn paint_target_thumbnail_border(&self, ui: &Ui, rect: Rect, active: bool) {
        let a = &self.settings.ui;
        let colors = theme::Colors::new(a);
        ui.painter().rect_stroke(
            rect.expand(if active { 2. } else { 1. }),
            a.corner_radius * 0.5,
            Stroke::new(
                if active { 2. } else { 1. },
                if active { colors.accent } else { colors.border },
            ),
            StrokeKind::Outside,
        );
    }

    pub(super) fn mask_thumbnail_ui(
        &mut self,
        ui: &mut Ui,
        owner: MaskOwner,
        rect: Rect,
        pending: &mut Vec<crate::application::Action>,
    ) -> Option<Response> {
        let metadata = self
            .session
            .doc
            .mask_state()
            .items
            .iter()
            .find(|mask| mask.owner == owner)?
            .clone();
        let plane = self.session.doc.mask_plane(metadata.id).ok()?;
        let a = &self.settings.ui;
        let active = self.session.doc.mask_state().target == PaintTarget::Mask { id: metadata.id };
        self.paint_target_thumbnail_border(ui, rect, active);
        // Preview raw samples, independent of enabled, density and feather properties.
        let mut mesh = egui::Mesh::default();
        for y in 0..16 {
            for x in 0..16 {
                let sample = plane.sample(
                    (y * self.session.doc.height / 16) * self.session.doc.width
                        + x * self.session.doc.width / 16,
                );
                let tile = Rect::from_min_size(
                    rect.min
                        + vec2(
                            x as f32 * rect.width() / 16.,
                            y as f32 * rect.height() / 16.,
                        ),
                    rect.size() / 16.,
                );
                mesh.add_colored_rect(tile, Color32::from_gray(sample));
            }
        }
        ui.painter().add(Shape::mesh(mesh));
        if !metadata.enabled {
            ui.painter().line_segment(
                [rect.left_bottom(), rect.right_top()],
                Stroke::new(2., theme::Colors::new(a).accent),
            );
        }
        let response = ui.interact(
            rect,
            ui.id()
                .with(("mask-thumbnail", self.session.doc.identity, metadata.id)),
            Sense::click(),
        );
        #[cfg(test)]
        ui.data_mut(|data| {
            data.insert_temp(
                Id::new((
                    "mask-thumbnail-control",
                    self.session.doc.identity,
                    metadata.id,
                )),
                rect,
            )
        });
        if response.clicked() {
            self.focus_group_ui(
                ui.ctx(),
                match owner {
                    MaskOwner::Group { id } => Some(id),
                    _ => None,
                },
            );
            pending.push(crate::application::Action::new(
                "mask.target",
                json!({"target":{"kind":"mask","id":metadata.id}}),
            ));
        }
        Some(response.on_hover_text(tr("Paint Mask · raw grayscale preview")))
    }

    fn mask_owner_name_ui(&mut self, ctx: &Context, owner: MaskOwner) -> String {
        match owner {
            MaskOwner::Layer { id } => self
                .session
                .doc
                .layer_index(id)
                .map(|index| {
                    if self.session.doc.custom_layer_name(index).is_empty() {
                        localize(format!("Layer {:02}", index + 1))
                    } else {
                        self.session.doc.layer_name(index)
                    }
                })
                .unwrap_or_default(),
            MaskOwner::Group { id } => self
                .group_query_ui(ctx, &LayerFilter::default())
                .ok()
                .and_then(|data| {
                    data.groups
                        .iter()
                        .find(|group| group.id == id)
                        .map(|group| group.name.clone())
                })
                .unwrap_or_default(),
        }
    }

    pub(super) fn mask_properties_ui(
        &mut self,
        ui: &mut Ui,
        pending: &mut Vec<crate::application::Action>,
    ) {
        if !self.session.registry.actions.contains_key("mask.query") {
            return;
        }
        let target = self.session.doc.mask_state().target;
        let selected = match target {
            PaintTarget::Mask { id } => self
                .session
                .doc
                .mask_state()
                .items
                .iter()
                .find(|mask| mask.id == id)
                .cloned(),
            _ => None,
        };
        let owner = self
            .focused_group_ui(ui.ctx())
            .map(|id| MaskOwner::Group { id })
            .or_else(|| selected.as_ref().map(|mask| mask.owner))
            .unwrap_or(MaskOwner::Layer {
                id: self.session.doc.layer_id(self.session.doc.active),
            });
        let metadata = self
            .session
            .doc
            .mask_state()
            .items
            .iter()
            .find(|mask| mask.owner == owner)
            .cloned();
        let a = self.settings.ui.clone();
        let painting_name = if let Some(mask) = &selected {
            self.mask_owner_name_ui(ui.ctx(), mask.owner)
        } else {
            self.mask_owner_name_ui(
                ui.ctx(),
                MaskOwner::Layer {
                    id: self.session.doc.layer_id(self.session.doc.active),
                },
            )
        };
        ui.add_space(a.item_spacing);
        ui.add(
            Label::new(format!(
                "{} · {}",
                localize(if selected.is_some() {
                    "Painting Mask"
                } else {
                    "Painting Pixels"
                }),
                painting_name
            ))
            .wrap(),
        );
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(target.is_default(), tr("Pixels"))
                .on_hover_text(tr("Paint Layer Pixels"))
                .clicked()
            {
                pending.push(crate::application::Action::new(
                    "mask.target",
                    json!({"target":{"kind":"layer_pixels"}}),
                ));
            }
            if let Some(mask) = &metadata {
                if ui
                    .selectable_label(target == PaintTarget::Mask { id: mask.id }, tr("Mask"))
                    .on_hover_text(tr("Paint Mask"))
                    .clicked()
                {
                    pending.push(crate::application::Action::new(
                        "mask.target",
                        json!({"target":{"kind":"mask","id":mask.id}}),
                    ));
                }
            }
        });
        let owner_name = self.mask_owner_name_ui(ui.ctx(), owner);
        egui::CollapsingHeader::new(tr("Raster Mask"))
            .id_salt((
                "raster-mask-properties",
                self.session.doc.identity,
                match owner {
                    MaskOwner::Layer { id } => (false, id),
                    MaskOwner::Group { id } => (true, id),
                },
            ))
            .default_open(selected.as_ref().is_some_and(|mask| mask.owner == owner))
            .show(ui, |ui| {
                ui.add(Label::new(format!("{} · {}", localize("Mask Owner"), owner_name)).wrap());
                let Some(mask) = metadata else {
                    ui.weak(tr("Add a mask to hide pixels without erasing them."));
                    ui.menu_button(tr("Create Mask"), |ui| {
                        for (source, title, enabled) in [
                            ("reveal_all", "Reveal All", true),
                            ("hide_all", "Hide All", true),
                            (
                                "selection",
                                "Mask from Selection",
                                self.session.doc.selection().current.is_some(),
                            ),
                        ] {
                            if ui.add_enabled(enabled, Button::new(tr(title))).clicked() {
                                pending.push(crate::application::Action::new(
                                    "mask.create",
                                    json!({"owner":owner,"source":source,"select":true}),
                                ));
                                ui.close();
                            }
                        }
                    });
                    return;
                };
                let mut changes = json!({"mask_id":mask.id});
                ui.horizontal_wrapped(|ui| {
                    let mut enabled = mask.enabled;
                    if ui.checkbox(&mut enabled, tr("Enable Mask")).changed() {
                        changes["enabled"] = json!(enabled);
                    }
                    let mut linked = mask.linked;
                    if ui
                        .checkbox(&mut linked, tr("Link Mask"))
                        .on_hover_text(tr("Move the mask with its owner when supported."))
                        .changed()
                    {
                        changes["linked"] = json!(linked);
                    }
                });
                if changes.as_object().unwrap().len() > 1 {
                    pending.push(crate::application::Action::new("mask.set", changes));
                }
                let effects = self
                    .session
                    .doc
                    .renderer
                    .as_ref()
                    .is_some_and(|renderer| renderer.supports_mask_effects());
                ui.push_id(("mask-properties", mask.id), |ui| {
                    ui.add_enabled_ui(effects, |ui| {
                        if let Some(action) = self.mask_property_fields_ui(ui, &mask) {
                            pending.push(action);
                        }
                    })
                    .response
                    .on_disabled_hover_text(tr(
                        "The compositor plugin does not support mask effects.",
                    ));
                });
                ui.menu_button(tr("Mask Actions"), |ui| {
                    for (command, title, args, enabled) in [
                        (
                            "mask.invert",
                            "Invert Mask",
                            json!({"mask_id":mask.id}),
                            effects,
                        ),
                        (
                            "mask.fill",
                            "Reveal All",
                            json!({"mask_id":mask.id,"value":255}),
                            true,
                        ),
                        (
                            "mask.fill",
                            "Hide All",
                            json!({"mask_id":mask.id,"value":0}),
                            true,
                        ),
                        (
                            "mask.from_selection",
                            "Replace Mask from Selection",
                            json!({"mask_id":mask.id}),
                            self.session.doc.selection().current.is_some(),
                        ),
                        (
                            "mask.to_selection",
                            "Raw Mask to Selection",
                            json!({"mask_id":mask.id,"effective":false}),
                            true,
                        ),
                        (
                            "mask.to_selection",
                            "Effective Mask to Selection",
                            json!({"mask_id":mask.id,"effective":true}),
                            effects || mask.density == 255 && mask.feather_radius_px == 0,
                        ),
                        (
                            "mask.apply",
                            "Apply Mask to Pixels",
                            json!({"mask_id":mask.id}),
                            matches!(owner, MaskOwner::Layer { .. }),
                        ),
                        (
                            "mask.delete",
                            "Delete Mask",
                            json!({"mask_id":mask.id}),
                            true,
                        ),
                    ] {
                        if ui
                            .add_enabled(enabled, Button::new(tr(title)).wrap())
                            .clicked()
                        {
                            pending.push(crate::application::Action::new(command, args));
                            ui.close();
                        }
                    }
                });
            });
        ui.add_space(a.item_spacing);
    }

    fn mask_property_fields_ui(
        &self,
        ui: &mut Ui,
        mask: &MaskMetadata,
    ) -> Option<crate::application::Action> {
        let key = draft_key(self.session.doc.identity, mask.id);
        let mut draft = ui.data_mut(|data| data.get_temp::<PropertyDraft>(key));
        if draft.as_ref().is_some_and(|draft| {
            draft.submitted
                && (draft.density, draft.feather) == (mask.density, mask.feather_radius_px)
        }) {
            ui.data_mut(|data| data.remove::<PropertyDraft>(key));
            draft = None;
        }
        let stale = draft
            .as_ref()
            .is_some_and(|draft| draft.revision != self.session.revision);
        let mut density = draft.as_ref().map_or(mask.density, |draft| draft.density) as f32 / 255.;
        let mut feather = draft
            .as_ref()
            .map_or(mask.feather_radius_px, |draft| draft.feather) as f32;
        let before = (density, feather);
        let mut dragging = false;
        ui.add_enabled_ui(!stale, |ui| {
            parameter(ui, "Mask Density", &mut density, 0.0..=1., "%", true, false);
            parameter(
                ui,
                "Mask Feather",
                &mut feather,
                0.0..=256.,
                " px",
                false,
                false,
            );
            // The shared parameter control supports both slider and number dragging.
            dragging = ui.input(|input| input.pointer.any_down());
        });
        if stale {
            ui.weak(tr(
                "The document changed. Reset this mask adjustment before editing.",
            ));
            if ui.small_button(tr("Reset Mask Adjustment")).clicked() {
                ui.data_mut(|data| data.remove::<PropertyDraft>(key));
            }
            return None;
        }
        let value = ((density * 255.).round() as u8, feather.round() as u16);
        let changed = before != (density, feather);
        if changed {
            let entry = draft.get_or_insert(PropertyDraft {
                density: mask.density,
                feather: mask.feather_radius_px,
                revision: self.session.revision,
                dragging: false,
                submitted: false,
            });
            entry.density = value.0;
            entry.feather = value.1;
            entry.submitted = false;
        }
        if let Some(mut draft) = draft {
            let released = draft.dragging && !dragging;
            draft.dragging = dragging;
            if !draft.submitted && (released || changed && !dragging) {
                if (draft.density, draft.feather) == (mask.density, mask.feather_radius_px) {
                    ui.data_mut(|data| data.remove::<PropertyDraft>(key));
                    return None;
                }
                let mut action = crate::application::Action::new(
                    "mask.set",
                    json!({"mask_id":mask.id,"density":draft.density,"feather_radius_px":draft.feather}),
                );
                action.expected_revision = Some(draft.revision);
                draft.submitted = true;
                ui.data_mut(|data| data.insert_temp(key, draft));
                return Some(action);
            }
            ui.data_mut(|data| data.insert_temp(key, draft));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::tests::studio;
    use crate::engine::raster_masks::PlaneView;

    fn fixture() -> (Studio, Vec<u64>, u64) {
        let mut app = studio();
        app.editor.session =
            crate::application::Session::new(crate::plugins::Registry::bundled()).unwrap();
        assert!(app
            .editor
            .core("canvas.new", json!({"width":32,"height":24})));
        assert!(app.editor.core("layer.add", json!({})));
        let ids = (0..2)
            .map(|index| app.editor.session.doc.layer_id(index))
            .collect::<Vec<_>>();
        assert!(app.editor.core(
            "layer.selection.set",
            json!({"layer_ids":ids,"active_layer_id":ids[1]})
        ));
        let response = app
            .editor
            .session
            .apply(crate::application::Action::new(
                "mask.create",
                json!({"owner":{"kind":"layer","id":ids[0]},"source":"hide_all","select":false}),
            ))
            .unwrap();
        let mask = response["result"]["mask_id"].as_u64().unwrap();
        (app, ids, mask)
    }

    fn frame(
        app: &mut Studio,
        ctx: &Context,
        width: f32,
        events: Vec<Event>,
        time: f64,
    ) -> FullOutput {
        crate::i18n::set_language(app.editor.settings.ui.language);
        if !ctx.data_mut(|data| {
            data.get_temp::<bool>(Id::new("mask-test-fonts"))
                .unwrap_or(false)
        }) {
            theme::fonts(ctx);
            theme::apply(ctx, &app.editor.settings.ui);
            ctx.data_mut(|data| data.insert_temp(Id::new("mask-test-fonts"), true));
        }
        ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 1600.))),
                events,
                time: Some(time),
                focused: true,
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| app.editor.layers_ui(ui));
            },
        )
    }

    fn click(position: Pos2) -> Vec<Event> {
        vec![
            Event::PointerMoved(position),
            Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ]
    }

    fn raw_preview(shape: &Shape, rect: Rect, value: u8) -> bool {
        match shape {
            Shape::Mesh(mesh) => {
                mesh.vertices.len() == 16 * 16 * 4
                    && mesh.vertices.iter().all(|vertex| {
                        rect.contains(vertex.pos) && vertex.color == Color32::from_gray(value)
                    })
            }
            Shape::Vec(shapes) => shapes.iter().any(|shape| raw_preview(shape, rect, value)),
            _ => false,
        }
    }

    #[test]
    fn mask_and_pixel_thumbnails_select_targets_without_widening_the_layer_selection() {
        let (mut app, ids, mask) = fixture();
        let ctx = Context::default();
        for time in 0..3 {
            let _ = frame(&mut app, &ctx, 272., vec![], time as f64);
        }
        let mask_rect = ctx
            .data_mut(|data| {
                data.get_temp::<Rect>(Id::new((
                    "mask-thumbnail-control",
                    app.editor.session.doc.identity,
                    mask,
                )))
            })
            .unwrap();
        let selection = app.editor.session.doc.selected_layer_ids();
        let active = app.editor.session.doc.active;
        let _ = frame(&mut app, &ctx, 272., click(mask_rect.center()), 3.);
        assert_eq!(
            app.editor.session.doc.mask_state().target,
            PaintTarget::Mask { id: mask }
        );
        assert_eq!(app.editor.session.doc.active, active);
        assert_eq!(app.editor.session.doc.selected_layer_ids(), selection);
        let _ = frame(&mut app, &ctx, 272., vec![], 4.);
        let pixels = ctx
            .data_mut(|data| {
                data.get_temp::<Rect>(Id::new((
                    "layer-pixels-control",
                    app.editor.session.doc.identity,
                    ids[1],
                )))
            })
            .unwrap();
        let _ = frame(&mut app, &ctx, 272., click(pixels.center()), 5.);
        assert_eq!(
            app.editor.session.doc.mask_state().target,
            PaintTarget::LayerPixels
        );
        assert_eq!(app.editor.session.doc.active, active);
        assert_eq!(app.editor.session.doc.selected_layer_ids(), selection);
    }

    #[test]
    fn pixel_thumbnail_on_an_inactive_raster_uses_the_existing_layer_selection_path() {
        let (mut app, ids, mask) = fixture();
        app.editor
            .session
            .apply(crate::application::Action::new(
                "mask.target",
                json!({"target":{"kind":"mask","id":mask}}),
            ))
            .unwrap();
        let ctx = Context::default();
        for time in 0..3 {
            let _ = frame(&mut app, &ctx, 272., vec![], time as f64);
        }
        let pixels = ctx
            .data_mut(|data| {
                data.get_temp::<Rect>(Id::new((
                    "layer-pixels-control",
                    app.editor.session.doc.identity,
                    ids[0],
                )))
            })
            .unwrap();
        let _ = frame(&mut app, &ctx, 272., click(pixels.center()), 3.);
        assert_eq!(
            app.editor
                .session
                .doc
                .layer_id(app.editor.session.doc.active),
            ids[0]
        );
        assert_eq!(app.editor.session.doc.selected_layer_ids(), vec![ids[0]]);
        assert_eq!(
            app.editor.session.doc.mask_state().target,
            PaintTarget::LayerPixels
        );
        assert!(app.editor.session.doc.mask_plane(mask).is_ok());
    }

    #[test]
    fn group_mask_click_keeps_group_inspection_separate_from_the_active_raster() {
        let (mut app, ids, existing_mask) = fixture();
        // Structural group edits remain guarded for mask-bearing documents.
        // Establish the hierarchy before creating the group mask under test.
        app.editor
            .session
            .apply(crate::application::Action::new(
                "mask.delete",
                json!({"mask_id":existing_mask}),
            ))
            .unwrap();
        let group = app
            .editor
            .session
            .apply(crate::application::Action::new(
                "group.create",
                json!({"name":"Tone Folder","members":[{"kind":"layer","id":ids[0]}]}),
            ))
            .unwrap()["result"]["group_id"]
            .as_u64()
            .unwrap();
        let mask = app
            .editor
            .session
            .apply(crate::application::Action::new(
                "mask.create",
                json!({"owner":{"kind":"group","id":group},"source":"reveal_all","select":false}),
            ))
            .unwrap()["result"]["mask_id"]
            .as_u64()
            .unwrap();
        let ctx = Context::default();
        let selection = app.editor.session.doc.selected_layer_ids();
        let active = app
            .editor
            .session
            .doc
            .layer_id(app.editor.session.doc.active);
        for time in 0..3 {
            let _ = frame(&mut app, &ctx, 272., vec![], time as f64);
        }
        let rect = ctx
            .data_mut(|data| {
                data.get_temp::<Rect>(Id::new((
                    "mask-thumbnail-control",
                    app.editor.session.doc.identity,
                    mask,
                )))
            })
            .unwrap();
        let _ = frame(&mut app, &ctx, 272., click(rect.center()), 3.);
        assert_eq!(
            app.editor.session.doc.mask_state().target,
            PaintTarget::Mask { id: mask }
        );
        assert_eq!(app.editor.focused_group_ui(&ctx), Some(group));
        assert_eq!(app.editor.session.doc.selected_layer_ids(), selection);
        assert_eq!(
            app.editor
                .session
                .doc
                .layer_id(app.editor.session.doc.active),
            active
        );
        app.editor.focus_group_ui(&ctx, None);
        assert_eq!(
            app.editor.session.doc.mask_state().target,
            PaintTarget::Mask { id: mask }
        );
    }

    #[test]
    fn mask_pending_mutations_preserve_live_local_and_external_contacts() {
        for tool in ["brush", "eraser", "hand"] {
            let (mut app, _, mask) = fixture();
            assert!(app
                .editor
                .core("mask.target", json!({"target":{"kind":"mask","id":mask}})));
            assert!(app.editor.core("tool.select", json!({ "tool": tool })));
            app.editor
                .session
                .pointer(crate::application::Pointer {
                    phase: crate::application::PointerPhase::Down,
                    x: 8.,
                    y: 8.,
                    pressure: 1.,
                    time: 1,
                    pen: false,
                    eraser: false,
                    view_x: Some(8.),
                    view_y: Some(8.),
                    tool_override: None,
                })
                .unwrap();
            app.editor.input_owned = true;
            app.editor.drawing = tool != "hand";
            let state = app.editor.session.state();
            let history = app.editor.session.doc.history_bytes();
            for (command, args) in [
                ("mask.target", json!({"target":{"kind":"layer_pixels"}})),
                ("mask.set", json!({"mask_id":mask,"enabled":false})),
                ("mask.fill", json!({"mask_id":mask,"value":120})),
                ("mask.to_selection", json!({ "mask_id": mask })),
                ("mask.delete", json!({ "mask_id": mask })),
            ] {
                app.editor
                    .dispatch_layer_pending(vec![crate::application::Action::new(command, args)]);
                assert!(
                    app.editor.status.contains("stroke_active"),
                    "{tool}/{command}: {}",
                    app.editor.status
                );
                assert_eq!(app.editor.session.state(), state, "{tool}/{command}");
                assert_eq!(app.editor.session.doc.history_bytes(), history);
                assert!(app.editor.session.stroke.active());
                assert!(app.editor.session.pointer_checkpoint.is_some());
                assert!(app.editor.input_owned);
                assert_eq!(app.editor.drawing, tool != "hand");
            }
            app.editor.finish_stroke();
        }
    }

    #[test]
    fn mask_property_drafts_commit_once_on_release_and_reject_stale_documents() {
        let (mut app, _, mask) = fixture();
        let metadata = app.editor.session.doc.mask_state().items[0].clone();
        let ctx = Context::default();
        let key = draft_key(app.editor.session.doc.identity, mask);
        ctx.data_mut(|data| {
            data.insert_temp(
                key,
                PropertyDraft {
                    density: 170,
                    feather: 3,
                    revision: app.editor.session.revision,
                    dragging: true,
                    submitted: false,
                },
            )
        });
        let mut action = None;
        let _ = ctx.run(RawInput::default(), |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                action = app.editor.mask_property_fields_ui(ui, &metadata)
            });
        });
        let action = action.unwrap();
        assert_eq!(action.action, "mask.set");
        assert_eq!(
            action.args,
            json!({"mask_id":mask,"density":170,"feather_radius_px":3})
        );
        assert_eq!(action.expected_revision, Some(app.editor.session.revision));
        let _ = ctx.run(RawInput::default(), |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                assert!(app.editor.mask_property_fields_ui(ui, &metadata).is_none())
            });
        });
        assert!(app
            .editor
            .core("mask.set", json!({"mask_id":mask,"enabled":false})));
        let state = app.editor.session.state();
        let _ = ctx.run(RawInput::default(), |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                assert!(app.editor.mask_property_fields_ui(ui, &metadata).is_none())
            });
        });
        assert_eq!(app.editor.session.state(), state);
        assert!(ctx
            .data_mut(|data| data.get_temp::<PropertyDraft>(key))
            .is_some());
    }

    #[test]
    fn raw_mask_thumbnail_ignores_effects_and_all_localized_panel_widths_keep_state_immutable() {
        let (mut app, _, mask) = fixture();
        assert!(app
            .editor
            .core("mask.fill", json!({"mask_id":mask,"value":73})));
        assert!(app.editor.core(
            "mask.set",
            json!({"mask_id":mask,"enabled":false,"density":0,"feather_radius_px":2})
        ));
        assert!(app
            .editor
            .core("mask.target", json!({"target":{"kind":"mask","id":mask}})));
        for language in crate::i18n::Language::ALL {
            app.editor.settings.ui.language = language;
            for width in [176., 244., 560.] {
                let ctx = Context::default();
                let state = app.editor.session.state();
                let revision = app.editor.session.revision;
                let history = app.editor.session.doc.history_bytes();
                let mut output = frame(&mut app, &ctx, width, vec![], 0.);
                for time in 1..3 {
                    output = frame(&mut app, &ctx, width, vec![], time as f64);
                }
                assert_eq!(app.editor.session.state(), state, "{language:?}/{width}");
                assert_eq!(app.editor.session.revision, revision);
                assert_eq!(app.editor.session.doc.history_bytes(), history);
                assert!(matches!(
                    app.editor.session.doc.mask_plane(mask).unwrap(),
                    PlaneView::Uniform(73)
                ));
                let rect = ctx
                    .data_mut(|data| {
                        data.get_temp::<Rect>(Id::new((
                            "mask-thumbnail-control",
                            app.editor.session.doc.identity,
                            mask,
                        )))
                    })
                    .unwrap();
                assert!(
                    rect.left() >= 0. && rect.right() <= width,
                    "{language:?}/{width}: {rect:?}"
                );
                assert!(
                    output
                        .shapes
                        .iter()
                        .any(|shape| raw_preview(&shape.shape, rect, 73)),
                    "{language:?}/{width}: expected raw grayscale preview"
                );
            }
        }
        crate::i18n::set_language(crate::i18n::Language::English);
    }
}
