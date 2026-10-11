//! Shared document/session actions. UI entry points route changes through this catalog.
//!
//! The declaration generates both the dispatcher and its complete enumeration for the MCP
//! parity gate. View preferences (camera, layers, tool choice, test scene) are local UI state.

use crate::fabrication::export_fabrication;
use crate::output_jobs::{load as load_output_job, run as run_output_job, save as save_output_job};
use serde_json::{Value, json};
use viamind_engine::{
    AddPart, Command, DeletePart, EngineError, EngineHandle, EventCursor, ImportBoardOutlineDxf, MovePart, NewProject, OpenProject, Origin,
    SaveProject, SetBoardOutline, SetPartPads, SetSelection, TransformParts, UpdatePart, input_schema,
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
    PlaceSymbolTemplate => (viamind_engine::PlaceSymbolTemplate::NAME, input_schema::<viamind_engine::PlaceSymbolTemplate>(), place_symbol_template),
    SnapshotSymbolSheet => ("snapshot_symbol_sheet", input_schema::<viamind_engine::SymbolSheetQuery>(), snapshot_symbol_sheet),
    LoadSymbolTemplate => ("load_symbol_template", input_schema::<viamind_engine::LoadSymbolTemplate>(), load_symbol_template),
    SaveSymbolTemplate => ("save_symbol_template", input_schema::<viamind_engine::SaveSymbolTemplate>(), save_symbol_template),
    ExportSymbolTemplate => ("export_symbol_template", input_schema::<viamind_engine::ExportSymbolTemplate>(), export_symbol_template),

    PreviewPcbImport => ("preview_pcb_import", input_schema::<viamind_engine::PreviewPcbImport>(), preview_pcb_import),
    ImportPcb => ("import_pcb", input_schema::<viamind_engine::ImportPcb>(), import_pcb),
    DrawSchematicWirePolyline => (viamind_engine::DrawSchematicWirePolyline::NAME, input_schema::<viamind_engine::DrawSchematicWirePolyline>(), draw_schematic_wire_polyline),
    SnapshotSchematicWireSheet => ("snapshot_schematic_wire_sheet", input_schema::<viamind_engine::SchematicWireSheetQuery>(), snapshot_schematic_wire_sheet),
    QueryLinkedPinPads => ("query_linked_pin_pads", input_schema::<viamind_engine::PinPadLinkQuery>(), query_linked_pin_pads),
    QueryLinkedComponents => ("query_linked_components", input_schema::<viamind_engine::ComponentLinkQuery>(), query_linked_components),
    CreateSchematicPcbComponent => (viamind_engine::CreateSchematicPcbComponent::NAME, input_schema::<viamind_engine::CreateSchematicPcbComponent>(), create_schematic_pcb_component),
    LinkSchematicSymbol => (viamind_engine::LinkSchematicSymbol::NAME, input_schema::<viamind_engine::LinkSchematicSymbol>(), link_schematic_symbol),
    SynchronizeSchematicPcb => (viamind_engine::SynchronizeSchematicPcb::NAME, input_schema::<viamind_engine::SynchronizeSchematicPcb>(), synchronize_schematic_pcb),
    AnnotateSchematic => (viamind_engine::AnnotateSchematic::NAME, input_schema::<viamind_engine::AnnotateSchematic>(), annotate_schematic),
    EditSchematicSheet => (viamind_engine::EditSchematicSheet::NAME, input_schema::<viamind_engine::EditSchematicSheet>(), edit_schematic_sheet),
    DeleteSchematicSheet => (viamind_engine::DeleteSchematicSheet::NAME, input_schema::<viamind_engine::DeleteSchematicSheet>(), delete_schematic_sheet),
    ApplyFootprint => (viamind_engine::ApplyFootprint::NAME, input_schema::<viamind_engine::ApplyFootprint>(), apply_footprint),
    ImportFootprint => ("import_footprint", input_schema::<viamind_engine::FootprintFile>(), import_footprint),
    ExportFootprint => ("export_footprint", input_schema::<viamind_engine::FootprintFile>(), export_footprint),

    RenderBoardSvg => ("render_board_view", input_schema::<viamind_render::BoardViewRequest<EventCursor>>(), render_board_svg),
    RenderBoardPng => ("render_board_png", input_schema::<viamind_render::BoardViewRequest<EventCursor>>(), render_board_png),
    RenderSchematicSvg => ("render_schematic_view", input_schema::<viamind_render::SchematicViewRequest<EventCursor>>(), render_schematic_svg),
    RenderSchematicPng => ("render_schematic_png", input_schema::<viamind_render::SchematicViewRequest<EventCursor>>(), render_schematic_png),
    ExportAssemblyCsv => ("export_assembly_csv", viamind_fab::assembly_schema(), export_assembly_csv),
    LoadOutputJob => ("load_output_job", viamind_fab::output_job_schema("load"), load_output_job),
    SaveOutputJob => ("save_output_job", viamind_fab::output_job_schema("save"), save_output_job),
    RunOutputJob => ("run_output_job", viamind_fab::output_job_schema("run"), run_output_job),
    ExportFabrication => ("export_fabrication", viamind_fab::fabrication_schema(), export_fabrication),
    ConfigureAutoDrc => ("configure_auto_drc", input_schema::<viamind_engine::AutoDrcConfig>(), configure_auto_drc),
    GetAutoDrcStatus => ("get_auto_drc_status", no_args(), get_auto_drc_status),
    RunErc => ("run_erc", input_schema::<viamind_engine::ErcInput>(), run_erc),
    RunDrc => ("run_drc", input_schema::<viamind_drc::DrcInput>(), run_drc),
    NewProject => ("new_project", input_schema::<NewProject>(), new_project),
    OpenProject => ("open_project", input_schema::<OpenProject>(), open_project),
    TransformParts => (TransformParts::NAME, input_schema::<TransformParts>(), transform_parts),
    SetPartPads => (SetPartPads::NAME, input_schema::<SetPartPads>(), set_part_pads),
    ImportBoardOutlineDxf => (ImportBoardOutlineDxf::NAME, input_schema::<ImportBoardOutlineDxf>(), import_board_outline_dxf),
    PlanPadWalkaround => ("plan_pad_walkaround",input_schema::<viamind_engine::RoutePadWalkaround>(),plan_pad_walkaround),
    RoutePadWalkaround => (viamind_engine::RoutePadWalkaround::NAME,input_schema::<viamind_engine::RoutePadWalkaround>(),route_pad_walkaround),
    PlanPadViaWalkaround => ("plan_pad_via_walkaround",input_schema::<viamind_engine::RoutePadViaWalkaround>(),plan_pad_via_walkaround),
    RoutePadViaWalkaround => (viamind_engine::RoutePadViaWalkaround::NAME,input_schema::<viamind_engine::RoutePadViaWalkaround>(),route_pad_via_walkaround),
    RoutePadPosture => (viamind_engine::RoutePadPosture::NAME,input_schema::<viamind_engine::RoutePadPosture>(),route_pad_posture),
    RoutePadViaPolyline => (viamind_engine::RoutePadViaPolyline::NAME,input_schema::<viamind_engine::RoutePadViaPolyline>(),route_pad_via_polyline),
    RoutePadPolyline => (viamind_engine::RoutePadPolyline::NAME, input_schema::<viamind_engine::RoutePadPolyline>(), route_pad_polyline),
    RoutePolyline => (viamind_engine::RoutePolyline::NAME, input_schema::<viamind_engine::RoutePolyline>(), route_polyline),
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

fn configure_auto_drc(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let config =
        serde_json::from_value(input).map_err(|e| EngineError::InvalidInput { command: "configure_auto_drc".into(), message: e.to_string() })?;
    serde_json::to_value(engine.configure_auto_drc(config)?).map_err(encode_error)
}

fn get_auto_drc_status(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    if !input.as_object().is_some_and(serde_json::Map::is_empty) {
        return Err(EngineError::InvalidInput { command: "get_auto_drc_status".into(), message: "expected an empty object".into() });
    }
    serde_json::to_value(engine.auto_drc_status()).map_err(encode_error)
}

fn run_erc(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input = serde_json::from_value(input).map_err(encode_error)?;
    serde_json::to_value(engine.run_erc(&input).map_err(|error| EngineError::Command { command: "run_erc".into(), error })?).map_err(encode_error)
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

fn edit_schematic_sheet(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute("edit_schematic_sheet", input, Origin::User)?).map_err(encode_error)
}
fn delete_schematic_sheet(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute("delete_schematic_sheet", input, Origin::User)?).map_err(encode_error)
}

fn annotate_schematic(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::AnnotateSchematic::NAME, input, Origin::User)?).map_err(encode_error)
}

fn route_polyline(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::RoutePolyline::NAME, input, Origin::User)?).map_err(encode_error)
}

fn link_schematic_symbol(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::LinkSchematicSymbol::NAME, input, Origin::User)?).map_err(encode_error)
}
fn create_schematic_pcb_component(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::CreateSchematicPcbComponent::NAME, input, Origin::User)?).map_err(encode_error)
}
fn synchronize_schematic_pcb(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::SynchronizeSchematicPcb::NAME, input, Origin::User)?).map_err(encode_error)
}
fn route_pad_polyline(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::RoutePadPolyline::NAME, input, Origin::User)?).map_err(encode_error)
}

fn query_linked_components(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input = serde_json::from_value(input).map_err(encode_error)?;
    serde_json::to_value(
        engine.query_linked_components(&input).map_err(|error| EngineError::Command { command: "query_linked_components".into(), error })?,
    )
    .map_err(encode_error)
}

fn render_board_svg(e: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    crate::view_export::export(e, UiAction::RenderBoardSvg, input)
}
fn render_board_png(e: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    crate::view_export::export(e, UiAction::RenderBoardPng, input)
}
fn render_schematic_svg(e: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    crate::view_export::export(e, UiAction::RenderSchematicSvg, input)
}
fn render_schematic_png(e: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    crate::view_export::export(e, UiAction::RenderSchematicPng, input)
}

fn route_pad_via_polyline(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::RoutePadViaPolyline::NAME, input, Origin::User)?).map_err(encode_error)
}

fn draw_schematic_wire_polyline(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::DrawSchematicWirePolyline::NAME, input, Origin::User)?).map_err(encode_error)
}
fn snapshot_schematic_wire_sheet(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input = serde_json::from_value(input).map_err(encode_error)?;
    serde_json::to_value(
        engine
            .snapshot_schematic_wire_sheet(&input)
            .map_err(|error| EngineError::Command { command: "snapshot_schematic_wire_sheet".into(), error })?,
    )
    .map_err(encode_error)
}

fn route_pad_posture(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::RoutePadPosture::NAME, input, Origin::User)?).map_err(encode_error)
}

fn export_assembly_csv(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    crate::assembly_export::export(engine, input)
}

fn query_linked_pin_pads(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input = serde_json::from_value(input).map_err(encode_error)?;
    serde_json::to_value(
        engine.query_linked_pin_pads(&input).map_err(|error| EngineError::Command { command: "query_linked_pin_pads".into(), error })?,
    )
    .map_err(encode_error)
}

fn route_pad_walkaround(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::RoutePadWalkaround::NAME, input, Origin::User)?).map_err(encode_error)
}
fn route_pad_via_walkaround(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::RoutePadViaWalkaround::NAME, input, Origin::User)?).map_err(encode_error)
}
fn plan_pad_via_walkaround(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input = serde_json::from_value(input).map_err(encode_error)?;
    serde_json::to_value(
        engine.plan_pad_via_walkaround(&input).map_err(|error| EngineError::Command { command: "plan_pad_via_walkaround".into(), error })?,
    )
    .map_err(encode_error)
}
fn plan_pad_walkaround(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input = serde_json::from_value(input).map_err(encode_error)?;
    serde_json::to_value(engine.plan_pad_walkaround(&input).map_err(|error| EngineError::Command { command: "plan_pad_walkaround".into(), error })?)
        .map_err(encode_error)
}

fn place_symbol_template(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::PlaceSymbolTemplate::NAME, input, Origin::User)?).map_err(encode_error)
}
fn snapshot_symbol_sheet(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input = serde_json::from_value(input).map_err(encode_error)?;
    serde_json::to_value(
        engine.snapshot_symbol_sheet(&input).map_err(|error| EngineError::Command { command: "snapshot_symbol_sheet".into(), error })?,
    )
    .map_err(encode_error)
}
fn load_symbol_template(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.load_symbol_template(serde_json::from_value(input).map_err(encode_error)?)?).map_err(encode_error)
}
fn save_symbol_template(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.save_symbol_template(serde_json::from_value(input).map_err(encode_error)?)?).map_err(encode_error)
}
fn export_symbol_template(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.export_symbol_template(serde_json::from_value(input).map_err(encode_error)?)?).map_err(encode_error)
}

fn preview_pcb_import(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input = serde_json::from_value(input).map_err(encode_error)?;
    serde_json::to_value(engine.preview_pcb_import(input)?).map_err(encode_error)
}
fn import_pcb(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input = serde_json::from_value(input).map_err(encode_error)?;
    serde_json::to_value(engine.import_pcb(input, Origin::User)?).map_err(encode_error)
}
