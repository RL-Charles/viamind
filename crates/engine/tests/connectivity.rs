//! Immutable electrical membership and bounded deterministic guide calculations.
use serde_json::json;
use viamind_doc::{ObjectId, Pad, Pads, Part, Project, Side};
use viamind_engine::{ConnectivityQuery, EngineHandle, NetQuery, Origin, connection_guides};
use viamind_geom::{Nm, Point};

fn part(i: usize, x: f64, y: f64, net: Option<&str>) -> Part {
    Part {
        id: ObjectId::new(),
        designator: format!("R{i}"),
        value: "1k".into(),
        footprint: "R_0603".into(),
        position: Point::from_mm(x, y),
        rotation_mdeg: 0,
        side: Side::Top,
        pads: Some(
            Pads::try_from(vec![Pad {
                id: ObjectId::new(),
                number: "1".into(),
                position: Point::ORIGIN,
                width: Nm::from_mm(1.0),
                height: Nm::from_mm(1.0),
                net: net.map(String::from),
            }])
            .unwrap(),
        ),
    }
}
fn query(net: &str, offset: usize, limit: usize) -> ConnectivityQuery {
    ConnectivityQuery { net: net.into(), offset, limit }
}
#[test]
fn exact_tree_membership_pages_transforms_and_read_only_state() {
    let mut project = Project::new("Membership");
    for (i, (x, y)) in [(0.0, 0.0), (3.0, 0.0), (0.0, 4.0), (3.0, 4.0)].into_iter().enumerate() {
        let p = part(i, x, y, Some("GND"));
        project.board.parts.insert(p.id, p);
    }
    for (i, net) in [(10, Some("gnd")), (11, None)] {
        let p = part(i, 0.0, 0.0, net);
        project.board.parts.insert(p.id, p);
    }
    let mut legacy = part(12, 0.0, 0.0, Some("GND"));
    legacy.pads = None;
    project.board.parts.insert(legacy.id, legacy);
    let engine = EngineHandle::new(project);
    let snapshot = engine.session_state();
    let events = engine.subscribe();
    let page = engine.query_nets(&NetQuery { after: None, limit: 1 }).unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.nets[0].name, "GND");
    assert_eq!(page.nets[0].guide_count, 3);
    assert_eq!(engine.query_nets(&NetQuery { after: page.next_after, limit: 1 }).unwrap().nets[0].name, "gnd");
    let all = engine.query_connectivity(&query("GND", 0, 100)).unwrap();
    assert_eq!(all.algorithm, "dense_prim");
    assert_eq!(all.terminal_count, 4);
    assert_eq!(all.guide_count, 3);
    assert!((all.guides.iter().map(|e| e.length_mm).sum::<f64>() - 10.0).abs() < 1e-9);
    let first = engine.query_connectivity(&query("GND", 0, 2)).unwrap();
    let second = engine.query_connectivity(&query("GND", first.next_offset.unwrap(), 2)).unwrap();
    assert_eq!([first.terminals, second.terminals].concat(), all.terminals);
    assert_eq!([first.guides, second.guides].concat(), all.guides);
    assert!(second.next_offset.is_none());
    assert_eq!(engine.query_connectivity(&query("GND", usize::MAX, 1)).unwrap().next_offset, None);
    assert!(engine.query_connectivity(&query("GND", 0, 0)).is_err());
    assert!(engine.query_nets(&NetQuery { after: None, limit: 1001 }).is_err());
    assert!(engine.query_connectivity(&query("missing", 0, 100)).unwrap().terminals.is_empty());
    assert_eq!(engine.session_state(), snapshot);
    assert!(events.try_recv().is_err());
    assert!(engine.next_undo().is_none());
    let id = all.terminals[0].part_id;
    engine.execute("update_part", json!({"id":id,"x_mm":20,"y_mm":30,"rotation_deg":45,"side":"bottom"}), Origin::User).unwrap();
    // The local centre is zero; moving the part moves exactly that query terminal.
    let moved = engine.query_connectivity(&query("GND", 0, 100)).unwrap();
    let moved = moved.terminals.iter().find(|t| t.part_id == id).unwrap();
    assert_eq!((moved.x_mm, moved.y_mm), (20.0, 30.0));
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.query_connectivity(&query("GND", 0, 100)).unwrap(), all);
}
#[test]
fn large_nets_are_deterministic_connected_linear_size_even_for_coincident_pads() {
    let mut project = Project::new("Large net");
    for i in 0..4096 {
        let p = part(i, (i % 64) as f64, (i / 64) as f64, Some("BUS"));
        project.board.parts.insert(p.id, p);
    }
    let guides = connection_guides(&project);
    assert_eq!(guides.len(), 4095);
    assert_eq!(guides, connection_guides(&project));
    let page = query("BUS", 0, 1000).run(&project).unwrap();
    assert_eq!(page.algorithm, "coordinate_chain");
    assert_eq!(page.guides.len(), 1000);
    let mut reached = std::collections::BTreeSet::new();
    for e in &guides {
        reached.insert((e.from.part_id, e.from.pad_id));
        reached.insert((e.to.part_id, e.to.pad_id));
    }
    assert_eq!(reached.len(), 4096);
    // Chain ordering implies each successive edge starts where the previous edge ends.
    assert!(guides.windows(2).all(|w| w[0].to.pad_id == w[1].from.pad_id));
    for p in project.board.parts.values_mut() {
        p.position = Point::ORIGIN;
    }
    let guides = connection_guides(&project);
    assert_eq!(guides.len(), 4095);
    assert!(guides.iter().all(|e| e.length_mm == 0.0));
}
#[test]
fn mirrored_arbitrary_rotation_uses_the_shared_query_render_transform() {
    let mut p = part(1, 10.0, 20.0, Some("NET"));
    p.rotation_mdeg = 45000;
    p.side = Side::Bottom;
    p.pads.as_mut().unwrap().0[0].position = Point::from_mm(2.0, 1.0);
    let (x, y) = p.local_to_world_mm(2.0, 1.0);
    assert!((x - (10.0 - std::f64::consts::FRAC_1_SQRT_2)).abs() < 1e-9);
    assert!((y - (20.0 + 3.0 * std::f64::consts::FRAC_1_SQRT_2)).abs() < 1e-9);
    let mut project = Project::new("transform");
    project.board.parts.insert(p.id, p);
    let result = query("NET", 0, 100).run(&project).unwrap();
    assert_eq!((result.terminals[0].x_mm, result.terminals[0].y_mm), (x, y));
}
