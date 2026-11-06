use super::*;

fn event(viewport_id: ViewportId, user_data: UserData, color: egui::Color32) -> Event {
    Event::Screenshot {
        viewport_id,
        user_data,
        image: Arc::new(ColorImage::new([2, 1], vec![color; 2])),
    }
}
fn completion(request: &PendingCapture) -> capture_writer::Completion {
    capture_writer::Completion {
        request_id: request.request_id,
        viewport: ViewportId::ROOT,
        path: request.path.clone(),
        token: request.token.clone(),
        result: Ok([2, 1]),
    }
}
fn output(name: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-output/capture-root")
        .join(format!(
            "{}-{}-{name}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

#[test]
fn foreign_default_and_stale_frames_cannot_satisfy_the_current_request() {
    let old = PendingCapture::new("old.png".into(), Some(1));
    let mut current = PendingCapture::new("current.png".into(), Some(2));
    current.receive_root(&[
        event(ViewportId::ROOT, old.token, egui::Color32::RED),
        event(ViewportId::ROOT, UserData::default(), egui::Color32::RED),
        event(
            ViewportId::from_hash_of("foreign"),
            current.token.clone(),
            egui::Color32::RED,
        ),
        event(ViewportId::ROOT, UserData::new(()), egui::Color32::RED),
    ]);
    assert!(current.image.is_none());
    current.receive_root(&[event(
        ViewportId::ROOT,
        current.token.clone(),
        egui::Color32::GREEN,
    )]);
    assert_eq!(
        current.image.as_ref().unwrap().pixels,
        vec![egui::Color32::GREEN; 2]
    );
    current.receive_root(&[event(
        ViewportId::ROOT,
        current.token.clone(),
        egui::Color32::RED,
    )]);
    assert_eq!(
        current.image.as_ref().unwrap().pixels,
        vec![egui::Color32::GREEN; 2]
    );
}

#[test]
fn root_fifo_waits_for_the_matching_front_acknowledgement_exactly_once() {
    let first = PendingCapture::new("same.png".into(), Some(1));
    let second = PendingCapture::new("same.png".into(), Some(2));
    let first_result = completion(&first);
    let second_result = completion(&second);
    let mut pending = VecDeque::from([first, second]);
    assert!(acknowledge_root(&mut pending, &second_result).is_none());
    let mut foreign = completion(pending.front().unwrap());
    foreign.viewport = ViewportId::from_hash_of("foreign");
    assert!(acknowledge_root(&mut pending, &foreign).is_none());
    let mut incorrect_id = completion(pending.front().unwrap());
    incorrect_id.request_id = Some(100);
    assert!(acknowledge_root(&mut pending, &incorrect_id).is_none());
    let mut incorrect_path = completion(pending.front().unwrap());
    incorrect_path.path = "another.png".into();
    assert!(acknowledge_root(&mut pending, &incorrect_path).is_none());
    assert_eq!(pending.len(), 2);
    assert_eq!(
        acknowledge_root(&mut pending, &first_result)
            .unwrap()
            .request_id,
        Some(1)
    );
    assert!(acknowledge_root(&mut pending, &first_result).is_none());
    assert_eq!(pending.front().unwrap().request_id, Some(2));
    assert_eq!(
        acknowledge_root(&mut pending, &second_result)
            .unwrap()
            .request_id,
        Some(2)
    );
    assert!(pending.is_empty());
}

#[test]
fn preview_and_action_requests_have_independent_identity_and_shared_claims() {
    let action = PendingCapture::new("same.png".into(), Some(1));
    let mut preview = PendingCapture::new("same.png".into(), None);
    preview.receive_root(&[event(
        ViewportId::ROOT,
        action.token.clone(),
        egui::Color32::RED,
    )]);
    assert!(preview.image.is_none());
    let copy = action.clone();
    assert!(copy.matches(&action.token));
    assert!(!copy.claimed.swap(true, Ordering::AcqRel));
    assert!(action.claimed.load(Ordering::Acquire));
}

#[test]
fn screenshot_commands_always_target_root_once_from_a_foreign_context() {
    let mut request = PendingCapture::new("root.png".into(), Some(1));
    let foreign = ViewportId::from_hash_of("foreign");
    let mut input = egui::RawInput {
        viewport_id: foreign,
        ..Default::default()
    };
    input.viewports.insert(foreign, Default::default());
    let ctx = Context::default();
    let output = ctx.run(input, |ctx| {
        assert_eq!(ctx.viewport_id(), foreign);
        request.request_root(ctx);
        request.request_root(ctx);
    });
    assert_eq!(output.viewport_output[&ViewportId::ROOT].commands.iter().filter(|command|
        matches!(command, ViewportCommand::Screenshot(token) if *token == request.token)).count(), 1);
    assert!(output
        .viewport_output
        .get(&foreign)
        .is_none_or(|viewport| !viewport
            .commands
            .iter()
            .any(|command| matches!(command, ViewportCommand::Screenshot(_)))));
}

#[test]
fn accepted_root_fifo_saves_every_frame_and_publishes_terminal_status_without_document_edits() {
    let output = output("fifo");
    let mut app = crate::desktop::tests::studio();
    let before = app.editor.session.state();
    let history = app.editor.session.doc.history_bytes();
    let colors = [
        egui::Color32::RED,
        egui::Color32::GREEN,
        egui::Color32::BLUE,
    ];
    let mut ids = Vec::new();
    let mut paths = Vec::new();
    for (index, _) in colors.iter().enumerate() {
        let path = output.join(format!("{index}.png"));
        let id = app
            .editor
            .session
            .queue_capture(None, path.to_str().unwrap())
            .unwrap();
        app.queue_root_capture(path.clone(), Some(id));
        ids.push(id);
        paths.push(path);
    }
    let ctx = Context::default();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !app.pending_captures.is_empty() {
        assert!(
            Instant::now() < deadline,
            "Root capture worker did not acknowledge its queue"
        );
        let front = app.pending_captures.front().unwrap();
        let events = if front.claimed.load(Ordering::Acquire) {
            Vec::new()
        } else {
            let index = ids
                .iter()
                .position(|id| Some(*id) == front.request_id)
                .unwrap();
            vec![event(ViewportId::ROOT, front.token.clone(), colors[index])]
        };
        let _ = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| app.poll_captures(ctx),
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    for (index, path) in paths.iter().enumerate() {
        let image = image::open(path).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (2, 1));
        assert!(image.pixels().all(|p| p.0 == colors[index].to_array()));
    }
    let diagnostics = app.editor.session.capture_diagnostics();
    assert!(diagnostics["pending"].as_array().unwrap().is_empty());
    for id in ids {
        assert!(diagnostics["recent"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["request_id"] == id && row["status"] == "saved"));
    }
    assert_eq!(app.editor.session.state(), before);
    assert_eq!(app.editor.session.doc.history_bytes(), history);
}

#[test]
fn timeout_and_encoding_failure_finish_explicitly_and_do_not_discard_the_next_capture() {
    let output = output("failures");
    let mut app = crate::desktop::tests::studio();
    let before = app.editor.session.state();
    let blocked = output.join("destination-directory");
    std::fs::create_dir_all(&blocked).unwrap();
    let paths = [
        output.join("expired.png"),
        blocked.clone(),
        output.join("valid.png"),
    ];
    let ids: Vec<_> = paths
        .iter()
        .map(|path| {
            let id = app
                .editor
                .session
                .queue_capture(None, path.to_str().unwrap())
                .unwrap();
            app.queue_root_capture(path.clone(), Some(id));
            id
        })
        .collect();
    app.pending_captures.front_mut().unwrap().created =
        Instant::now() - CAPTURE_TIMEOUT - Duration::from_secs(1);
    let ctx = Context::default();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !app.pending_captures.is_empty() {
        assert!(
            Instant::now() < deadline,
            "Failed captures stopped FIFO progress"
        );
        let front = app.pending_captures.front().unwrap();
        let events = if front.claimed.load(Ordering::Acquire) {
            Vec::new()
        } else {
            vec![event(
                ViewportId::ROOT,
                front.token.clone(),
                egui::Color32::WHITE,
            )]
        };
        let _ = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| app.poll_captures(ctx),
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let diagnostics = app.editor.session.capture_diagnostics();
    for (index, status) in ["failed", "failed", "saved"].into_iter().enumerate() {
        let record = diagnostics["recent"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["request_id"] == ids[index])
            .unwrap();
        assert_eq!(record["status"], status);
        if status == "failed" {
            assert!(!record["error"].as_str().unwrap().is_empty());
        }
    }
    assert!(!paths[0].exists());
    assert!(blocked.is_dir());
    assert_eq!(
        image::open(&paths[2]).unwrap().to_rgba8().dimensions(),
        (2, 1)
    );
    assert_eq!(app.editor.session.state(), before);
}

#[test]
fn legacy_root_capture_admission_is_bounded_without_overwriting_accepted_requests() {
    let mut app = crate::desktop::tests::studio();
    for index in 0..CAPTURE_QUEUE_LIMIT {
        app.queue_root_capture(format!("legacy-{index}.png").into(), None);
    }
    let first = app.pending_captures.front().unwrap().token.clone();
    app.queue_root_capture("overflow.png".into(), None);
    assert_eq!(app.pending_captures.len(), CAPTURE_QUEUE_LIMIT);
    assert!(app.pending_captures.front().unwrap().matches(&first));
    assert_eq!(app.editor.status, "Workspace capture queue is full");
}

#[test]
fn preview_timeout_closes_without_waiting_forever_or_creating_a_file() {
    let output = output("preview-timeout").join("absent.png");
    let mut app = crate::desktop::tests::studio();
    let mut preview = PendingCapture::new(output.clone(), None);
    preview.created = Instant::now() - CAPTURE_TIMEOUT - Duration::from_secs(1);
    app.preview = Some(preview);
    let ctx = Context::default();
    let _ = ctx.run(Default::default(), |ctx| app.poll_captures(ctx));
    assert!(app.preview.is_none());
    assert!(app.allow_close);
    assert!(!output.exists());
    assert_eq!(
        app.editor.status,
        "Preview capture timed out before readback"
    );
}
