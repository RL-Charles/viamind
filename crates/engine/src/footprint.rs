//! Guarded application of detached reusable local pad geometry.
use crate::{Command, CommandError, Delta};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{ObjectId, Pad, Pads, Part, Project, footprint::Footprint};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyFootprint {
    pub expected_project: ObjectId,
    pub expected: Part,
    pub footprint: Footprint,
}

impl Command for ApplyFootprint {
    const NAME: &'static str = "apply_footprint";
    const DESCRIPTION: &'static str = "Apply reusable rectangular SMD local geometry and footprint name to a part guarded by exact project and part snapshots. Matching pad numbers preserve IDs/nets, new pads have fresh IDs and no net. Retains component placement. One undoable edit. Native Viamind geometry; no KiCad compatibility implied.";
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if project.id != self.expected_project {
            return Err(CommandError::conflict("project changed; reload before applying footprint"));
        }
        let current = project.board.part(&self.expected.id).ok_or_else(|| CommandError::not_found("part", self.expected.id))?;
        if current != &self.expected {
            return Err(CommandError::conflict("part changed; reload before applying footprint"));
        }
        self.footprint.validate().map_err(|e| CommandError::invalid("footprint", e))?;
        let existing = current.pads.as_ref().map(|p| p.0.as_slice()).unwrap_or_default();
        let mut after = current.clone();
        after.footprint = self.footprint.name;
        after.pads = Some(
            Pads::try_from(
                self.footprint
                    .pads
                    .into_iter()
                    .map(|p| {
                        let old = existing.iter().find(|old| old.number == p.number);
                        Pad {
                            id: old.map(|p| p.id).unwrap_or_default(),
                            net: old.and_then(|p| p.net.clone()),
                            number: p.number,
                            position: viamind_geom::Point::new(p.position.x, p.position.y),
                            width: p.width,
                            height: p.height,
                        }
                    })
                    .collect::<Vec<_>>(),
            )
            .map_err(|e| CommandError::invalid("footprint", e))?,
        );
        if after == *current {
            return Ok(vec![]);
        }
        Ok(vec![Delta::record_part(project, self.expected.id, Some(after))])
    }
}

/// Capture/reuse a committed footprint through a local library file.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintFile {
    pub expected_project: ObjectId,
    pub expected: Part,
    pub path: std::path::PathBuf,
}

impl crate::EngineHandle {
    /// Read bounded library data outside the engine mutex, then apply atomically.
    pub fn import_footprint(&self, input: FootprintFile, origin: crate::Origin) -> Result<crate::ExecResult, crate::EngineError> {
        let footprint = viamind_doc::footprint::load(&input.path).map_err(|message| crate::EngineError::File { message })?;
        self.execute_typed(ApplyFootprint { expected_project: input.expected_project, expected: input.expected, footprint }, origin)
    }

    /// Publish committed local geometry without changing design/history/dirty state.
    pub fn export_footprint(&self, input: FootprintFile) -> Result<std::path::PathBuf, crate::EngineError> {
        let (part, native) = self.with_engine(|engine| {
            let project = engine.project();
            if project.id != input.expected_project || project.board.part(&input.expected.id) != Some(&input.expected) {
                return Err(crate::EngineError::File { message: "project or part changed; reload before exporting footprint".into() });
            }
            Ok((input.expected.clone(), engine.document_status().path))
        })?;
        let footprint = Footprint::from_part(&part).map_err(|message| crate::EngineError::File { message })?;
        viamind_doc::footprint::save_new(&footprint, &input.path, native.as_deref()).map_err(|message| crate::EngineError::File { message })
    }
}
