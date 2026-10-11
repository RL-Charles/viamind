//! Inspection-only native WGPU readback when compositor capture is unavailable.
use std::{io::Write, path::PathBuf};

pub struct NativeCapture {
    path: Option<PathBuf>,
    frames: usize,
}
impl NativeCapture {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path, frames: 0 }
    }
    pub fn poll(&mut self, ctx: &egui::Context, controls: &[egui::Rect]) {
        self.frames += 1;
        let Some(path) = self.path.as_ref() else { return };
        let content = ctx.content_rect();
        assert!(controls.iter().all(|r| content.contains_rect(*r)), "prepared production controls must fit actual native viewport");
        let image = ctx.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            assert!(image.pixels.len() <= 4_194_304, "inspection framebuffer exceeds capture cap");
            assert!(image.pixels.iter().all(|p| p.a() == 255), "opaque framebuffer required for lossless RGB artifact");
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path).expect("new native framebuffer artifact");
            write!(file, "P6\n{} {}\n255\n", image.size[0], image.size[1]).expect("native PPM header");
            let pixels: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
            file.write_all(&pixels).expect("native framebuffer RGB pixels");
            file.sync_all().expect("sync native artifact");
            println!(
                "{}",
                serde_json::json!({"capture":"eframe native WGPU framebuffer Screenshot event","pid":std::process::id(),"path":path,"pixels":image.size,"native_frames":self.frames,"full_session_unchanged":true,"control_bounds":controls.iter().map(|r| [r.min.x,r.min.y,r.max.x,r.max.y]).collect::<Vec<_>>(),"content_rect":[content.min.x,content.min.y,content.max.x,content.max.y]})
            );
            self.path = None;
        } else {
            if self.frames == 8 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
}
