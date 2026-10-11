use viamind_doc::fabrication_view::{self as view, FabricationLayer as L, FabricationPrimitive as P, FabricationReadBudget};
use viamind_geom::{Nm, Point};
#[test]
fn all_native_export_layers_parse_independently_with_exact_profile_tracks_drills_and_rotated_pads() {
    let project = viamind_doc::file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let package =
        viamind_fab::fabrication(&project, viamind_fab::FabricationOptions { mask_expansion_mm: 0.050001, include_silkscreen: false }).unwrap();
    let mut budget = FabricationReadBudget::default();
    let mut layers = Vec::new();
    for (name, text) in &package.files {
        if name == "manifest.json" {
            continue;
        }
        let geometry = if name.ends_with(".gbr") { view::parse_gerber(text, &mut budget) } else { view::parse_xnc(text, &mut budget) }.unwrap();
        if geometry.layer != L::Drill {
            assert_eq!(geometry.same_coordinates, Some(project.id));
        }
        match geometry.layer {
            L::BottomCopper => {
                assert!(geometry.primitives.contains(&P::Segment {
                    start: Point::new(Nm(30000000), Nm(17000000)),
                    end: Point::new(Nm(40000000), Nm(9000000)),
                    width: Nm(400000)
                }));
                assert!(geometry.primitives.iter().any(|p| matches!(p, P::Rectangle { rotation_mdeg: 30000, .. })));
            }
            L::Profile => assert_eq!(geometry.primitives.len(), 8),
            L::Drill => {
                assert_eq!(geometry.primitives.len(), 2);
                assert!(
                    geometry
                        .primitives
                        .iter()
                        .any(|p| matches!(p,P::Circle{center,diameter}if *center==Point::new(Nm(18000000),Nm(12000000))&&*diameter==Nm(300000)))
                );
            }
            L::TopMaskOpenings | L::BottomMaskOpenings => assert!(geometry.negative_file_polarity),
            _ => {}
        }
        layers.push(geometry.layer);
    }
    assert_eq!(layers.len(), 6);
}
