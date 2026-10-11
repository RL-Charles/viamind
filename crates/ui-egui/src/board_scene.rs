//! Builds the canvas [`Scene`] from the design model, so the canvas shows the live design.
//!
//! Footprints are drawn from a small built-in table of common packages until the library epic (E4)
//! brings real footprints; unknown footprint names fall back to a generic two-pad part.

use viamind_doc::{Part, Project, Side};

use crate::camera::{WorldPos, WorldRect};
use crate::layers::LayerId;
use crate::scene::{Item, Prim, Scene};

/// Board size used until the model has a board outline (E6-S1).
pub const DEFAULT_BOARD_MM: (f64, f64) = (100.0, 80.0);

/// Pads (centre x, centre y, width, height) and body size in mm, for a part at the origin, unrotated.
struct Footprint {
    pads: Vec<(f64, f64, f64, f64)>,
    body: (f64, f64),
}

fn two_pad(pitch: f64, pad_w: f64, pad_h: f64, body: (f64, f64)) -> Footprint {
    Footprint { pads: vec![(-pitch / 2.0, 0.0, pad_w, pad_h), (pitch / 2.0, 0.0, pad_w, pad_h)], body }
}

fn footprint(name: &str) -> Footprint {
    let n = name.to_ascii_uppercase();
    if n.contains("SOIC-8") || n.contains("SOIC8") {
        let pads = [-1.905, -0.635, 0.635, 1.905].iter().flat_map(|&y| [(-2.7, y, 1.55, 0.6), (2.7, y, 1.55, 0.6)]).collect();
        return Footprint { pads, body: (3.9, 4.9) };
    }
    if n.contains("SOT-23") || n.contains("SOT23") {
        return Footprint { pads: vec![(-0.95, 1.0, 0.8, 0.9), (0.95, 1.0, 0.8, 0.9), (0.0, -1.0, 0.8, 0.9)], body: (3.0, 1.4) };
    }
    if n.contains("0402") {
        return two_pad(1.0, 0.6, 0.5, (1.0, 0.5));
    }
    if n.contains("0603") {
        return two_pad(1.6, 0.9, 0.95, (1.6, 0.8));
    }
    if n.contains("0805") {
        return two_pad(1.9, 1.0, 1.3, (2.0, 1.25));
    }
    if n.contains("1206") {
        return two_pad(3.0, 1.1, 1.8, (3.2, 1.6));
    }
    two_pad(2.0, 1.0, 1.0, (2.5, 1.5))
}

/// Places a footprint-local point in the world: mirror for the bottom side, rotate, then translate.
fn place(part: &Part, x: f64, y: f64) -> WorldPos {
    let (x, y) = part.local_to_world_mm(x, y);
    WorldPos::new(x, y)
}

/// True when the part is rotated by roughly 90° or 270°, so rectangular pads swap width and height.
fn quarter_turned(part: &Part) -> bool {
    let quarter = (f64::from(part.rotation_mdeg) / 90_000.0).round() as i64;
    quarter.rem_euclid(2) == 1
}

/// The canvas items for one part: copper pads, solder-mask openings and a silkscreen outline.
pub fn part_items(part: &Part) -> Vec<Item> {
    let fp = footprint(&part.footprint);
    let (copper, mask, silk) = if part.side == Side::Bottom {
        (LayerId::B_CU, LayerId::B_MASK, LayerId::B_SILKS)
    } else {
        (LayerId::F_CU, LayerId::F_MASK, LayerId::F_SILKS)
    };
    let swap = quarter_turned(part);
    let mut items = Vec::with_capacity(fp.pads.len() * 2 + 4);
    if let Some(pads) = &part.pads {
        for p in &pads.0 {
            let (x, y, w, h) = (p.position.x.to_mm(), p.position.y.to_mm(), p.width.to_mm(), p.height.to_mm());
            for (margin, layer) in [(0.0, copper), (0.05, mask)] {
                let (hw, hh) = (w / 2.0 + margin, h / 2.0 + margin);
                let corners = [place(part, x - hw, y - hh), place(part, x + hw, y - hh), place(part, x + hw, y + hh), place(part, x - hw, y + hh)];
                items.push(Item::new(Prim::Quad(corners), layer));
            }
        }
    } else {
        for &(x, y, w, h) in &fp.pads {
            let center = place(part, x, y);
            let (hw, hh) = if swap { (h / 2.0, w / 2.0) } else { (w / 2.0, h / 2.0) };
            let pad = WorldRect::new(WorldPos::new(center.x - hw, center.y - hh), WorldPos::new(center.x + hw, center.y + hh));
            items.push(Item::new(Prim::Rect(pad), copper));
            items.push(Item::new(Prim::Rect(pad.expand(0.05)), mask));
        }
    }
    let (bw, bh) = (fp.body.0 / 2.0, fp.body.1 / 2.0);
    let corners = [place(part, -bw, -bh), place(part, bw, -bh), place(part, bw, bh), place(part, -bw, bh)];
    for i in 0..4 {
        items.push(Item::new(Prim::Segment { a: corners[i], b: corners[(i + 1) % 4], width: 0.12 }, silk));
    }
    items
}

/// The board outline: the default board, grown to include every part with a 5 mm margin.
pub fn board_outline(project: &Project) -> WorldRect {
    if let Some(outline) = &project.board.outline
        && let Some(bounds) = outline.outer.bbox()
    {
        return WorldRect::new(WorldPos::new(bounds.min.x.to_mm(), bounds.min.y.to_mm()), WorldPos::new(bounds.max.x.to_mm(), bounds.max.y.to_mm()));
    }
    let mut board = WorldRect::new(WorldPos::new(0.0, 0.0), WorldPos::new(DEFAULT_BOARD_MM.0, DEFAULT_BOARD_MM.1));
    for part in project.board.parts.values() {
        let p = WorldPos::new(part.position.x.to_mm(), part.position.y.to_mm());
        board = WorldRect::new(
            WorldPos::new(board.min.x.min(p.x - 5.0), board.min.y.min(p.y - 5.0)),
            WorldPos::new(board.max.x.max(p.x + 5.0), board.max.y.max(p.y + 5.0)),
        );
    }
    board
}

/// The whole design as a canvas scene: the board outline on Edge.Cuts plus every part.
pub fn scene_from_project(project: &Project) -> Scene {
    let board = board_outline(project);
    let mut items = Vec::new();
    let substrate = if let Some(outline) = &project.board.outline {
        for ring in outline.rings() {
            for edge in ring.edges() {
                let world = |p: viamind_geom::Point| WorldPos::new(p.x.to_mm(), p.y.to_mm());
                items.push(Item::new(Prim::Segment { a: world(edge.a), b: world(edge.b), width: 0.2 }, LayerId::EDGE_CUTS));
            }
        }
        Some(substrate_triangles(outline))
    } else {
        let corners = [board.min, WorldPos::new(board.max.x, board.min.y), board.max, WorldPos::new(board.min.x, board.max.y)];
        items.extend((0..4).map(|i| Item::new(Prim::Segment { a: corners[i], b: corners[(i + 1) % 4], width: 0.2 }, LayerId::EDGE_CUTS)));
        None
    };
    for part in project.board.parts.values() {
        items.extend(part_items(part));
    }
    for track in project.board.tracks.values() {
        let world = |p: viamind_geom::Point| WorldPos::new(p.x.to_mm(), p.y.to_mm());
        let (a, b, width) = (world(track.start), world(track.end), track.width.to_mm());
        let layer = match track.layer {
            viamind_doc::CopperLayer::Top => LayerId::F_CU,
            viamind_doc::CopperLayer::Bottom => LayerId::B_CU,
        };
        items.push(Item::new(Prim::Segment { a, b, width }, layer));
        for center in [a, b] {
            items.push(Item::new(Prim::Circle { center, radius: width / 2.0 }, layer));
        }
    }
    for via in project.board.vias.values() {
        items.push(Item::new(
            Prim::Via {
                center: WorldPos::new(via.position.x.to_mm(), via.position.y.to_mm()),
                radius: via.diameter.to_mm() / 2.0,
                drill_radius: via.drill.to_mm() / 2.0,
            },
            LayerId::VIAS,
        ));
    }
    let ratsnest = viamind_engine::connection_guides(project)
        .into_iter()
        .map(|edge| [WorldPos::new(edge.from.x_mm, edge.from.y_mm), WorldPos::new(edge.to.x_mm, edge.to.y_mm)])
        .collect();
    Scene { board, substrate, items, ratsnest }
}

/// Triangulate validated polygons once while rebuilding a scene.
pub fn substrate_triangles(outline: &viamind_doc::BoardOutline) -> Vec<[WorldPos; 3]> {
    let mut vertices = Vec::new();
    let mut holes = Vec::new();
    for (i, ring) in outline.rings().enumerate() {
        if i > 0 {
            holes.push(vertices.len() / 2);
        }
        for p in &ring.points {
            vertices.extend([p.x.to_mm(), p.y.to_mm()]);
        }
    }
    let Ok(indices) = earcutr::earcut(&vertices, &holes, 2) else { return Vec::new() };
    indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|triangle| {
            std::array::from_fn(|i| {
                let index = triangle[i] * 2;
                WorldPos::new(vertices[index], vertices[index + 1])
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use viamind_doc::ObjectId;
    use viamind_geom::Point;

    use super::*;

    pub(super) fn part(footprint: &str, rotation_mdeg: i32, side: Side) -> Part {
        Part {
            pads: None,
            id: ObjectId::new(),
            designator: "R1".into(),
            value: "10k".into(),
            footprint: footprint.into(),
            position: Point::from_mm(10.0, 20.0),
            rotation_mdeg,
            side,
        }
    }

    #[test]
    fn resistor_has_two_pads_masks_and_outline() {
        let items = part_items(&part("R_0603", 0, Side::Top));
        assert_eq!(items.iter().filter(|i| i.layer == LayerId::F_CU).count(), 2);
        assert_eq!(items.iter().filter(|i| i.layer == LayerId::F_MASK).count(), 2);
        assert_eq!(items.iter().filter(|i| i.layer == LayerId::F_SILKS).count(), 4);
        let pad = items[0].bounds;
        assert!((pad.center().x - 9.2).abs() < 1e-9 && (pad.center().y - 20.0).abs() < 1e-9);
    }

    #[test]
    fn quarter_turn_swaps_pad_axes() {
        let items = part_items(&part("R_0603", 90_000, Side::Top));
        let pad = items[0].bounds;
        assert!((pad.center().x - 10.0).abs() < 1e-9);
        assert!((pad.width() - 0.95).abs() < 1e-9 && (pad.height() - 0.9).abs() < 1e-9);
    }

    #[test]
    fn bottom_side_parts_use_bottom_layers() {
        let items = part_items(&part("SOIC-8", 0, Side::Bottom));
        assert_eq!(items.iter().filter(|i| i.layer == LayerId::B_CU).count(), 8);
        assert!(items.iter().all(|i| i.layer != LayerId::F_CU));
    }

    #[test]
    fn scene_contains_outline_and_parts() {
        let mut project = Project::new("test");
        let p = part("R_0805", 0, Side::Top);
        project.board.parts.insert(p.id, p);
        let scene = scene_from_project(&project);
        assert_eq!(scene.items.iter().filter(|i| i.layer == LayerId::EDGE_CUTS).count(), 4);
        assert_eq!(scene.items.len(), 4 + 8);
    }
}

#[cfg(test)]
mod outline_tests {
    use super::*;
    use viamind_doc::BoardOutline;
    use viamind_geom::{Point, Polygon};
    #[test]
    fn concave_substrate_triangulation_excludes_cutouts_and_keeps_fixed_boundaries() {
        let polygon = |points: &[(f64, f64)]| Polygon::new(points.iter().map(|&(x, y)| Point::from_mm(x, y)).collect());
        let outline = BoardOutline::new(
            polygon(&[(0.0, 0.0), (40.0, 0.0), (40.0, 20.0), (20.0, 20.0), (20.0, 40.0), (0.0, 40.0)]),
            vec![polygon(&[(5.0, 5.0), (10.0, 5.0), (10.0, 10.0), (5.0, 10.0)])],
        )
        .unwrap();
        let triangles = substrate_triangles(&outline);
        let area: f64 = triangles.iter().map(|[a, b, c]| ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs() / 2.0).sum();
        assert!((area - 1175.0).abs() < 1e-8);
        let hole = &outline.cutouts[0];
        for [a, b, c] in triangles {
            assert!(!hole.contains(Point::from_mm((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0)));
        }
        let mut project = Project::new("fixed");
        project.board.outline = Some(outline);
        let p = super::tests::part("R_0603", 0, Side::Top);
        let mut p = p;
        p.position = Point::from_mm(150.0, 150.0);
        project.board.parts.insert(p.id, p);
        let scene = scene_from_project(&project);
        assert_eq!(scene.board.max, WorldPos::new(40.0, 40.0));
        assert!(scene.bounds().max.x > 150.0 && scene.bounds().max.y > 150.0);
    }
}

/// Seed editable pads from the graphical preview without persisting invented connectivity.
pub(crate) fn preview_pad_inputs(part: &Part) -> Vec<viamind_engine::PadInput> {
    footprint(&part.footprint)
        .pads
        .into_iter()
        .enumerate()
        .map(|(i, (x, y, w, h))| viamind_engine::PadInput {
            id: None,
            number: (i + 1).to_string(),
            x_mm: x,
            y_mm: y,
            width_mm: w,
            height_mm: h,
            net: None,
        })
        .collect()
}

#[cfg(test)]
mod pad_tests {
    use super::*;
    use viamind_doc::{ObjectId, Pad, Pads};
    use viamind_geom::{Nm, Point};
    #[test]
    fn explicit_copper_uses_true_rotated_mirrored_corners() {
        let mut part = super::tests::part("R_0603", 45_000, Side::Bottom);
        part.pads = Some(
            Pads::try_from(vec![Pad {
                id: ObjectId::new(),
                number: "1".into(),
                position: Point::from_mm(2.0, 0.0),
                width: Nm::from_mm(2.0),
                height: Nm::from_mm(1.0),
                net: Some("GND".into()),
            }])
            .unwrap(),
        );
        let items = part_items(&part);
        assert_eq!(items.iter().filter(|i| i.layer == LayerId::B_CU).count(), 1);
        let Prim::Quad(points) = items[0].prim else {
            panic!("expected actual pad corners");
        };
        let edge = |a: WorldPos, b: WorldPos| ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
        assert!((edge(points[0], points[1]) - 2.0).abs() < 1e-9);
        assert!((edge(points[1], points[2]) - 1.0).abs() < 1e-9);
        let center = WorldPos::new(points.iter().map(|p| p.x).sum::<f64>() / 4.0, points.iter().map(|p| p.y).sum::<f64>() / 4.0);
        assert!((center.x - (10.0 - 2.0_f64.sqrt())).abs() < 1e-9);
        assert!((center.y - (20.0 + 2.0_f64.sqrt())).abs() < 1e-9);
        part.pads = Some(Pads(vec![]));
        assert!(part_items(&part).iter().all(|i| i.layer == LayerId::B_SILKS));
    }
}
