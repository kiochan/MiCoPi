//! Optional selected-only merge review; every plan and commit uses the shared actions.
use super::*;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
struct Plan {
    layer_ids: Vec<u64>,
    common_parent_id: u64,
    parent_index: usize,
    layer_id: u64,
    consumed_hidden_ids: Vec<u64>,
    interleaved_unselected_ids: Vec<u64>,
    changes_stacking: bool,
    output_buffer_bytes: u64,
    backdrop_policy: String,
}
pub(in crate::desktop) struct SelectedMergeDraft {
    identity: u64,
    revision: u64,
    targets: Vec<u64>,
    plan: Plan,
    names: std::collections::BTreeMap<u64, String>,
    parent_name: Option<String>,
    error: String,
}
impl SelectedMergeDraft {
    fn current(&self, editor: &Editor) -> bool {
        self.identity == editor.session.doc.identity && self.revision == editor.session.revision
    }
    fn name(&self, id: u64) -> String {
        self.names
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("#{id}"))
    }
    fn labels(&mut self, editor: &Editor) {
        self.names = (0..editor.session.doc.layers.len())
            .map(|i| {
                (
                    editor.session.doc.layer_id(i),
                    editor.session.doc.layer_name(i),
                )
            })
            .collect();
        self.parent_name = editor.session.doc.groups_state()["groups"]
            .as_array()
            .and_then(|groups| {
                groups
                    .iter()
                    .find(|group| group["id"].as_u64() == Some(self.plan.common_parent_id))
            })
            .and_then(|group| group["name"].as_str())
            .map(str::to_owned);
    }
}
impl Editor {
    pub(in crate::desktop) fn open_selected_merge(
        &mut self,
        value: Value,
        document_identity: u64,
        revision: u64,
    ) {
        match serde_json::from_value::<Plan>(value) {
            Ok(plan) if plan.backdrop_policy == "selected_only_transparent" => {
                let mut draft = SelectedMergeDraft {
                    identity: document_identity,
                    revision,
                    targets: plan.layer_ids.clone(),
                    plan,
                    names: Default::default(),
                    parent_name: None,
                    error: String::new(),
                };
                draft.labels(self);
                self.selected_merge = Some(draft);
            }
            Ok(_) => self.status = "Invalid selected-layer merge backdrop policy".into(),
            Err(error) => self.status = format!("Invalid selected-layer merge plan: {error}"),
        }
    }
    fn refresh_selected_merge(&mut self, draft: &mut SelectedMergeDraft) {
        if draft.identity != self.session.doc.identity {
            draft.error = "The document changed. Reopen this review.".into();
            return;
        }
        let mut action = crate::application::Action::new(
            "layer.merge_selected.plan",
            json!({"layer_ids":draft.targets}),
        );
        action.expected_revision = Some(self.session.revision);
        let result = self
            .session
            .apply(action)
            .map_err(|error| error.message)
            .and_then(|response| {
                serde_json::from_value::<Plan>(response["result"].clone())
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(plan) if plan.backdrop_policy == "selected_only_transparent" => {
                draft.plan = plan;
                draft.revision = self.session.revision;
                draft.labels(self);
                draft.error.clear();
            }
            Ok(_) => draft.error = "Invalid selected-layer merge backdrop policy".into(),
            Err(error) => draft.error = error,
        }
    }
    fn apply_selected_merge(&mut self, ctx: &Context, draft: &mut SelectedMergeDraft) -> bool {
        if !draft.current(self) {
            draft.error =
                "The document changed. Refresh the captured layer plan before merging.".into();
            return false;
        }
        let mut action = crate::application::Action::new(
            "layer.merge_selected",
            json!({"layer_ids":draft.targets}),
        );
        action.expected_revision = Some(draft.revision);
        match self.session.apply(action) {
            Ok(_) => {
                self.focus_group_ui(ctx, None);
                self.sync_from_session();
                true
            }
            Err(error) => {
                draft.error = error.message.clone();
                self.status = error.to_string();
                false
            }
        }
    }
    pub(in crate::desktop) fn selected_merge_window(&mut self, ctx: &Context) {
        let Some(mut draft) = self.selected_merge.take() else {
            return;
        };
        let a = self.settings.ui.clone();
        let screen = ctx.content_rect();
        let frame = Frame::window(&ctx.style()).inner_margin(Margin::same(a.panel_padding as i8));
        let width = (a.control_height * 22.).min(
            (screen.width() - a.outer_padding * 2. - frame.total_margin().sum().x)
                .max(a.control_height * 6.),
        );
        let stale = !draft.current(self);
        let contact = self.session.stroke.active() || self.session.pointer_checkpoint.is_some();
        let mut open = true;
        let mut apply = false;
        let mut refresh = false;
        let mut cancel = false;
        Window::new(tr("Merge Selected Layers")).id(Id::new("selected-layer-merge-review")).open(&mut open)
            .collapsible(false).resizable(false).frame(frame).min_width(width).max_width(width).default_width(width)
            .default_pos(pos2(screen.center().x-width*0.5,screen.top()+a.title_height*1.5)).show(ctx,|ui| {
                let footer_width=ui.available_width().floor().max(1.);
                let titles=["Refresh Plan","Cancel","Merge Selected Layers"];
                let font=TextStyle::Button.resolve(ui.style());
                let padding=ui.spacing().button_padding;
                let minimums=[4.,3.,6.];
                let mut sizes=[Vec2::ZERO;3];
                for (i,title) in titles.iter().enumerate() {
                    let text=ui.fonts_mut(|fonts|fonts.layout_no_wrap(localize(*title).into_owned(),font.clone(),Color32::WHITE)).size();
                    sizes[i]=vec2((text.x+padding.x*2.).max(a.control_height*minimums[i]).ceil(),(text.y+padding.y*2.).max(a.control_height).ceil());
                }
                let horizontal=sizes.iter().map(|size|size.x).sum::<f32>()+ui.spacing().item_spacing.x*2.<=footer_width;
                if !horizontal {
                    for (i,title) in titles.iter().enumerate() {
                        let text=ui.fonts_mut(|fonts|fonts.layout(localize(*title).into_owned(),font.clone(),Color32::WHITE,(footer_width-padding.x*2.).max(1.))).size();
                        sizes[i]=vec2(footer_width,(text.y+padding.y*2.).max(a.control_height).ceil());
                    }
                }
                let footer_height=if horizontal {sizes.iter().map(|size|size.y).fold(0.,f32::max)} else {sizes.iter().map(|size|size.y).sum::<f32>()+ui.spacing().item_spacing.y*2.};
                // Reserve the real footer before scrolling; a disabled UI scope cannot
                // participate reliably in a wrapping row's width allocation.
                let height=(screen.bottom()-a.outer_padding-ui.next_widget_position().y-footer_height-ui.spacing().item_spacing.y*3.-frame.total_margin().bottom)
                    .max(a.control_height).min(a.control_height*14.);
                ScrollArea::vertical().id_salt("selected-merge-plan").max_height(height).auto_shrink([false,true]).show(ui,|ui| {
                    ui.strong(tr("Merge only the captured layers over transparency."));
                    ui.weak(tr("Unselected layers stay editable. The visible result may change where other layers intervene or blends depend on their backdrop."));
                    ui.add_space(a.section_spacing);
                    ui.strong(format!("{} ({})",localize("Layers to Merge"),draft.plan.layer_ids.len()));
                    for id in &draft.plan.layer_ids {
                        ui.add(Label::new(draft.name(*id)).truncate()).on_hover_text(format!("{} · #{}",draft.name(*id),id));
                    }
                    ui.add_space(a.section_spacing);
                    let destination=if draft.plan.common_parent_id==0 {localize("Root Stack").into_owned()} else {draft.parent_name.clone().unwrap_or_else(||format!("#{}",draft.plan.common_parent_id))};
                    ui.label(format!("{}: {}",localize("Destination"),destination));
                    ui.weak(tr("Placed above the highest captured branch."));
                    ui.label(format!("{}: {}",localize("Result Layer"),draft.name(draft.plan.layer_id)));
                    ui.weak(format!("{}: {:.2} MiB",localize("Output Pixel Buffer"),draft.plan.output_buffer_bytes as f64/(1024.*1024.)));
                    if draft.plan.changes_stacking {
                        ui.colored_label(ui.visuals().warn_fg_color,tr("This selection changes stack placement when merged."));
                    }
                    if !draft.plan.consumed_hidden_ids.is_empty() {
                        ui.colored_label(ui.visuals().warn_fg_color,format!("{}: {}",localize("Hidden Captured Layers Will Be Removed"),draft.plan.consumed_hidden_ids.len()));
                    }
                    CollapsingHeader::new(tr("Plan Details")).show(ui,|ui| {
                        ui.label(format!("{}: {}",localize("Insertion Slot"),draft.plan.parent_index));
                        ui.label(format!("{}: {}",localize("Intervening Unselected Layers"),draft.plan.interleaved_unselected_ids.len()));
                        for id in &draft.plan.interleaved_unselected_ids {ui.add(Label::new(draft.name(*id)).truncate()).on_hover_text(draft.name(*id));}
                        for id in &draft.plan.consumed_hidden_ids {ui.add(Label::new(draft.name(*id)).truncate()).on_hover_text(draft.name(*id));}
                    });
                    if stale {ui.colored_label(ui.visuals().warn_fg_color,tr("The document changed. Refresh the captured layer plan before merging."));}
                    if contact {ui.weak(tr("Finish the current contact before merging selected layers."));}
                    if !draft.error.is_empty() {ui.colored_label(ui.visuals().error_fg_color,localize(&draft.error));}
                });
                ui.separator();
                let mut footer=|ui:&mut Ui| {
                    if ui.add_sized(sizes[0],Button::new(tr(titles[0])).wrap()).clicked() {refresh=true;}
                    if ui.add_sized(sizes[1],Button::new(tr(titles[1])).wrap()).clicked() {cancel=true;}
                    if ui.add_enabled(!stale && !contact && draft.error.is_empty(),Button::new(tr(titles[2])).min_size(sizes[2]).wrap()).clicked() {apply=true;}
                };
                if horizontal {ui.horizontal(&mut footer);} else {ui.vertical(&mut footer);}
            });
        if refresh {
            self.refresh_selected_merge(&mut draft);
        }
        if apply && self.apply_selected_merge(ctx, &mut draft) {
            open = false;
        }
        if open && !cancel {
            self.selected_merge = Some(draft);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{Pointer, PointerPhase};
    fn fixture() -> (Studio, Vec<u64>) {
        let mut app = crate::desktop::tests::studio();
        app.editor.session = crate::application::Session::new(
            crate::plugins::Registry::load(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
            )
            .unwrap(),
        )
        .unwrap();
        app.editor.session.runtime = "desktop";
        assert!(app
            .editor
            .core("canvas.new", json!({"width":12,"height":8})));
        for (i, color) in ["#cc3366", "#55aa77", "#6688ee"].into_iter().enumerate() {
            if i > 0 {
                assert!(app.editor.core("layer.add", json!({})));
            }
            assert!(app.editor.core("layer.fill_color", json!({"color":color})));
            assert!(app.editor.core(
                "layer.rename",
                json!({"name":format!("Artwork {} / 原画 / 캐릭터",i+1)})
            ));
        }
        let ids = (0..3)
            .map(|i| app.editor.session.doc.layer_id(i))
            .collect::<Vec<_>>();
        assert!(app.editor.core(
            "layer.selection.set",
            json!({"layer_ids":[ids[2],ids[0]],"active_layer_id":ids[2]})
        ));
        (app, ids)
    }
    fn open(app: &mut Studio, ctx: &Context, ids: &[u64]) {
        assert!(
            app.editor
                .core("layer.merge_selected.open", json!({"layer_ids":ids})),
            "{}",
            app.editor.status
        );
        app.consume_ui_effects(ctx);
        assert!(app.editor.selected_merge.is_some());
    }
    fn frame(app: &mut Studio, ctx: &Context, size: Vec2, time: f64) -> FullOutput {
        crate::i18n::set_language(app.editor.settings.ui.language);
        ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                time: Some(time),
                focused: true,
                ..Default::default()
            },
            |ctx| app.editor.selected_merge_window(ctx),
        )
    }
    #[test]
    fn selected_merge_review_is_inert_stale_guarded_and_refreshes_only_captured_ids() {
        let (mut app, ids) = fixture();
        let ctx = Context::default();
        let before = app.editor.session.state();
        let history = app.editor.session.doc.history_bytes();
        open(&mut app, &ctx, &[ids[2], ids[0]]);
        assert_eq!(app.editor.session.state(), before);
        assert_eq!(app.editor.session.doc.history_bytes(), history);
        assert_eq!(
            app.editor.selected_merge.as_ref().unwrap().targets,
            vec![ids[0], ids[2]]
        );
        assert!(app.editor.core(
            "layer.selection.set",
            json!({"layer_ids":[ids[1],ids[2]],"active_layer_id":ids[1]})
        ));
        let mut draft = app.editor.selected_merge.take().unwrap();
        let state = app.editor.session.state();
        assert!(!app.editor.apply_selected_merge(&ctx, &mut draft));
        assert_eq!(app.editor.session.state(), state);
        app.editor.refresh_selected_merge(&mut draft);
        assert!(draft.current(&app.editor));
        assert_eq!(draft.targets, vec![ids[0], ids[2]]);
        let raw = app
            .editor
            .session
            .doc
            .layers
            .iter()
            .map(|layer| layer.pixels.clone())
            .collect::<Vec<_>>();
        let before = app.editor.session.doc.composition();
        let untouched = app.editor.session.doc.layers[1].pixels.as_ptr();
        app.editor.session.replay_boundary().unwrap();
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        let revision = app.editor.session.revision;
        assert!(
            app.editor.apply_selected_merge(&ctx, &mut draft),
            "{}",
            draft.error
        );
        assert_eq!(app.editor.session.revision, revision + 1);
        assert_eq!(app.editor.session.doc.layers.len(), 2);
        let unselected = app.editor.session.doc.layer_index(ids[1]).unwrap();
        assert_eq!(
            app.editor.session.doc.layers[unselected].pixels.as_ptr(),
            untouched
        );
        assert_eq!(app.editor.session.doc.layers[unselected].pixels, raw[1]);
        assert_eq!(
            app.editor
                .session
                .doc
                .layer_id(app.editor.session.doc.active),
            ids[2]
        );
        let journal = app.editor.session.replay_journal(false).unwrap();
        assert_eq!(journal.entries.len(), events + 1);
        assert_eq!(
            journal.events(events, 1).unwrap()["events"][0]["source"]["args"],
            json!({"layer_ids":[ids[0],ids[2]]})
        );
        assert!(app.editor.core("history.undo", json!({})));
        assert_eq!(app.editor.session.doc.composition(), before);
        assert_eq!(
            app.editor
                .session
                .doc
                .layers
                .iter()
                .map(|layer| layer.pixels.clone())
                .collect::<Vec<_>>(),
            raw
        );
    }
    #[test]
    fn selected_merge_review_and_cancel_preserve_local_pressure_contacts() {
        for tool in [Tool::Brush, Tool::Hand] {
            let (mut app, ids) = fixture();
            let ctx = Context::default();
            let sample = Pointer {
                phase: PointerPhase::Down,
                x: 3.,
                y: 3.,
                pressure: 0.3,
                time: 0,
                pen: true,
                eraser: false,
                tool_override: Some(tool),
                view_x: Some(300.),
                view_y: Some(300.),
            };
            app.editor.session.pointer(sample).unwrap();
            app.editor.sync_from_session();
            app.editor.input_owned = true;
            app.editor.pen_active = true;
            app.editor.panning = (tool == Tool::Hand).then_some(pos2(300., 300.));
            let composition = app.editor.session.doc.composition();
            let history = app.editor.session.doc.history_bytes();
            let parameters = app.editor.session.state()["parameters"].clone();
            let events = app
                .editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len();
            open(&mut app, &ctx, &[ids[2], ids[0]]);
            for n in 0..3 {
                frame(&mut app, &ctx, vec2(800., 600.), n as f64);
            }
            assert!(
                app.editor.session.stroke.active()
                    && app.editor.session.pointer_checkpoint.is_some()
            );
            assert!(app.editor.input_owned && app.editor.pen_active && !app.editor.blocked);
            assert_eq!(app.editor.session.doc.composition(), composition);
            assert_eq!(app.editor.session.doc.history_bytes(), history);
            assert_eq!(app.editor.session.state()["parameters"], parameters);
            app.editor.actions.push(Action::Cancel);
            app.run_actions(&ctx);
            assert!(app.editor.selected_merge.is_none());
            assert!(app.editor.session.stroke.active());
            assert_eq!(app.editor.drawing, tool == Tool::Brush);
            assert_eq!(
                app.editor.panning,
                (tool == Tool::Hand).then_some(pos2(300., 300.))
            );
            app.editor
                .session
                .pointer(Pointer {
                    phase: PointerPhase::Move,
                    x: 5.,
                    pressure: 0.85,
                    time: 20,
                    ..sample
                })
                .unwrap();
            app.editor
                .session
                .pointer(Pointer {
                    phase: PointerPhase::Up,
                    x: 6.,
                    pressure: 0.4,
                    time: 40,
                    ..sample
                })
                .unwrap();
            let journal = app.editor.session.replay_journal(false).unwrap();
            let recorded = journal
                .events(events, journal.entries.len() - events)
                .unwrap();
            let samples = recorded["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|event| event["source"]["kind"] == "pointer")
                .flat_map(|event| event["source"]["samples"].as_array().unwrap().clone())
                .collect::<Vec<_>>();
            assert_eq!(samples.len(), 3);
            assert_eq!(samples[0], serde_json::to_value(sample).unwrap());
            assert_eq!(samples[1]["pressure"], json!(0.85f32));
            assert_eq!(samples[2]["pressure"], json!(0.4f32));
        }
    }
    #[test]
    fn selected_merge_review_footer_stays_visible_with_many_long_names_in_four_languages() {
        fn text(shape: &Shape, title: &str) -> Option<Rect> {
            match shape {
                Shape::Text(value) if value.galley.text() == title => {
                    Some(Rect::from_min_size(value.pos, value.galley.size()))
                }
                Shape::Vec(values) => values.iter().find_map(|value| text(value, title)),
                _ => None,
            }
        }
        let previous = crate::i18n::current();
        for language in crate::i18n::Language::ALL {
            for (width, height, font) in [
                (320., 360., 13.),
                (320., 520., 13.),
                (800., 620., 13.),
                (320., 600., 20.),
            ] {
                let (mut app, _) = fixture();
                while app.editor.session.doc.layers.len() < 16 {
                    assert!(app.editor.core("layer.add", json!({})));
                    assert!(app.editor.core(
                        "layer.rename",
                        json!({"name":"Long artwork title / 色彩と線画 / 캐릭터 원화".repeat(3)})
                    ));
                }
                let ids = (0..16)
                    .map(|i| app.editor.session.doc.layer_id(i))
                    .collect::<Vec<_>>();
                app.editor.settings.ui.language = language;
                app.editor.settings.ui.font_size = font;
                let ctx = Context::default();
                theme::fonts(&ctx);
                theme::apply(&ctx, &app.editor.settings.ui);
                let state = app.editor.session.state();
                open(&mut app, &ctx, &ids);
                let size = vec2(width, height);
                let mut output = None;
                for n in 0..4 {
                    output = Some(frame(&mut app, &ctx, size, n as f64));
                }
                let output = output.unwrap();
                for title in ["Refresh Plan", "Cancel", "Merge Selected Layers"] {
                    let translated = crate::i18n::text_in(language, title);
                    let (clip, rect) = output
                        .shapes
                        .iter()
                        .filter_map(|shape| {
                            text(&shape.shape, &translated).map(|rect| (shape.clip_rect, rect))
                        })
                        .max_by(|a, b| a.1.bottom().total_cmp(&b.1.bottom()))
                        .unwrap_or_else(|| panic!("{language:?}/{width}/{font}: missing {title}"));
                    assert!(
                        clip.contains_rect(rect)
                            && Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect),
                        "{language:?}/{width}/{font}: {title}: rect={rect:?}, clip={clip:?}"
                    );
                }
                assert_eq!(app.editor.session.state(), state);
            }
        }
        crate::i18n::set_language(previous);
    }
}
