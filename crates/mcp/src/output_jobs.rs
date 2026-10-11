//! Saved read-only manufacturing jobs with a coherent bounded board capture.
use crate::{RegistryError, ToolError, ToolRegistry};
use serde_json::json;
use viamind_engine::{EngineHandle, EventCursor};
use viamind_fab::{OutputJobFileRequest, OutputJobSnapshot, RunOutputJobRequest, SaveOutputJobRequest};
fn failed(error: impl std::fmt::Display) -> ToolError {
    ToolError::Failed(error.to_string())
}
pub fn register(registry: &mut ToolRegistry, engine: EngineHandle) -> Result<(), RegistryError> {
    registry.register(
        "load_output_job",
        "Read a strict regular non-symlink detached output-job JSON, at most 64 KiB. Does not change the design or history.",
        viamind_fab::output_job_schema("load"),
        |input| async move {
            let input: OutputJobFileRequest = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput(e.to_string()))?;
            let job = tokio::task::spawn_blocking(move || viamind_fab::load_output_job(&input.path)).await.map_err(failed)?.map_err(failed)?;
            Ok(json!({"job":job}))
        },
    )?;
    let save_engine = engine.clone();
    registry.register("save_output_job","Save canonical detached job settings to a NEW file. Existing destinations and active native files are preserved. Does not change the design.",viamind_fab::output_job_schema("save"),move |input| {
        let engine=save_engine.clone();async move {
            let input:SaveOutputJobRequest=serde_json::from_value(input).map_err(|e|ToolError::InvalidInput(e.to_string()))?;
            tokio::task::spawn_blocking(move || {
                let path=viamind_fab::save_output_job(&input.job,&input.path,engine.document_status().path.as_deref()).map_err(failed)?;
                Ok(json!({"path":path,"job":input.job}))
            }).await.map_err(failed)?
        }
    })?;
    registry.register("run_output_job","Read saved bounded job settings and export one guarded board snapshot to a NEW release directory: generic BOM/placement, supported outer Gerber X2/XNC and manifests. Generic profile: 11 files; checked profile: 12 including bounded production DRC validation. Checked jobs refuse violations, incomplete/excluded/missing geometry coverage on the same immutable board. Saved request clearance baselines cannot weaken captured exact named-net document minima; explicit width floors also apply. Validation reports both requested_rules and report.document_net_rules. 192 MiB total. No vendor certification/inner copper/pours/source metadata. Shared at-most-two manufacturing admission; cancellation retains slot until worker ends. Returns complete exported_cursor; unrelated schematics excluded and design unchanged.",viamind_fab::output_job_schema("run"),move |input| {
        let engine=engine.clone();async move {
            let input:RunOutputJobRequest<EventCursor>=serde_json::from_value(input).map_err(|e|ToolError::InvalidInput(e.to_string()))?;
            tokio::task::spawn_blocking(move || {
                let job=viamind_fab::load_output_job(&input.job_path).map_err(failed)?;
                let (snapshot,cursor,native)=engine.with_engine(|e| {
                    let cursor=e.event_cursor();
                    if input.expected_cursor.is_some_and(|c|c!=cursor) {return Err(failed("document changed; obtain a fresh cursor and retry output job"));}
                    Ok((OutputJobSnapshot::capture(e.project(),job).map_err(failed)?,cursor,e.document_status().path))
                })?;
                let (path,manifest)=snapshot.write_with_validation(cursor,&input.path,native.as_deref(), viamind_engine::checked_output_job_report).map_err(failed)?;
                Ok(json!({"path":path,"manifest":manifest,"exported_cursor":cursor,"file_count":manifest.files.len()+1}))
            }).await.map_err(failed)?
        }
    })
}
