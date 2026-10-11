//! Snapshot-based manufacturing file export tools shared by HTTP and headless MCP.
use serde_json::json;
use viamind_engine::{EngineHandle, EventCursor};

use crate::{RegistryError, ToolError, ToolRegistry};

type Export = viamind_fab::AssemblyExportRequest<EventCursor>;

fn write_export(
    engine: &EngineHandle,
    snapshot: viamind_fab::AssemblySnapshot,
    input: &Export,
    native: Option<&std::path::Path>,
    before_publish: impl FnOnce(),
) -> std::io::Result<std::path::PathBuf> {
    let prepared = snapshot.prepare(input.kind, &input.path, input.overwrite, native)?;
    before_publish();
    engine.with_native_file_guard(|current| prepared.publish(current))
}

/// Add read-only design export. Publication uses explicit overwrite permission.
pub fn register(registry: &mut ToolRegistry, engine: EngineHandle) -> Result<(), RegistryError> {
    registry.register(
        "export_assembly_csv",
        "Export generic Viamind BOM, placement or sourcing BOM CSV to a local path. Placement uses component origins in global mm, positive Y down, CCW degrees viewed from top; bottom keeps the stored transform. Sourcing BOM includes explicitly entered identifiers and population for all parts. BOM/placement ignore population. Not a manufacturer preset. Refuses existing files unless overwrite=true and always protects the active native file. Optional expected_cursor guards the exported snapshot; returns exported_cursor. Bounded to100000 components,4096 bytes/string,2 MiB aggregate text and32 MiB output. Shared two-snapshot process admission rejects busy requests. Does not change design/history/dirty state.",
        viamind_fab::assembly_schema(),
        move |input| {
            let engine = engine.clone();
            async move {
                let input: Export = serde_json::from_value(input).map_err(|error| ToolError::InvalidInput(error.to_string()))?;
                if input.path.as_os_str().is_empty() {
                    return Err(ToolError::InvalidInput("path must not be empty".into()));
                }
                let (snapshot, cursor, native) = engine.with_engine(|engine| {
                    let cursor = engine.event_cursor();
                    if input.expected_cursor.is_some_and(|expected| expected != cursor) {
                        return Err(ToolError::Failed("document changed; call get_session_state and retry the export".into()));
                    }
                    let snapshot = viamind_fab::AssemblySnapshot::capture_for(engine.project(), input.kind).map_err(|e|ToolError::Failed(e.to_string()))?;
                    Ok((snapshot, cursor, engine.document_status().path))
                })?;
                let count = snapshot.component_count();
                let kind = input.kind;
                let path = tokio::task::spawn_blocking(move || {
                    write_export(&engine, snapshot, &input, native.as_deref(), || {})
                })
                    .await.map_err(|error| ToolError::Failed(error.to_string()))?
                    .map_err(|error| ToolError::Failed(format!("could not export CSV: {error}")))?;
                Ok(json!({"path": path, "exported_cursor": cursor, "component_count": count, "kind": kind}))
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn other_agent_open_between_staging_and_mcp_publication_cannot_be_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.json");
        let destination = dir.path().join("new-native.csv");
        viamind_doc::file::save(&viamind_doc::Project::new("source"), &source).unwrap();
        viamind_doc::file::save(&viamind_doc::Project::new("other native"), &destination).unwrap();
        let original = std::fs::read(&destination).unwrap();
        let engine = EngineHandle::load(&source).unwrap();
        let snapshot = engine.read(viamind_fab::AssemblySnapshot::capture).unwrap();
        let worker = engine.clone();
        let input = Export { kind: viamind_fab::AssemblyCsv::Placement, path: destination.clone(), overwrite: true, expected_cursor: None };
        let (staged, ready) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let export = std::thread::spawn(move || {
            write_export(&worker, snapshot, &input, Some(&source), || {
                staged.send(()).unwrap();
                released.recv().unwrap();
            })
        });
        ready.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        engine
            .open_project(
                viamind_engine::OpenProject { path: destination.clone(), discard_dirty: false, expected_cursor: None },
                viamind_engine::Origin::agent("native-opener"),
            )
            .unwrap();
        let opened = (engine.session_state(), engine.document_status());
        release.send(()).unwrap();
        assert!(export.join().unwrap().unwrap_err().to_string().contains("active native"));
        assert_eq!(std::fs::read(destination).unwrap(), original);
        assert_eq!((engine.session_state(), engine.document_status()), opened);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
