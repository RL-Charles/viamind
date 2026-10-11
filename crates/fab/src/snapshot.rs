//! Admission and allocation budgets shared by all manufacturing snapshots.
use std::{
    io,
    sync::atomic::{AtomicUsize, Ordering},
};
use viamind_doc::Project;

static ACTIVE: AtomicUsize = AtomicUsize::new(0);
pub(crate) struct Permit;
impl Permit {
    pub(crate) fn acquire() -> io::Result<Self> {
        ACTIVE.try_update(Ordering::AcqRel, Ordering::Acquire, |active| (active < 2).then_some(active + 1)).map(|_| Self).map_err(|_| {
            io::Error::new(io::ErrorKind::WouldBlock, "manufacturing export busy: at most two snapshots per process; retry after an export finishes")
        })
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(crate) struct TextBudget(usize);
impl TextBudget {
    pub(crate) fn new() -> Self {
        Self(0)
    }
    pub(crate) fn add(&mut self, text: &str) -> io::Result<()> {
        if text.len() > 4096 {
            return Err(invalid("manufacturing strings must not exceed 4096 UTF-8 bytes"));
        }
        self.0 = self.0.saturating_add(text.len());
        if self.0 > 2 * 1024 * 1024 {
            return Err(invalid("manufacturing snapshot exceeds 2 MiB aggregate UTF-8 text"));
        }
        Ok(())
    }
}
pub(crate) fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

/// A bounded immutable board-only manufacturing snapshot. Unrelated schematics are excluded.
pub struct FabricationSnapshot {
    pub(crate) project: Project,
    _permit: Permit,
}
impl FabricationSnapshot {
    /// Validate allocation budgets before cloning. Busy admission never waits.
    pub fn capture(project: &Project) -> io::Result<Self> {
        Self::capture_for_job(project, false)
    }
    /// One permit covers all borrowed preflight and the single geometry copy.
    pub(crate) fn capture_for_job(project: &Project, calibrated: bool) -> io::Result<Self> {
        let permit = Permit::acquire()?;
        let policy = viamind_doc::named_net_rule_budget(&project.board.net_rules).map_err(invalid)?;
        let mut count =
            project.board.parts.len().saturating_add(project.board.tracks.len()).saturating_add(project.board.vias.len().saturating_mul(5));
        let check = |count| {
            if count > 200_000 {
                Err(invalid("fabrication export supports at most 200000 primitives including part headers and outline rings"))
            } else {
                Ok(())
            }
        };
        count = count.saturating_add(policy.rows);
        check(count)?;
        let mut text = TextBudget::new();
        for name in project.board.net_rules.keys() {
            text.add(name)?;
        }
        text.add(&project.name)?;
        for part in project.board.parts.values() {
            count = count.saturating_add(part.pads.as_ref().map_or(0, |p| p.0.len()).saturating_mul(2));
            check(count)?;
            for value in [&part.designator, &part.value, &part.footprint] {
                text.add(value)?;
            }
            if let Some(pads) = &part.pads {
                for pad in &pads.0 {
                    text.add(&pad.number)?;
                    if let Some(net) = &pad.net {
                        text.add(net)?;
                    }
                }
            }
        }
        for net in project.board.tracks.values().filter_map(|p| p.net.as_deref()).chain(project.board.vias.values().filter_map(|p| p.net.as_deref()))
        {
            text.add(net)?;
        }
        if let Some(outline) = &project.board.outline {
            for ring in outline.rings() {
                count = count.saturating_add(1).saturating_add(ring.points.len());
                check(count)?;
            }
        }
        if calibrated {
            crate::assembly_placement::validate(project.board.parts.values())?;
        }
        let mut board = project.board.clone_without_assembly();
        if calibrated {
            for (id, part) in &mut board.parts {
                part.assembly = project.board.parts[id].assembly.as_ref().map(|source| {
                    Box::new(viamind_doc::AssemblyMetadata {
                        populate: source.populate,
                        mounting: source.populate.then_some(source.mounting).flatten(),
                        placement: source.populate.then_some(source.placement).flatten(),
                        ..Default::default()
                    })
                });
            }
        }
        Ok(Self { project: Project { id: project.id, name: project.name.clone(), board, schematic: Default::default() }, _permit: permit })
    }
}
