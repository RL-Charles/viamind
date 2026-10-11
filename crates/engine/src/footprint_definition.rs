//! Detached native definition files; publication never changes the design.
use crate::{EngineError, EngineHandle};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use viamind_doc::footprint::Footprint;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoadFootprintDefinition {
    pub path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveFootprintDefinition {
    pub footprint: Footprint,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FootprintDefinitionSaved {
    pub path: PathBuf,
    pub name: String,
    pub pad_count: usize,
}

/// Bound borrowed text/counts before native serialization clones geometry.
pub fn validate_definition(footprint: &Footprint) -> Result<(), String> {
    if let Some(artwork) = &footprint.artwork {
        artwork.validate()?;
    }
    if footprint.pads.len() > 1024 || footprint.name.len() > 512 || footprint.pads.iter().any(|p| p.number.len() > 512) {
        return Err("footprint definitions require at most 1024 pads and name/number fields at most 512 UTF-8 bytes".into());
    }
    footprint.validate()
}

impl EngineHandle {
    /// Same-fd regular-file loading is bounded and outside the design mutex.
    pub fn load_footprint_definition(&self, input: LoadFootprintDefinition) -> Result<Footprint, EngineError> {
        let footprint = viamind_doc::footprint::load(&input.path).map_err(|message| EngineError::File { message })?;
        validate_definition(&footprint).map_err(|message| EngineError::File { message })?;
        Ok(footprint)
    }

    /// Stage outside locks; publish under the latest active-native-file lifecycle guard.
    pub fn save_footprint_definition(&self, input: SaveFootprintDefinition) -> Result<FootprintDefinitionSaved, EngineError> {
        self.save_definition_before_publish(input, || {})
    }

    fn save_definition_before_publish(
        &self,
        input: SaveFootprintDefinition,
        before_publish: impl FnOnce(),
    ) -> Result<FootprintDefinitionSaved, EngineError> {
        validate_definition(&input.footprint).map_err(|message| EngineError::File { message })?;
        let staged = viamind_doc::footprint::stage_new(&input.footprint, &input.path).map_err(|message| EngineError::File { message })?;
        before_publish();
        let path = self.with_native_file_guard(|native| staged.publish(native)).map_err(|message| EngineError::File { message })?;
        Ok(FootprintDefinitionSaved { path, name: input.footprint.name, pad_count: input.footprint.pads.len() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn latest_native_path_and_racing_destination_are_preserved_after_staging() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("new.json");
        let engine = EngineHandle::new(viamind_doc::Project::new("keep"));
        let input = SaveFootprintDefinition {
            footprint: Footprint { artwork: None, name: "Empty explicit definition".into(), pads: vec![] },
            path: path.clone(),
        };
        let peer = engine.clone();
        let mut after = None;
        assert!(
            engine
                .save_definition_before_publish(input.clone(), || {
                    peer.save_project(crate::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
                    after = Some((peer.session_state(), peer.document_status(), std::fs::read(&path).unwrap()));
                })
                .unwrap_err()
                .to_string()
                .contains("active native")
        );
        let (session, status, bytes) = after.unwrap();
        assert_eq!(engine.session_state(), session);
        assert_eq!(engine.document_status(), status);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let other = directory.path().join("racing.json");
        assert!(
            engine
                .save_definition_before_publish(SaveFootprintDefinition { path: other.clone(), ..input }, || {
                    std::fs::write(&other, b"racing file").unwrap();
                })
                .is_err()
        );
        assert_eq!(std::fs::read(other).unwrap(), b"racing file");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2, "private stages cleaned up");
    }
}
