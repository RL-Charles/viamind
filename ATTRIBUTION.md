# Attribution

Every third-party asset bundled with Viamind (fonts, icons, images, datasets) must be listed
here with its author, source and licence before it is merged. Viamind's own code and original
assets are under the MIT License (see [LICENSE](LICENSE)); third-party code components are in
[NOTICE](NOTICE).

Add one row per asset or per file set from one source. Pin the source URL to a commit or
release where possible, and keep the asset's licence file next to it in the repo.

## Fonts

| Asset | Author | Source URL | Licence | Files |
| --- | --- | --- | --- | --- |
| craft-fonts (adopted when used; not bundled yet) | ArtCraft Team and the craft-fonts contributors; per-font authors in upstream `ATTRIBUTION.md` | <https://github.com/storytold/craft-fonts> | Fonts: OFL-1.1; code: MIT OR Apache-2.0 | None yet |
| egui default fonts (embedded through epaint_default_fonts 0.36.2) | Source Foundry/Bitstream and DejaVu contributors (Hack); Google Noto contributors; Ubuntu font contributors; John Slegers (emoji icons) | [Pinned upstream font set](https://github.com/emilk/egui/tree/49682f8baa058bf49e011035cfbd6e825f88a5ef/crates/epaint_default_fonts/fonts) | Hack: MIT and Bitstream Vera; Noto: OFL-1.1; Ubuntu: Ubuntu-font-1.0; emoji icons: MIT | Hack-Regular.ttf, NotoEmoji-Regular.ttf, Ubuntu-Light.ttf, emoji-icon-font.ttf embedded in executable; [license texts](packaging/licenses/emilk-egui-49682f8baa05/fonts) |

## Icons

| Asset | Author | Source URL | Licence | Files |
| --- | --- | --- | --- | --- |

## Images

| Asset | Author | Source URL | Licence | Files |
| --- | --- | --- | --- | --- |

## Datasets

| Asset | Author | Source URL | Licence | Files |
| --- | --- | --- | --- | --- |

## Shader conventions

| Asset | Author | Source URL | Licence | Files |
| --- | --- | --- | --- | --- |
| egui-wgpu 0.36.2 colour conversion, premultiplied blending and dithering conventions | Emil Ernerfeldt and egui contributors | [Pinned shader source](https://github.com/emilk/egui/blob/0.36.2/crates/egui-wgpu/src/egui.wgsl), [upstream MIT licence](https://github.com/emilk/egui/blob/0.36.2/LICENSE-MIT) | MIT; complete notice retained at the top of the adapted shader | `crates/ui-egui/src/gpu_tiles.wgsl` |
