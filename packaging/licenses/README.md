# Pinned dependency license texts

Some published crates omit their repository-root license files. `sources.json`
records exact crate versions, the source commit from each crate's
`.cargo_vcs_info.json`, original upstream URLs and SHA256 values for these texts.
The packaging script verifies both commit identity and file hashes before copying
them into the bundle. The files retain upstream text unchanged.

egui's embedded fonts also retain their font-specific license texts. Other
dependency license files are copied from the local Cargo registry into the
bundle. Every packaged dependency entry and text path is listed in the generated
`licenses/index.json`. A missing text stops packaging for review.
