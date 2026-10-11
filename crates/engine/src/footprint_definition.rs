//! Detached native definition files; publication never changes the design.
use crate::{EngineError, EngineHandle};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use std::path::PathBuf;
use viamind_doc::footprint::Footprint;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoadFootprintDefinition {
    pub path: PathBuf,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveFootprintDefinition {
    pub footprint: Footprint,
    pub path: PathBuf,
}

impl<'de> Deserialize<'de> for SaveFootprintDefinition {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = crate::pad_apertures::StrictValue::deserialize(d)?.0;
        if v.get("path").and_then(serde_json::Value::as_str).is_some_and(|s| s.len() > 4096) {
            return Err(serde::de::Error::custom("definition path exceeds4096 bytes before typed decoding"));
        }
        if let Some(f) = v.get("footprint") {
            preflight_definition(f).map_err(serde::de::Error::custom)?;
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            footprint: Footprint,
            path: PathBuf,
        }
        let input: Input = serde_json::from_value(v).map_err(serde::de::Error::custom)?;
        Ok(Self { footprint: input.footprint, path: input.path })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FootprintDefinitionSaved {
    pub path: PathBuf,
    pub name: String,
    pub pad_count: usize,
}

/// Borrowed transport admission before constructing owned pad/artwork rows.
pub fn preflight_definition(value: &serde_json::Value) -> Result<(), String> {
    use serde_json::Value;
    let pads = value.get("pads").and_then(Value::as_array);
    if pads.is_some_and(|p| p.len() > 1024) {
        return Err("footprint definitions support at most 1024 pads before typed decoding".into());
    }
    if value.get("name").and_then(Value::as_str).is_some_and(|s| s.len() > 512)
        || pads.is_some_and(|p| p.iter().any(|p| p.get("number").and_then(Value::as_str).is_some_and(|s| s.len() > 512)))
    {
        return Err("footprint name/number exceeds512 UTF-8 bytes before typed decoding".into());
    }
    if value.get("artwork").and_then(|a| a.get("silkscreen")).and_then(Value::as_array).is_some_and(|a| a.len() > 256) {
        return Err("footprint artwork supports at most256 segments before typed decoding".into());
    }
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(b.len());
            if self.0 > 1024 * 1024 {
                return Err(std::io::Error::other("footprint definition exceeds1MiB before typed decoding"));
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(&mut Counter(0), value).map_err(|e| e.to_string())
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
        crate::command_hooks::check_read()?;
        let footprint = viamind_doc::footprint::load(&input.path).map_err(|message| EngineError::File { message })?;
        validate_definition(&footprint).map_err(|message| EngineError::File { message })?;
        Ok(footprint)
    }

    /// Stage outside locks; publish under the latest active-native-file lifecycle guard.
    pub fn save_footprint_definition(&self, input: SaveFootprintDefinition) -> Result<FootprintDefinitionSaved, EngineError> {
        crate::command_hooks::check_write()?;
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
            footprint: Footprint { holes: None, artwork: None, name: "Empty explicit definition".into(), pads: vec![] },
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
