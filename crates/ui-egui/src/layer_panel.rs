//! The layer list in the left side panel: visibility, colour, opacity, active layer and order.

use egui::color_picker::{Alpha, color_edit_button_srgba};
use egui::{Checkbox, Color32, Slider, Ui, WidgetInfo, WidgetType};

use crate::layers::{LayerId, LayerStack};

/// Width of the per-layer opacity sliders, in points.
const OPACITY_SLIDER_WIDTH: f32 = 64.0;

/// Draws the layer list, top layer first, and applies the user's edits to `layers`.
///
/// Each row has a visibility checkbox (accessible label "`<name>` visible"), a colour swatch
/// that opens a colour picker, the layer name (click to make it the active layer) and an opacity
/// slider. Below the list: "Active layer on top" and buttons that move the active layer.
pub fn show(ui: &mut Ui, layers: &mut LayerStack) {
    ui.heading("Layers");
    ui.separator();

    let active = layers.active();
    let mut activate = None;
    // Top of the stack first, as board tools list layers.
    let ids: Vec<LayerId> = layers.layers().iter().rev().map(|l| l.id).collect();
    egui::Grid::new("layer_list").num_columns(4).spacing([6.0, 4.0]).show(ui, |ui| {
        for id in ids {
            let Some(layer) = layers.get_mut(id) else { continue };
            let name = layer.name.clone();

            let visible = ui.add(Checkbox::without_text(&mut layer.visible)).on_hover_text(format!("Show {name}"));
            let shown = layer.visible;
            visible.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, shown, format!("{name} visible")));

            color_edit_button_srgba(ui, &mut layer.color, Alpha::Opaque).on_hover_text(format!("{name} colour"));

            if ui.selectable_label(id == active, &name).on_hover_text("Make this the active layer").clicked() {
                activate = Some(id);
            }

            ui.spacing_mut().slider_width = OPACITY_SLIDER_WIDTH;
            ui.add(Slider::new(&mut layer.opacity, 0.0..=1.0).show_value(false)).on_hover_text(format!("{name} opacity"));
            ui.end_row();
        }
    });
    if let Some(id) = activate {
        layers.set_active(id);
    }

    ui.separator();
    ui.checkbox(&mut layers.active_on_top, "Active layer on top");
    ui.horizontal(|ui| {
        let active = layers.active();
        ui.label("Move active layer");
        if ui.small_button("Up").clicked() {
            layers.move_layer(active, true);
        }
        if ui.small_button("Down").clicked() {
            layers.move_layer(active, false);
        }
    });

    ui.add_space(8.0);
    ui.colored_label(Color32::GRAY, "Pan: middle-drag or space+drag");
    ui.colored_label(Color32::GRAY, "Zoom: scroll wheel");
    ui.colored_label(Color32::GRAY, "Fit: Home");
}
