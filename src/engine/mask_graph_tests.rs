use super::*;
fn node(id: u32, data: NodeKind) -> Node {
    Node {
        id,
        data,
        position: [0., 0.],
        node_type: None,
    }
}
fn edge(from: u32, to: u32, input: u32) -> Edge {
    Edge { from, to, input }
}
fn graph() -> Graph {
    Graph {
        enabled: true,
        linked: false,
        nodes: vec![
            node(10, NodeKind::Layer { index: 0 }),
            node(20, NodeKind::Layer { index: 1 }),
            node(30, NodeKind::MaskSource { mask_id: 99 }),
            node(40, NodeKind::MaskedMix { opacity: 0.5 }),
            node(50, NodeKind::Output),
        ],
        edges: vec![
            edge(10, 40, 0),
            edge(20, 40, 1),
            edge(30, 40, 2),
            edge(40, 50, 0),
        ],
    }
}
#[test]
fn stable_mask_ids_resolve_only_in_the_supplied_descriptor_context() {
    let value = graph();
    assert!(value
        .compile(2)
        .err()
        .unwrap()
        .contains("descriptor context"));
    let encoded = serde_json::to_value(&value).unwrap();
    let decoded: Graph = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, value);
    let plan = value.compile_with_masks(2, &[42, 99]).unwrap();
    assert_eq!(plan.len(), 5);
    assert_eq!(
        (
            plan[2].kind,
            plan[2].a,
            plan[2].b,
            plan[2].mode,
            plan[2].amount
        ),
        (6, 1, 0, 0, 0.)
    );
    assert_eq!(
        (
            plan[3].kind,
            plan[3].a,
            plan[3].b,
            plan[3].mode,
            plan[3].amount
        ),
        (8, 0, 1, 2, 0.5)
    );
    assert_eq!(NodeKind::MaskedMix { opacity: 1. }.inputs(), 3);
    assert_eq!(
        NodeKind::ApplyMask { opacity: 1. }.input_domain(1),
        Some(PortDomain::Coverage)
    );
    assert_eq!(
        NodeKind::MaskSource { mask_id: 99 }.output_domain(),
        PortDomain::Coverage
    );
    assert_eq!(NodeKind::MaskedMix { opacity: 1. }.input_domain(3), None);
    for ids in [
        vec![0, 99],
        vec![99, 99],
        vec![u32::MAX as u64 + 1, 99],
        vec![42],
        (1..=33).collect(),
    ] {
        assert!(value.compile_with_masks(2, &ids).is_err(), "{ids:?}");
    }
}
#[test]
fn coverage_domains_are_checked_on_reachable_and_disconnected_edges() {
    let mut value = graph();
    value.edges[2].from = 10;
    assert!(value
        .compile_with_masks(2, &[99])
        .err()
        .unwrap()
        .contains("not interchangeable"));
    let mut value = graph();
    value.edges[0].from = 30;
    assert!(value
        .compile_with_masks(2, &[99])
        .err()
        .unwrap()
        .contains("not interchangeable"));
    let mut value = graph();
    value.nodes.extend([
        node(
            70,
            NodeKind::Filter {
                filter: Filter::Brightness,
                amount: 0.2,
            },
        ),
        node(80, NodeKind::ApplyMask { opacity: 1. }),
    ]);
    value.edges.push(edge(30, 70, 0));
    assert!(value
        .compile_with_masks(2, &[99])
        .err()
        .unwrap()
        .contains("not interchangeable"));
    value.edges.pop();
    value.edges.push(edge(20, 80, 1));
    assert!(value
        .compile_with_masks(2, &[99])
        .err()
        .unwrap()
        .contains("not interchangeable"));
    value.edges.pop();
    value
        .nodes
        .push(node(90, NodeKind::MaskSource { mask_id: 1 }));
    assert!(value
        .compile_with_masks(2, &[99])
        .err()
        .unwrap()
        .contains("missing raster mask"));
    value.nodes.pop();
    value.nodes[6].data = NodeKind::ApplyMask { opacity: f32::NAN };
    assert!(value
        .compile_with_masks(2, &[99])
        .err()
        .unwrap()
        .contains("Invalid blend opacity"));
}
#[test]
fn required_third_ports_and_image_output_remain_strict() {
    let mut value = graph();
    value.edges.remove(2);
    assert!(value
        .compile_with_masks(2, &[99])
        .err()
        .unwrap()
        .contains("not connected"));
    let mut value = graph();
    value.edges[3].from = 30;
    assert!(value
        .compile_with_masks(2, &[99])
        .err()
        .unwrap()
        .contains("not interchangeable"));
    let mut value = graph();
    value.edges.push(edge(10, 40, 3));
    assert!(value.compile_with_masks(2, &[99]).is_err());
    let mut value = graph();
    value
        .nodes
        .push(node(60, NodeKind::ApplyMask { opacity: 1. }));
    value.edges.extend([edge(60, 60, 0), edge(30, 60, 1)]);
    assert!(value
        .compile_with_masks(2, &[99])
        .err()
        .unwrap()
        .contains("cycle"));
}
