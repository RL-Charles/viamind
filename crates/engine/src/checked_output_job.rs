//! Trusted validation for manufacturing jobs; no external report authorizes release.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, Write};
use viamind_doc::Project;
use viamind_drc::{DrcInput, DrcReport};

const REPORT_LIMIT: usize = 16 * 1024 * 1024;
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
struct GeometryCounts {
    pads: usize,
    tracks: usize,
    vias: usize,
    holes: usize,
}
fn counts(project: &Project) -> io::Result<GeometryCounts> {
    // Borrowed preflight also protects direct internal callers before DRC clones labels.
    let mut text = 0usize;
    let mut charge = |value: &str| -> io::Result<()> {
        text = text.saturating_add(value.len());
        if value.len() > 4096 || text > 2 * 1024 * 1024 {
            return Err(invalid("checked DRC snapshot text exceeds4096bytes/string or2MiB aggregate"));
        }
        Ok(())
    };
    for part in project.board.parts.values() {
        charge(&part.designator)?;
        if let Some(pads) = &part.pads {
            for pad in &pads.0 {
                charge(&pad.number)?;
                if let Some(net) = &pad.net {
                    charge(net)?;
                }
            }
        }
    }
    for net in
        project.board.tracks.values().filter_map(|track| track.net.as_deref()).chain(project.board.vias.values().filter_map(|via| via.net.as_deref()))
    {
        charge(net)?;
    }
    if project.board.outline.as_ref().is_some_and(|outline| outline.rings().map(|ring| ring.points.len().saturating_add(1)).sum::<usize>() > 200_000)
    {
        return Err(invalid("checked DRC outline exceeds200000 primitives"));
    }
    let pads = project.board.parts.values().try_fold(0usize, |count, part| {
        count.checked_add(part.pads.as_ref().map_or(0, |pads| pads.0.len())).ok_or_else(|| invalid("checked pad count overflow"))
    })?;
    if project.board.parts.len() > 100_000
        || pads > viamind_drc::MAX_PADS
        || project.board.tracks.len().saturating_add(project.board.vias.len().saturating_mul(2)).saturating_add(pads) > viamind_drc::MAX_COPPER_SHAPES
    {
        return Err(invalid("checked job exceeds bounded DRC copper coverage"));
    }
    Ok(GeometryCounts { pads, tracks: project.board.tracks.len(), vias: project.board.vias.len(), holes: project.board.vias.len() })
}
fn require_complete(project: &Project, expected: GeometryCounts, report: &DrcReport) -> io::Result<()> {
    let coverage = &report.coverage;
    if report.project_id != project.id || !coverage.explicit_outline_checked || project.board.outline.is_none() {
        return Err(invalid("checked job requires complete explicit outline coverage for the captured project"));
    }
    if coverage.excluded_part_count != 0 || !coverage.excluded_parts.is_empty() || coverage.excluded_details_truncated {
        return Err(invalid("checked job refuses excluded or unsupported components"));
    }
    if report.truncated || report.incomplete_reason.is_some() {
        return Err(invalid("checked job DRC is incomplete or truncated"));
    }
    if expected
        != (GeometryCounts {
            pads: coverage.checked_pads,
            tracks: coverage.checked_tracks,
            vias: coverage.checked_vias,
            holes: coverage.checked_holes,
        })
    {
        return Err(invalid("checked job DRC coverage does not match every exported copper object"));
    }
    if !report.violations.is_empty() {
        return Err(invalid(format!(
            "checked job refused: {} scoped DRC violations; first: {}",
            report.violations.len(),
            report.violations[0].message
        )));
    }
    Ok(())
}
struct BoundedReport(Vec<u8>);
impl Write for BoundedReport {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > REPORT_LIMIT.saturating_sub(self.0.len()) {
            return Err(invalid("checked validation artifact exceeds16MiB"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[derive(Serialize)]
struct Validation<'a, C> {
    format: &'static str,
    version: u32,
    profile: &'static str,
    status: &'static str,
    project_id: viamind_doc::ObjectId,
    exported_cursor: &'a C,
    requested_rules: &'a DrcInput,
    exported_geometry: GeometryCounts,
    report: &'a DrcReport,
}
/// Called inside the retained manufacturing worker on its exact owned board snapshot.
/// The only input bridge contains fixed numeric rules, not an authorization report.
/// Serialization charges the report limit before appending output bytes.
pub fn checked_output_job_report<C: Serialize>(project: &Project, input: &Value, cursor: &C) -> io::Result<Vec<u8>> {
    let input: DrcInput = DrcInput::deserialize(input).map_err(|error| invalid(error.to_string()))?;
    input.validate().map_err(|error| invalid(error.to_string()))?;
    let expected = counts(project)?;
    let report = viamind_drc::run_drc(project, &input).map_err(|error| invalid(error.to_string()))?;
    require_complete(project, expected, &report)?;
    let artifact = Validation {
        format: "viamind-output-validation",
        version: 1,
        profile: "checked_outer_2layer",
        status: "passed_for_scoped_checks",
        project_id: project.id,
        exported_cursor: cursor,
        requested_rules: &input,
        exported_geometry: expected,
        report: &report,
    };
    let mut output = BoundedReport(Vec::new());
    serde_json::to_writer_pretty(&mut output, &artifact).map_err(|error| invalid(error.to_string()))?;
    output.write_all(b"\n")?;
    Ok(output.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project() -> Project {
        viamind_doc::file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap()
    }
    fn rules() -> DrcInput {
        DrcInput {
            copper_clearance_mm: 0.,
            edge_clearance_mm: 0.,
            minimum_via_drill_mm: 0.,
            minimum_annular_width_mm: 0.,
            hole_clearance_mm: 0.,
            hole_to_copper_clearance_mm: 0.,
            ..Default::default()
        }
    }
    #[test]
    fn exact_geometry_report_exclusions_and_requested_rules_are_explicit() {
        let project = project();
        let before = project.clone();
        let bytes =
            checked_output_job_report(&project, &serde_json::to_value(rules()).unwrap(), &serde_json::json!({"stream_id":"test","sequence":42}))
                .unwrap();
        let report: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(report["exported_cursor"]["sequence"], 42);
        assert_eq!(report["exported_geometry"]["tracks"], project.board.tracks.len());
        assert_eq!(report["report"]["violations"], serde_json::json!([]));
        assert!(!report["report"]["coverage"]["checks_not_performed"].as_array().unwrap().is_empty());
        assert_eq!(project, before);
    }
    #[test]
    fn unsupported_outline_violations_and_incomplete_counts_cannot_authorize_release() {
        let mut project = project();
        let rules = rules();
        let expected = counts(&project).unwrap();
        let report = viamind_drc::run_drc(&project, &rules).unwrap();
        require_complete(&project, expected, &report).unwrap();
        for bad in [0, 1, 2, 3, 4] {
            let mut report = report.clone();
            match bad {
                0 => report.truncated = true,
                1 => report.incomplete_reason = Some("limited".into()),
                2 => report.coverage.checked_tracks += 1,
                3 => report.coverage.checked_holes += 1,
                _ => report.coverage.excluded_details_truncated = true,
            }
            assert!(require_complete(&project, expected, &report).is_err());
        }
        let mut strict = rules.clone();
        strict.edge_clearance_mm = 100.;
        assert!(checked_output_job_report(&project, &serde_json::to_value(strict).unwrap(), &0).unwrap_err().to_string().contains("violations"));
        project.board.parts.values_mut().next().unwrap().pads = None;
        assert!(checked_output_job_report(&project, &serde_json::to_value(&rules).unwrap(), &0).unwrap_err().to_string().contains("excluded"));
        project.board.parts.clear();
        project.board.outline = None;
        assert!(checked_output_job_report(&project, &serde_json::to_value(rules).unwrap(), &0).unwrap_err().to_string().contains("outline"));
    }
    #[test]
    fn report_append_budget_is_charged_before_allocation() {
        let mut output = BoundedReport(Vec::new());
        assert!(output.write_all(&vec![0; REPORT_LIMIT + 1]).is_err());
        assert!(output.0.is_empty());
        assert!(checked_output_job_report(&project(), &serde_json::json!({"pass":true}), &0).is_err());
    }
}
