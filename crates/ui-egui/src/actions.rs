//! Shared document/session actions. UI entry points route changes through this catalog.
//!
//! The declaration generates both the dispatcher and its complete enumeration for the MCP
//! parity gate. View preferences (camera, layers, tool choice, test scene) are local UI state.

use crate::fabrication::export_fabrication;
use crate::output_jobs::{load as load_output_job, run as run_output_job, save as save_output_job};
use crate::release_inspection::inspect as inspect_output_release;
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
    ListPcbBranches => ("list_pcb_branches", no_args(), list_pcb_branches),
    GetPcbBranchDiff => ("get_pcb_branch_diff", input_schema::<viamind_engine::pcb_branches::PcbBranchDiffInput>(), get_pcb_branch_diff),
    MergePcbBranch => ("merge_pcb_branch", input_schema::<viamind_engine::pcb_branches::MergePcbBranch>(), merge_pcb_branch),
    DiscardPcbBranch => ("discard_pcb_branch", input_schema::<viamind_engine::pcb_branches::PcbBranchRevision>(), discard_pcb_branch),
    QueryNamedNetRules => ("query_named_net_rules", no_args(), query_named_net_rules),
    SetNamedNetRules => (viamind_engine::SetNamedNetRules::NAME, input_schema::<viamind_engine::SetNamedNetRules>(), set_named_net_rules),
    PlaceSymbolPackage => (viamind_engine::PlaceSymbolPackage::NAME, input_schema::<viamind_engine::PlaceSymbolPackage>(), place_symbol_package),
    DeleteSymbolPackage => (viamind_engine::DeleteSymbolPackage::NAME, input_schema::<viamind_engine::DeleteSymbolPackage>(), delete_symbol_package),
    LoadSymbolPackage => ("load_symbol_package", input_schema::<viamind_engine::LoadSymbolPackage>(), load_symbol_package),
    SaveSymbolPackage => ("save_symbol_package", input_schema::<viamind_engine::SaveSymbolPackage>(), save_symbol_package),
    QuerySymbolPackages => ("query_symbol_packages", input_schema::<viamind_engine::SymbolPackageQuery>(), query_symbol_packages),
    PlaceSymbolTemplate => (viamind_engine::PlaceSymbolTemplate::NAME, input_schema::<viamind_engine::PlaceSymbolTemplate>(), place_symbol_template),
    SnapshotSymbolSheet => ("snapshot_symbol_sheet", input_schema::<viamind_engine::SymbolSheetQuery>(), snapshot_symbol_sheet),
    LoadSymbolTemplate => ("load_symbol_template", input_schema::<viamind_engine::LoadSymbolTemplate>(), load_symbol_template),
    SaveSymbolTemplate => ("save_symbol_template", input_schema::<viamind_engine::SaveSymbolTemplate>(), save_symbol_template),
    ExportSymbolTemplate => ("export_symbol_template", input_schema::<viamind_engine::ExportSymbolTemplate>(), export_symbol_template),

    LoadFootprintDefinition => ("load_footprint_definition", input_schema::<viamind_engine::LoadFootprintDefinition>(), load_footprint_definition),
    SaveFootprintDefinition => ("save_footprint_definition", input_schema::<viamind_engine::SaveFootprintDefinition>(), save_footprint_definition),
    PreviewFootprintConversion => ("preview_footprint_conversion", input_schema::<viamind_engine::PreviewFootprintConversion>(), preview_footprint_conversion),
    ConvertFootprintFile => ("convert_footprint_file", input_schema::<viamind_engine::ConvertFootprintFile>(), convert_footprint_file),
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
    InspectOutputRelease => ("inspect_output_release",viamind_fab::release_inspection_schema(),inspect_output_release),
    LoadOutputJob => ("load_output_job", viamind_fab::output_job_schema("load"), load_output_job),
    SaveOutputJob => ("save_output_job", viamind_fab::output_job_schema("save"), save_output_job),
    RunOutputJob => ("run_output_job", viamind_fab::output_job_schema("run"), run_output_job),
    ExportFabrication => ("export_fabrication", viamind_fab::fabrication_schema(), export_fabrication),
    InspectFabricationFiles => ("inspect_fabrication_files",input_schema::<viamind_engine::InspectFabricationFiles>(),inspect_fabrication_files),
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
    PlanCardinalTrackCleanup => ("plan_cardinal_track_cleanup", input_schema::<viamind_engine::CleanupCardinalTracks>(), plan_cardinal_track_cleanup),
    CleanupCardinalTracks => (viamind_engine::CleanupCardinalTracks::NAME, input_schema::<viamind_engine::CleanupCardinalTracks>(), cleanup_cardinal_tracks),
    RoutePadPosture => (viamind_engine::RoutePadPosture::NAME,input_schema::<viamind_engine::RoutePadPosture>(),route_pad_posture),
    RoutePadViaPolyline => (viamind_engine::RoutePadViaPolyline::NAME,input_schema::<viamind_engine::RoutePadViaPolyline>(),route_pad_via_polyline),
    RouteTerminalPolyline => (viamind_engine::RouteTerminalPolyline::NAME,input_schema::<viamind_engine::RouteTerminalPolyline>(),route_terminal_polyline),
    RouteTerminalPosture => (viamind_engine::RouteTerminalPosture::NAME,input_schema::<viamind_engine::RouteTerminalPosture>(),route_terminal_posture),
    RoutePadPolyline => (viamind_engine::RoutePadPolyline::NAME, input_schema::<viamind_engine::RoutePadPolyline>(), route_pad_polyline),
    RoutePolyline => (viamind_engine::RoutePolyline::NAME, input_schema::<viamind_engine::RoutePolyline>(), route_polyline),
    SetTrackProperties => (viamind_engine::SetTrackProperties::NAME, input_schema::<viamind_engine::SetTrackProperties>(), set_track_properties),
    SetViaProperties => (viamind_engine::SetViaProperties::NAME, input_schema::<viamind_engine::SetViaProperties>(), set_via_properties),
    AddTrack => (viamind_engine::AddTrack::NAME, input_schema::<viamind_engine::AddTrack>(), add_track),
    DeleteTrack => (viamind_engine::DeleteTrack::NAME, input_schema::<viamind_engine::DeleteTrack>(), delete_track),
    AddVia => (viamind_engine::AddVia::NAME, input_schema::<viamind_engine::AddVia>(), add_via),
    DeleteVia => (viamind_engine::DeleteVia::NAME, input_schema::<viamind_engine::DeleteVia>(), delete_via),
    SetBoardOutline => (SetBoardOutline::NAME, input_schema::<SetBoardOutline>(), set_board_outline),
    SetPartAssembly => (viamind_engine::SetPartAssembly::NAME, input_schema::<viamind_engine::SetPartAssembly>(), set_part_assembly),
    UpdatePart => (UpdatePart::NAME, input_schema::<UpdatePart>(), update_part),
    DeletePart => (DeletePart::NAME, input_schema::<DeletePart>(), delete_part),
    MovePart => (MovePart::NAME, input_schema::<MovePart>(), move_part),
    PlacePart => (AddPart::NAME, input_schema::<AddPart>(), place_part),
    Undo => ("undo", no_args(), undo),
    Redo => ("redo", no_args(), redo),
    SetSelection => ("set_selection", input_schema::<SetSelection>(), set_selection),
    SaveProject => ("save_project", input_schema::<SaveProject>(), save_project),
}

fn branch_error(error: impl std::fmt::Display) -> EngineError {
    EngineError::InvalidInput { command: "pcb_branch".into(), message: error.to_string() }
}
fn list_pcb_branches(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    if !input.as_object().is_some_and(serde_json::Map::is_empty) {
        return Err(branch_error("list_pcb_branches takes an empty object"));
    }
    serde_json::to_value(engine.list_pcb_branches()).map_err(encode_error)
}
fn get_pcb_branch_diff(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.diff_pcb_branch(serde_json::from_value(input).map_err(encode_error)?).map_err(branch_error)?).map_err(encode_error)
}
fn merge_pcb_branch(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(
        engine
            .merge_pcb_branch(serde_json::from_value(input).map_err(encode_error)?, &viamind_engine::pcb_branches::BranchActor::human())
            .map_err(branch_error)?,
    )
    .map_err(encode_error)
}
fn discard_pcb_branch(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(
        engine
            .discard_pcb_branch(serde_json::from_value(input).map_err(encode_error)?, &viamind_engine::pcb_branches::BranchActor::human())
            .map_err(branch_error)?,
    )
    .map_err(encode_error)
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
    let snapshot = engine.with_engine(|e| {
        viamind_doc::named_net_rule_budget(&e.project().board.net_rules).map_err(|e| invalid(e.into()))?;
        Ok::<_, EngineError>(e.session_state())
    })?;
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
fn cleanup_cardinal_tracks(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::CleanupCardinalTracks::NAME, input, Origin::User)?).map_err(encode_error)
}
fn plan_cardinal_track_cleanup(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input = serde_json::from_value(input).map_err(encode_error)?;
    serde_json::to_value(
        engine.plan_cardinal_track_cleanup(&input).map_err(|error| EngineError::Command { command: "plan_cardinal_track_cleanup".into(), error })?,
    )
    .map_err(encode_error)
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

fn set_part_assembly(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::SetPartAssembly::NAME, input, Origin::User)?).map_err(encode_error)
}

fn inspect_fabrication_files(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let invalid = |e: String| EngineError::InvalidInput { command: "inspect_fabrication_files".into(), message: e };
    let input = serde_json::from_value(input).map_err(|e| invalid(e.to_string()))?;
    serde_json::to_value(engine.inspect_fabrication_files(input)?).map_err(|e| invalid(e.to_string()))
}
fn preview_footprint_conversion(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.preview_footprint_conversion(serde_json::from_value(input).map_err(encode_error)?)?).map_err(encode_error)
}
fn convert_footprint_file(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.convert_footprint_file(serde_json::from_value(input).map_err(encode_error)?)?).map_err(encode_error)
}

fn query_named_net_rules(engine: &EngineHandle, _input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.query_named_net_rules().map_err(|error| EngineError::Command { command: "query_named_net_rules".into(), error })?)
        .map_err(encode_error)
}
fn set_named_net_rules(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::SetNamedNetRules::NAME, input, Origin::User)?).map_err(encode_error)
}

fn load_footprint_definition(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.load_footprint_definition(serde_json::from_value(input).map_err(encode_error)?)?).map_err(encode_error)
}
fn save_footprint_definition(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.save_footprint_definition(serde_json::from_value(input).map_err(encode_error)?)?).map_err(encode_error)
}

fn route_terminal_polyline(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::RouteTerminalPolyline::NAME, input, Origin::User)?).map_err(encode_error)
}
fn route_terminal_posture(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::RouteTerminalPosture::NAME, input, Origin::User)?).map_err(encode_error)
}

fn set_track_properties(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute("set_track_properties", input, Origin::User)?).map_err(encode_error)
}
fn set_via_properties(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute("set_via_properties", input, Origin::User)?).map_err(encode_error)
}

fn place_symbol_package(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::PlaceSymbolPackage::NAME, input, Origin::User)?).map_err(encode_error)
}
fn delete_symbol_package(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.execute(viamind_engine::DeleteSymbolPackage::NAME, input, Origin::User)?).map_err(encode_error)
}
fn load_symbol_package(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(engine.load_symbol_package(serde_json::from_value(input).map_err(encode_error)?)?).map_err(encode_error)
}
fn save_symbol_package(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let invalid = |message| EngineError::InvalidInput { command: "save_symbol_package".into(), message };
    if input.get("path").and_then(Value::as_str).is_none_or(|path| path.is_empty() || path.len() > 4096) {
        return Err(invalid("Package path requires1..4096 UTF8 bytes".into()));
    }
    viamind_doc::symbol_package::predecode(input.get("template").ok_or_else(|| invalid("template is required".into()))?).map_err(invalid)?;
    serde_json::to_value(engine.save_symbol_package(serde_json::from_value(input).map_err(encode_error)?)?).map_err(encode_error)
}
fn query_symbol_packages(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    serde_json::to_value(
        engine
            .query_symbol_packages(&serde_json::from_value(input).map_err(encode_error)?)
            .map_err(|error| EngineError::Command { command: "query_symbol_packages".into(), error })?,
    )
    .map_err(encode_error)
}
