//! Shared document/session actions. UI entry points route changes through this catalog.
//!
//! The declaration generates both the dispatcher and its complete enumeration for the MCP
//! parity gate. View preferences (camera, layers, tool choice, test scene) are local UI state.

use serde_json::{Value, json};
use viamind_engine::{
    AddPart, Command, DeletePart, EngineError, EngineHandle, ImportBoardOutlineDxf, MovePart, NewProject, OpenProject, Origin, SaveProject,
    SetBoardOutline, SetPartPads, SetSelection, TransformParts, UpdatePart, input_schema,
};

macro_rules! ui_actions {
    ($( $variant:ident => ($tool:expr, $schema:expr, $handler:ident) ),+ $(,)?) => {
        /// Every action the desktop can apply to the shared design or session.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum UiAction { $( $variant ),+ }

        impl UiAction {
            /// Generated from the dispatcher declaration, so new actions enter the parity gate.
            pub const ALL: &'static [Self] = &[$( Self::$variant ),+];

            pub fn tool(self) -> &'static str {
                match self { $( Self::$variant => $tool ),+ }
            }

            pub fn input_schema(self) -> Value {
                match self { $( Self::$variant => $schema ),+ }
            }

            /// Run as the human. Agents use the same engine, with their own origin and undo scope.
            pub fn run(self, engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
                match self { $( Self::$variant => $handler(engine, input) ),+ }
            }
        }
    };
}

fn no_args() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

ui_actions! {
    ApplyFootprint => (viamind_engine::ApplyFootprint::NAME, input_schema::<viamind_engine::ApplyFootprint>(), apply_footprint),
    ImportFootprint => ("import_footprint", input_schema::<viamind_engine::FootprintFile>(), import_footprint),
    ExportFootprint => ("export_footprint", input_schema::<viamind_engine::FootprintFile>(), export_footprint),
    ExportFabrication => ("export_fabrication", viamind_fab::fabrication_schema(), crate::fabrication::export_fabrication),
    RunDrc => ("run_drc", input_schema::<viamind_drc::DrcInput>(), run_drc),
    NewProject => ("new_project", input_schema::<NewProject>(), new_project),
    OpenProject => ("open_project", input_schema::<OpenProject>(), open_project),
    TransformParts => (TransformParts::NAME, input_schema::<TransformParts>(), transform_parts),
    SetPartPads => (SetPartPads::NAME, input_schema::<SetPartPads>(), set_part_pads),
    ImportBoardOutlineDxf => (ImportBoardOutlineDxf::NAME, input_schema::<ImportBoardOutlineDxf>(), import_board_outline_dxf),
    AddTrack => (viamind_engine::AddTrack::NAME, input_schema::<viamind_engine::AddTrack>(), add_track),
    DeleteTrack => (viamind_engine::DeleteTrack::NAME, input_schema::<viamind_engine::DeleteTrack>(), delete_track),
    AddVia => (viamind_engine::AddVia::NAME, input_schema::<viamind_engine::AddVia>(), add_via),
    DeleteVia => (viamind_engine::DeleteVia::NAME, input_schema::<viamind_engine::DeleteVia>(), delete_via),
    SetBoardOutline => (SetBoardOutline::NAME, input_schema::<SetBoardOutline>(), set_board_outline),
    UpdatePart => (UpdatePart::NAME, input_schema::<UpdatePart>(), update_part),
    DeletePart => (DeletePart::NAME, input_schema::<DeletePart>(), delete_part),
    MovePart => (MovePart::NAME, input_schema::<MovePart>(), move_part),
    PlacePart => (AddPart::NAME, input_schema::<AddPart>(), place_part),
    Undo => ("undo", no_args(), undo),
    Redo => ("redo", no_args(), redo),
    SetSelection => ("set_selection", input_schema::<SetSelection>(), set_selection),
    SaveProject => ("save_project", input_schema::<SaveProject>(), save_project),
}

fn apply_footprint(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::ApplyFootprint::NAME, input, Origin::User)?).map_err(encode_error)
}

fn import_footprint(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input = serde_json::from_value(input).map_err(encode_error)?;
    serde_json::to_value(engine.import_footprint(input, Origin::User)?).map_err(encode_error)
}

fn export_footprint(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input = serde_json::from_value(input).map_err(encode_error)?;
    Ok(json!({"path":engine.export_footprint(input)?}))
}

fn encode_error(error: serde_json::Error) -> EngineError {
    EngineError::InvalidInput { command: "ui_action".into(), message: error.to_string() }
}

fn place_part(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let result = engine.execute(AddPart::NAME, input, Origin::User)?;
    serde_json::to_value(result).map_err(encode_error)
}

fn undo(engine: &EngineHandle, _input: Value) -> Result<Value, EngineError> {
    engine.undo(Origin::User).map(|tx| json!({ "undone": tx }))
}

fn redo(engine: &EngineHandle, _input: Value) -> Result<Value, EngineError> {
    engine.redo(Origin::User).map(|tx| json!({ "redone": tx }))
}

fn set_selection(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input =
        serde_json::from_value(input).map_err(|error| EngineError::InvalidInput { command: "set_selection".into(), message: error.to_string() })?;
    let result = engine.set_selection(input, Origin::User)?;
    serde_json::to_value(result).map_err(encode_error)
}

fn save_project(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input =
        serde_json::from_value(input).map_err(|error| EngineError::InvalidInput { command: "save_project".into(), message: error.to_string() })?;
    serde_json::to_value(engine.save_project(input)?).map_err(encode_error)
}

fn new_project(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input =
        serde_json::from_value(input).map_err(|error| EngineError::InvalidInput { command: "new_project".into(), message: error.to_string() })?;
    serde_json::to_value(engine.new_project(input, Origin::User)?).map_err(encode_error)
}
fn open_project(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input =
        serde_json::from_value(input).map_err(|error| EngineError::InvalidInput { command: "open_project".into(), message: error.to_string() })?;
    serde_json::to_value(engine.open_project(input, Origin::User)?).map_err(encode_error)
}

fn update_part(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(UpdatePart::NAME, input, Origin::User)?).map_err(encode_error)
}
fn delete_part(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(DeletePart::NAME, input, Origin::User)?).map_err(encode_error)
}
fn move_part(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(MovePart::NAME, input, Origin::User)?).map_err(encode_error)
}

fn set_board_outline(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(SetBoardOutline::NAME, input, Origin::User)?).map_err(encode_error)
}

fn set_part_pads(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(SetPartPads::NAME, input, Origin::User)?).map_err(encode_error)
}

fn import_board_outline_dxf(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(ImportBoardOutlineDxf::NAME, input, Origin::User)?).map_err(encode_error)
}

fn run_drc(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let invalid = |message: String| EngineError::InvalidInput { command: "run_drc".into(), message };
    let input: viamind_drc::DrcInput = serde_json::from_value(input).map_err(|e| invalid(e.to_string()))?;
    input.validate().map_err(|e| invalid(e.to_string()))?;
    let snapshot = engine.session_state();
    let report = viamind_drc::run_drc(&snapshot.project, &input).map_err(|e| invalid(e.to_string()))?;
    Ok(json!({"source_cursor":snapshot.cursor,"report":report}))
}

fn add_track(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute("add_track", input, Origin::User)?).map_err(encode_error)
}

fn delete_track(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute("delete_track", input, Origin::User)?).map_err(encode_error)
}

fn add_via(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute("add_via", input, Origin::User)?).map_err(encode_error)
}

fn delete_via(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute("delete_via", input, Origin::User)?).map_err(encode_error)
}

fn transform_parts(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(TransformParts::NAME, input, Origin::User)?).map_err(encode_error)
}
