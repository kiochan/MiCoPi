//! Framing must preserve stored pixels, graph identity, history and recorded outcomes.
use super::editing_tests::{call, check, session};
use super::Action;
use serde_json::json;

#[test]
fn all_canvas_anchors_copy_exact_pixels_across_odd_expansion_and_contraction() {
    let anchors = [
        "top_left",
        "top",
        "top_right",
        "left",
        "center",
        "right",
        "bottom_left",
        "bottom",
        "bottom_right",
    ];
    for (i, anchor) in anchors.iter().enumerate() {
        let mut s = session();
        call(&mut s, "native.checkerboard", json!({"cell":2}));
        let original = s.doc.layers[0].pixels.clone();
        for (w, h, offsets_x, offsets_y) in [
            (17, 11, [0i32, 2, 5], [0i32, 1, 3]),
            (7, 5, [0, -3, -5], [0, -2, -3]),
        ] {
            call(
                &mut s,
                "canvas.resize",
                json!({"width":w,"height":h,"anchor":anchor}),
            );
            let dx = offsets_x[i % 3];
            let dy = offsets_y[i / 3];
            for y in 0..h {
                for x in 0..w {
                    let sx = x as i32 - dx;
                    let sy = y as i32 - dy;
                    let expected = if (0..12).contains(&sx) && (0..8).contains(&sy) {
                        original[sy as usize * 12 + sx as usize]
                    } else {
                        0
                    };
                    assert_eq!(
                        s.doc.layers[0].pixels[y * w + x],
                        expected,
                        "{anchor}: {x},{y}"
                    );
                }
            }
            check(&mut s);
            call(&mut s, "history.undo", json!({}));
            assert_eq!((s.doc.width, s.doc.height), (12, 8));
            assert_eq!(s.doc.layers[0].pixels, original);
            call(&mut s, "history.redo", json!({}));
            assert_eq!((s.doc.width, s.doc.height), (w, h));
            check(&mut s);
            call(&mut s, "history.undo", json!({}));
        }
    }
}

#[test]
fn crop_preserves_hidden_locked_layers_graphs_and_projects_and_restores_clipped_pixels() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    call(
        &mut s,
        "graph.set",
        json!({"graph":{"enabled":true,"linked":true,"nodes":[
        {"id":0,"kind":"layer","index":0,"position":[0,0]},
        {"id":1,"kind":"filter","filter":"invert","amount":0.5,"position":[200,0]},
        {"id":2,"kind":"output","position":[400,0]}],"edges":[{"from":0,"to":1,"input":0},{"from":1,"to":2,"input":0}]}}),
    );
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "layer.fill_color", json!({"color":"#12345680"}));
    call(
        &mut s,
        "layer.set",
        json!({"alpha_locked":true,"visible":false,"opacity":0.4}),
    );
    let original: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.clone()).collect();
    let metadata = s.doc.composition();
    let identity = s.doc.identity;
    call(
        &mut s,
        "canvas.crop",
        json!({"x":3,"y":2,"width":7,"height":5}),
    );
    assert_eq!(s.doc.identity, identity);
    assert_eq!(s.doc.composition(), metadata);
    assert_eq!(s.doc.active, 1);
    for (layer, before) in s.doc.layers.iter().zip(&original) {
        let expected: Vec<_> = (2..7)
            .flat_map(|y| before[y * 12 + 3..y * 12 + 10].iter().copied())
            .collect();
        assert_eq!(layer.pixels, expected);
    }
    check(&mut s);
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-output/canvas-editing");
    std::fs::create_dir_all(&root).unwrap();
    let project = root.join(format!("{}-crop.mcp", std::process::id()));
    call(&mut s, "document.save", json!({"path":project}));
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":project}));
    assert_eq!((reopened.doc.width, reopened.doc.height), (7, 5));
    assert_eq!(reopened.doc.composition(), metadata);
    assert_eq!(reopened.doc.layers[1].pixels, s.doc.layers[1].pixels);
    check(&mut reopened);
    call(&mut s, "history.undo", json!({}));
    assert_eq!((s.doc.width, s.doc.height), (12, 8));
    for (layer, before) in s.doc.layers.iter().zip(&original) {
        assert_eq!(&layer.pixels, before);
    }
    check(&mut s);
    call(&mut s, "history.redo", json!({}));
    check(&mut s);
}

#[test]
fn failed_atomic_geometry_restores_dimensions_pixels_renderer_and_prior_history() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    let original = s.doc.layers[0].pixels.clone();
    call(
        &mut s,
        "canvas.resize",
        json!({"width":67,"height":65,"anchor":"bottom_right"}),
    );
    let before = s.state();
    let pixels = s.doc.layers[0].pixels.clone();
    let history = s.doc.history_bytes();
    let outcome = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"resize","action":"canvas.resize","args":{"width":70,"height":68}},
            {"id":"crop","action":"canvas.crop","args":{"x":2,"y":3,"width":65,"height":64}},
            {"id":"layer","action":"layer.duplicate","args":{}},
            {"id":"bad","action":"canvas.crop","args":{"x":64,"y":0,"width":2,"height":2}}
        ]}),
    );
    assert_eq!(outcome["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    assert!(s.doc.renderer.is_some());
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!((s.doc.width, s.doc.height), (12, 8));
    assert_eq!(s.doc.layers[0].pixels, original);
    call(&mut s, "history.undo", json!({}));
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0));
    call(&mut s, "history.redo", json!({}));
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, pixels);
    check(&mut s);
}

#[test]
fn geometry_rejects_invalid_bounds_budgets_and_stale_revisions_and_noops_keep_history() {
    let mut s = session();
    for _ in 0..4 {
        call(&mut s, "layer.add", json!({}));
    }
    let before = s.state();
    let history = s.doc.history_bytes();
    for (id, args) in [
        ("canvas.crop", json!({"x":11,"y":0,"width":2,"height":1})),
        ("canvas.crop", json!({"x":0,"y":8,"width":1,"height":1})),
        ("canvas.resize", json!({"width":4096,"height":4096})),
        ("canvas.resize", json!({"width":10.5,"height":8})),
        ("canvas.resize", json!({"width":0,"height":8})),
        (
            "canvas.resize",
            json!({"width":10,"height":8,"anchor":"invalid"}),
        ),
    ] {
        assert!(s.apply(Action::new(id, args)).is_err());
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.history_bytes(), history);
    }
    let mut action = Action::new("canvas.resize", json!({"width":15,"height":12}));
    action.expected_revision = Some(s.revision + 1);
    assert!(s.apply(action).is_err());
    assert_eq!(s.state(), before);
    call(&mut s, "canvas.resize", json!({"width":12.0,"height":8.0}));
    call(&mut s, "canvas.resize", json!({"width":12,"height":8}));
    call(
        &mut s,
        "canvas.crop",
        json!({"x":0,"y":0,"width":12,"height":8}),
    );
    assert_eq!(s.doc.history_bytes(), history);
    check(&mut s);
}
