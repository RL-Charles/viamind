//! Native adapter for the shared bounded integer label plan.
use crate::camera::{Camera, WorldPos};
use egui::{Color32, Context, FontId, Painter, Pos2, Rect, Stroke, Ui};
use std::sync::Arc;
use viamind_render::schematic_labels::SymbolLabel;

pub(super) struct Glyph<'a> {
    pub label: &'a SymbolLabel,
    pub at: Pos2,
    pub rect: Rect,
    pub galley: Arc<egui::Galley>,
}
pub(super) fn prepare<'a>(
    ctx: &Context,
    labels: &'a [SymbolLabel],
    camera: Camera,
    viewport: Rect,
    color: Color32,
) -> Result<Vec<Glyph<'a>>, String> {
    if camera.zoom < 10.0 {
        return Ok(vec![]);
    }
    let font = FontId::monospace(camera.zoom.min(32.0) as f32);
    let screen = |x: i64, y: i64| camera.world_to_screen(WorldPos::new(x as f64 / 1_000_000.0, y as f64 / 1_000_000.0), viewport);
    labels.iter().map(|label| {
        let galley=ctx.fonts_mut(|fonts|fonts.layout_no_wrap(label.display_text.clone(),font.clone(),color));
        let at=screen(label.anchor.x.0,label.anchor.y.0);
        let rect=galley.rect.translate(at.to_vec2());
        let mesh=galley.mesh_bounds.translate(at.to_vec2());
        let b=label.envelope;
        let promised=Rect::from_min_max(screen(b.min_x,b.min_y),screen(b.max_x,b.max_y));
        if !promised.contains_rect(rect) || (mesh.is_positive() && !promised.contains_rect(mesh)) {
            return Err(format!("Schematic label {} ({:?}) exceeds supported glyph metrics; use text supported by the bundled monospace font. Original text and design are unchanged.",label.object_id,label.role));
        }
        Ok(Glyph {label,at,rect,galley})
    }).collect()
}
pub(super) fn paint(ui: &mut Ui, painter: &Painter, glyphs: Vec<Glyph<'_>>, camera: Camera, viewport: Rect, color: Color32) {
    let screen = |p: viamind_doc::schematic::Coordinate| camera.world_to_screen(WorldPos::new(p.x.to_mm(), p.y.to_mm()), viewport);
    for glyph in glyphs {
        if let Some([start, end]) = glyph.label.leader {
            painter.extend(egui::Shape::dashed_line(&[screen(start), screen(end)], Stroke::new(0.8, Color32::from_rgb(155, 145, 105)), 3.0, 2.0));
        }
        painter.galley(glyph.at, glyph.galley, color);
        ui.interact(glyph.rect, egui::Id::new(("schematic-label", &glyph.label.id)), egui::Sense::hover()).on_hover_text(format!(
            "{:?} · {}\n{}{}",
            glyph.label.role,
            glyph.label.object_id,
            glyph.label.text,
            if glyph.label.abbreviated { "\nDisplay abbreviated; exact stored text is unchanged." } else { "" }
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use viamind_doc::{ObjectId, schematic::Coordinate};
    use viamind_geom::Nm;
    use viamind_render::{ViewBounds, schematic_labels::LabelRole};

    fn label(text: &str) -> SymbolLabel {
        let id = ObjectId::new();
        SymbolLabel {
            id: format!("value-{id}"),
            symbol_id: id,
            object_id: id,
            role: LabelRole::Value,
            text: text.into(),
            display_text: text.into(),
            abbreviated: false,
            anchor: Coordinate { x: Nm(200_000), y: Nm(200_000) },
            envelope: ViewBounds { min_x: 0, min_y: 0, max_x: i64::try_from(text.chars().count()).unwrap() * 700_000 + 400_000, max_y: 1_600_000 },
            leader: None,
        }
    }
    fn measure(text: &str, zoom: f64, shrink: bool) -> Result<usize, String> {
        let ctx = Context::default();
        let mut labels = vec![label(text)];
        if shrink {
            labels[0].envelope.max_x = 1;
        }
        let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(1600.0, 1000.0));
        let mut result = None;
        let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(viewport), ..Default::default() }, |ui| {
            result = Some(prepare(ui.ctx(), &labels, Camera { center: WorldPos::default(), zoom }, viewport, Color32::WHITE).map(|g| g.len()));
        });
        output.textures_delta.clear(); // Actual font measurement intentionally performs no GPU upload.
        result.unwrap()
    }
    #[test]
    fn actual_default_font_non_ascii_ink_fits_shared_envelopes_at_supported_zooms() {
        for zoom in [10.0, 12.0, 32.0, 128.0] {
            assert_eq!(measure("10kΩ µA é Я", zoom, false).unwrap(), 1);
            assert_eq!(measure("VIN 123 ABC xyz", zoom, false).unwrap(), 1);
        }
    }
    #[test]
    fn measured_fallback_never_escapes_envelope_and_refusal_identifies_source() {
        // Exercise actual fallback lookup; admission can depend on the bundled
        // font's replacement glyph, but any painted glyph must pass both bounds.
        for text in ["漢字", "😀🧪", "   "] {
            match measure(text, 12.0, false) {
                Ok(n) => assert_eq!(n, 1),
                Err(e) => assert!(e.contains("exceeds supported glyph metrics")),
            }
        }
        let error = measure("VIN", 12.0, true).unwrap_err();
        assert!(error.contains("Value") && error.contains("Original text and design are unchanged"));
        assert_eq!(measure("VIN", 9.999, true).unwrap(), 0, "low zoom suppresses labels before font work");
    }
}
