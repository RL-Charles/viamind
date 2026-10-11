//! Board layers as the canvas shows them: name, colour, visibility, opacity and draw order.
//!
//! [`LayerStack`] is plain data with no egui state beyond colours, so it can move into the design
//! model later. Layer ids are display ids for now, not a file format.

use egui::Color32;

/// Identifies a layer. The constants are the layers of [`LayerStack::pcb_default`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayerId(pub u16);

impl LayerId {
    pub const F_CU: Self = Self(0);
    /// Inner copper layers are `In1.Cu` = 1 up to `In30.Cu` = 30.
    pub const IN1_CU: Self = Self(1);
    pub const IN2_CU: Self = Self(2);
    pub const B_CU: Self = Self(31);
    pub const B_SILKS: Self = Self(36);
    pub const F_SILKS: Self = Self(37);
    pub const B_MASK: Self = Self(38);
    pub const F_MASK: Self = Self(39);
    pub const B_COURTYARD: Self = Self(46);
    pub const F_COURTYARD: Self = Self(47);
    pub const EDGE_CUTS: Self = Self(44);
    /// Plated through-holes; a display layer, not a fabrication layer.
    pub const VIAS: Self = Self(100);

    /// Inner copper layer `n` (1-based, `In{n}.Cu`).
    pub fn inner_copper(n: u16) -> Self {
        assert!((1..=30).contains(&n), "inner copper layer {n} out of range");
        Self(n)
    }
}

/// What a layer holds; picks defaults and lets tools filter layers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayerKind {
    Copper,
    Silkscreen,
    Courtyard,
    SolderMask,
    EdgeCuts,
    Vias,
}

/// One layer and how it is drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub kind: LayerKind,
    /// Opaque base colour; [`Self::opacity`] is applied on top.
    pub color: Color32,
    pub visible: bool,
    /// 0 (invisible) to 1 (opaque).
    pub opacity: f32,
}

impl Layer {
    pub fn new(id: LayerId, name: impl Into<String>, kind: LayerKind, color: Color32, opacity: f32) -> Self {
        Self { id, name: name.into(), kind, color: color.to_opaque(), visible: true, opacity }
    }

    /// The colour items on this layer are filled with: the base colour at the layer's opacity.
    pub fn paint_color(&self) -> Color32 {
        let [r, g, b, _] = self.color.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, (self.opacity.clamp(0.0, 1.0) * 255.0).round() as u8)
    }
}

/// The board's layers in draw order (first is painted first, at the bottom), plus the active one.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerStack {
    layers: Vec<Layer>,
    active: LayerId,
    /// Paint the active layer above all others, whatever its place in the order.
    pub active_on_top: bool,
}

impl Default for LayerStack {
    fn default() -> Self {
        Self::pcb_default(4)
    }
}

impl LayerStack {
    /// An empty stack. Add layers with [`Self::push`].
    pub fn new() -> Self {
        Self { layers: Vec::new(), active: LayerId::F_CU, active_on_top: false }
    }

    /// The usual layers of a board with `copper` copper layers (2 to 32, even), viewed from the
    /// front: back layers at the bottom, front layers and the board edge on top. F.Cu is active.
    pub fn pcb_default(copper: u16) -> Self {
        assert!((2..=32).contains(&copper) && copper.is_multiple_of(2), "copper layer count {copper} must be even, 2 to 32");
        let rgb = Color32::from_rgb;
        let mut stack = Self::new();
        stack.push(Layer::new(LayerId::B_SILKS, "B.SilkS", LayerKind::Silkscreen, rgb(232, 178, 167), 0.9));
        stack.push(Layer::new(LayerId::B_COURTYARD, "B.Courtyard", LayerKind::Courtyard, rgb(120, 160, 210), 0.8));
        stack.push(Layer::new(LayerId::B_MASK, "B.Mask", LayerKind::SolderMask, rgb(2, 255, 238), 0.25));
        stack.push(Layer::new(LayerId::B_CU, "B.Cu", LayerKind::Copper, rgb(70, 110, 220), 0.78));
        // Inner layers from the back up, cycling through distinct colours.
        let inner = [rgb(206, 125, 44), rgb(127, 200, 127), rgb(200, 100, 200), rgb(100, 180, 200)];
        for n in (1..copper - 1).rev() {
            let color = inner[usize::from(n - 1) % inner.len()];
            stack.push(Layer::new(LayerId::inner_copper(n), format!("In{n}.Cu"), LayerKind::Copper, color, 0.78));
        }
        stack.push(Layer::new(LayerId::F_CU, "F.Cu", LayerKind::Copper, rgb(220, 60, 50), 0.82));
        stack.push(Layer::new(LayerId::VIAS, "Vias", LayerKind::Vias, rgb(210, 190, 110), 1.0));
        stack.push(Layer::new(LayerId::F_MASK, "F.Mask", LayerKind::SolderMask, rgb(216, 100, 255), 0.25));
        stack.push(Layer::new(LayerId::F_SILKS, "F.SilkS", LayerKind::Silkscreen, rgb(242, 237, 161), 0.9));
        stack.push(Layer::new(LayerId::F_COURTYARD, "F.Courtyard", LayerKind::Courtyard, rgb(200, 120, 210), 0.8));
        stack.push(Layer::new(LayerId::EDGE_CUTS, "Edge.Cuts", LayerKind::EdgeCuts, rgb(208, 210, 205), 1.0));
        stack.active = LayerId::F_CU;
        stack
    }

    /// Adds `layer` on top. Replaces an existing layer with the same id in place.
    pub fn push(&mut self, layer: Layer) {
        match self.layers.iter_mut().find(|l| l.id == layer.id) {
            Some(existing) => *existing = layer,
            None => self.layers.push(layer),
        }
    }

    /// All layers in their stored order, bottom first (ignores [`Self::active_on_top`]).
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    pub fn len(&self) -> usize {
        self.layers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    pub fn get(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    pub fn get_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    pub fn by_name(&self, name: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.name == name)
    }

    /// The layer new items go on; highlighted in the layer list.
    pub fn active(&self) -> LayerId {
        self.active
    }

    /// Makes `id` the active layer; ignored if there is no such layer.
    pub fn set_active(&mut self, id: LayerId) {
        if self.get(id).is_some() {
            self.active = id;
        }
    }

    /// Shows or hides a layer; ignored if there is no such layer.
    pub fn set_visible(&mut self, id: LayerId, visible: bool) {
        if let Some(layer) = self.get_mut(id) {
            layer.visible = visible;
        }
    }

    /// Layers in paint order, bottom first, with the active layer last when
    /// [`Self::active_on_top`] is set.
    pub fn draw_order(&self) -> impl Iterator<Item = &Layer> + '_ {
        let lift = self.active_on_top;
        let active = self.active;
        let rest = self.layers.iter().filter(move |l| !lift || l.id != active);
        let top = self.layers.iter().filter(move |l| lift && l.id == active);
        rest.chain(top)
    }

    /// Moves layer `id` one place up (towards the top, `up = true`) or down in the stored order.
    /// Returns whether it moved.
    pub fn move_layer(&mut self, id: LayerId, up: bool) -> bool {
        let Some(i) = self.layers.iter().position(|l| l.id == id) else { return false };
        let j = if up { i + 1 } else { i.wrapping_sub(1) };
        if j >= self.layers.len() {
            return false;
        }
        self.layers.swap(i, j);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(stack: &LayerStack) -> Vec<&str> {
        stack.draw_order().map(|l| l.name.as_str()).collect()
    }

    #[test]
    fn default_stack_has_the_usual_pcb_layers() {
        let stack = LayerStack::pcb_default(4);
        assert_eq!(names(&stack), ["B.SilkS", "B.Mask", "B.Cu", "In2.Cu", "In1.Cu", "F.Cu", "Vias", "F.Mask", "F.SilkS", "Edge.Cuts"]);
        assert_eq!(stack.active(), LayerId::F_CU);
        assert!(stack.layers().iter().all(|l| l.visible && l.color.a() == 255));
        assert_eq!(stack.by_name("In2.Cu").map(|l| l.id), Some(LayerId::IN2_CU));

        let two = LayerStack::pcb_default(2);
        assert_eq!(two.len(), 10);
        assert!(two.layers().iter().filter(|l| l.kind == LayerKind::Copper).count() == 2);
        assert_eq!(LayerStack::pcb_default(8).layers().iter().filter(|l| l.kind == LayerKind::Copper).count(), 8);
    }

    #[test]
    fn active_layer_on_top_reorders_only_when_enabled() {
        let mut stack = LayerStack::pcb_default(2);
        stack.set_active(LayerId::B_CU);
        assert_eq!(stack.active(), LayerId::B_CU);
        assert_eq!(stack.draw_order().nth(2).map(|l| l.id), Some(LayerId::B_CU));
        stack.active_on_top = true;
        assert_eq!(stack.draw_order().last().map(|l| l.id), Some(LayerId::B_CU));
        assert_eq!(stack.draw_order().count(), stack.len());
        // Unknown ids are ignored.
        stack.set_active(LayerId(999));
        assert_eq!(stack.active(), LayerId::B_CU);
    }

    #[test]
    fn move_layer_swaps_neighbours() {
        let mut stack = LayerStack::pcb_default(2);
        assert!(stack.move_layer(LayerId::B_CU, true));
        assert_eq!(names(&stack)[2..4], ["F.Cu", "B.Cu"]);
        assert!(stack.move_layer(LayerId::B_CU, false));
        assert_eq!(names(&stack)[2..4], ["B.Cu", "F.Cu"]);
        assert!(!stack.move_layer(LayerId::B_SILKS, false), "already at the bottom");
        assert!(!stack.move_layer(LayerId::EDGE_CUTS, true), "already on top");
        assert!(!stack.move_layer(LayerId(999), true));
    }

    #[test]
    fn paint_color_applies_opacity() {
        let mut layer = Layer::new(LayerId::F_CU, "F.Cu", LayerKind::Copper, Color32::from_rgb(200, 100, 50), 1.0);
        assert_eq!(layer.paint_color(), Color32::from_rgb(200, 100, 50));
        layer.opacity = 0.5;
        assert_eq!(layer.paint_color(), Color32::from_rgba_unmultiplied(200, 100, 50, 128));
        layer.opacity = 0.0;
        assert_eq!(layer.paint_color().a(), 0);
    }
}
