# Compact schematic sidebar verification

T-0124 measures the existing 640×400 sidebar before changing it. All supported entries and Apply/Reload/Cancel already become fully reachable through its outer scroll area. Actual wheel input over the sidebar leaves the canvas camera and complete shared session unchanged. No sidebar layout change is made.

The measured defect was local field identity: removing pin row 1 changed retained row 2's text-edit ID. Draft and pin rows now own transient UUID keys and fields use explicit constant identities. Actual input retains the second pin's exact `1.000003` text and cursor state through removal of the first row. New and reloaded drafts intentionally receive new local identities. Persisted geometry and commands remain unchanged.

Five focused actual-input checks pass, including exact guarded Apply and keyboard Undo/redo; 15 inherited schematic, labels, wires and parity checks pass. Strict workspace/all-target Clippy and the inspection build pass. The one required complete Linux gate will run on the final combined predecessor stack. This is a targeted milestone, not final acceptance or submission.

The two actual native WGPU captures show [scrolled entries](verification/t0124/entries-wheel.png) and [draft actions](verification/t0124/draft-wheel.png). Requested current painted button backgrounds must fit their effective ancestor clips for eight UI passes before capture. Full SessionSnapshot and DocumentStatus are checked on every UI callback. Preparation uses egui test input and disclosed native raw_input_hook wheel events; no OS-input claim is made. Both 640×400 hosts self-closed. Original RGBA P7 and RGB P6 files are retained; all PNG RGB bytes independently decoded through ImageMagick match the originals. Alpha is 255 throughout these two captures.

Three refused inspection attempts are retained: initial single-layer observation, all-layer observation, and diagnostic observation. Diagnostics showed native scroll preparation started at the top, so the final observer performs wheel reveal in the native input path. These attempts are not successful visibility evidence and required no product layout change.

[The targeted master](verification/t0124/targeted-master.json) indexes source, logs, native metadata and hashes. Final combined build/runtime/captures/CI and independent review remain pending.
