//! The design engine as a [`CommandHost`], so every engine command is an MCP tool.

use serde_json::{Value, json};
use viamind_engine::{
    ConnectivityQuery, EngineHandle, NetQuery, NewProject, OpenProject, Origin, PartQuery, SaveProject, SetSelection, WatchEvents, input_schema,
};

use crate::{
    agent::AgentId,
    host::{CommandHost, HostCommand},
};

/// Adapts an [`EngineHandle`] to the agent harness.
///
/// Agent edits are recorded with [`Origin::Agent`] so the UI can attribute them, and agents undo only their
/// own changes ([`EngineHandle::undo_own`]); the human can still undo anything from the UI.
#[derive(Clone)]
pub struct EngineHost {
    engine: EngineHandle,
}

impl EngineHost {
    /// Wraps `engine`; the handle is shared with the UI.
    pub fn new(engine: EngineHandle) -> Self {
        Self { engine }
    }

    fn origin(agent: &AgentId) -> Origin {
        Origin::agent(agent.0.clone())
    }
}

/// Bound borrowed request fields before serde allocates typed definition rows.
fn preflight_footprint_definition(input: &Value, writing: bool) -> Result<(), String> {
    if input.get("path").and_then(Value::as_str).is_some_and(|path| path.len() > 4096) {
        return Err("footprint definition paths require at most 4096 UTF-8 bytes".into());
    }
    if writing && let Some(footprint) = input.get("footprint") {
        let pads = footprint.get("pads").and_then(Value::as_array);
        if pads.is_some_and(|pads| pads.len() > 1024) {
            return Err("footprint definitions require at most 1024 pads before typed decoding".into());
        }
        if footprint.get("name").and_then(Value::as_str).is_some_and(|name| name.len() > 512)
            || pads.is_some_and(|pads| pads.iter().any(|pad| pad.get("number").and_then(Value::as_str).is_some_and(|number| number.len() > 512)))
        {
            return Err("footprint definition name/number fields require at most 512 UTF-8 bytes before typed decoding".into());
        }
    }
    Ok(())
}

impl CommandHost for EngineHost {
    fn is_observation(&self, name: &str) -> bool {
        matches!(
            name,
            "inspect_fabrication_files"
                | "load_footprint_definition"
                | "preview_footprint_conversion"
                | "preview_pcb_import"
                | "plan_cardinal_track_cleanup"
                | "plan_pad_walkaround"
                | "plan_pad_via_walkaround"
                | "query_route_endpoints"
                | "query_route_terminals"
                | "query_copper_properties"
                | "query_linked_pin_pads"
                | "query_linked_components"
                | "run_erc"
                | "preview_schematic_pcb_component"
                | "preview_schematic_pcb_sync"
                | "query_schematic_connectivity"
                | "preview_schematic_annotation"
                | "query_schematic"
                | "get_document_status"
                | "get_session_state"
                | "watch_events"
                | "get_auto_drc_status"
                | "get_auto_drc_event_state"
                | "watch_auto_drc_events"
                | "run_drc"
                | "query_parts"
                | "query_nets"
                | "query_named_net_rules"
                | "query_connectivity"
                | "get_selection"
                | "snapshot_schematic_wire_sheet"
        )
    }

    fn commands(&self) -> Vec<HostCommand> {
        let mut commands: Vec<_> = self
            .engine
            .specs()
            .into_iter()
            .map(|spec| HostCommand { name: spec.name, description: spec.description, input_schema: spec.input_schema })
            .collect();
        commands.push(HostCommand {
            name: "query_named_net_rules".into(),
            description: "Read-only bounded exact document named-net minimum clearance/track-width/via-drill/annular policy and full source cursor. Empty map preserves existing request baselines. No mutation; use set_named_net_rules for guarded undoable replacement.".into(),
            input_schema: json!({"type":"object", "properties":{}, "additionalProperties":false}),
        });
        commands.push(HostCommand {
            name: "inspect_fabrication_files".into(),
            description: "Read-only strict bounded Gerber/XNC geometry inspection. Requires exact expected_project/full expected_cursor and one to six explicit paths. Supports native-export metric5.6 circular/rectangular flashes, linear circular strokes, aperture rotation, mask openings/profile and metric drill hits. Unknown semantics reject atomically. Max4MiB/file,16MiB total,100000 commands,10000 primitives; one nonwaiting reader retained inside blocking work. Returns native downward-Y geometry, exact file SHA256, captured cursor and file attributes. SameCoordinates UUID mismatch rejects; drill common origin unverified. Files are read independently, not an atomic package snapshot or fabrication/equivalence certification. No design/history/selection/file mutation.".into(),
            input_schema: input_schema::<viamind_engine::InspectFabricationFiles>(),
        });
        commands.push(HostCommand { name: "load_footprint_definition".into(), description: "Read one strict detached native viamind-footprint version1 library (regular file <=1MiB, path <=4096 bytes). Named rectangular SMD local integer Nm geometry only, at most1024 pads. Does not require a selected part or change project/history/selection/dirty state. Does not parse foreign footprint formats.".into(), input_schema: input_schema::<viamind_engine::LoadFootprintDefinition>() });
        commands.push(HostCommand { name: "save_footprint_definition".into(), description: "Validate and publish one detached native rectangular SMD footprint definition to a NEW file. Explicit local integer Nm centers/dimensions and unique pad numbers, at most1024 pads, no IDs/nets/placement or inferred manufacturing data. Refuses existing paths and protects the latest active native project under its lifecycle guard. No design/history/selection/dirty change; apply separately using guarded import_footprint.".into(), input_schema: input_schema::<viamind_engine::SaveFootprintDefinition>() });
        commands.push(HostCommand { name: "preview_footprint_conversion".into(), description: "Read-only bounded standalone footprint S-expression preview. Front zero-angle footprints with cardinal rectangular SMD pads only (-360..360 degrees), exact integer nm and exhaustive per-list fidelity report with full_fidelity=false. Source UUIDs and cosmetic/model metadata are report-only omissions; meaningful source nets, unsupported geometry and unknown semantics block conversion. Regular UTF-8 file <=1MiB. Returns exact source SHA256; never changes the project or writes source.".into(), input_schema: input_schema::<viamind_engine::PreviewFootprintConversion>() });
        commands.push(HostCommand { name: "convert_footprint_file".into(), description: "Reparse the exact SHA256-guarded footprint source and publish one explicit new detached native footprint library without overwriting source, existing files or the current native project. Dirty work, history, selection and active native path remain unchanged. Converted libraries omit source UUIDs/nets and require separate explicit import_footprint to reuse: matching pad numbers retain native IDs/nets; new numbers receive fresh IDs and no net. No full fidelity claim.".into(), input_schema: input_schema::<viamind_engine::ConvertFootprintFile>() });
        commands.push(HostCommand {
            name: "plan_cardinal_track_cleanup".into(),
            description: "Read-only bounded equivalent cardinal track cleanup with complete joint scoped DRC. Same input as cleanup_cardinal_tracks: exact project/full cursor, explicit track IDs or assigned net/optional outer layer and rules. Preserves branches/exact quarter-turn pad/via anchors; all via drill-hole AABB contacts retain chains, preserving conservative guide proof. Reports deterministic survivor/redundant IDs, exact before/after geometry and supported/unsupported/work counts. Max2048 selected tracks,1024 deletions,100k capture objects,1M hole candidates; two retained operations. No mutation; commit recomputes. No diagonal bend optimization, shove or physical certification.".into(),
            input_schema: input_schema::<viamind_engine::CleanupCardinalTracks>(),
        });
        commands.push(HostCommand { name: "preview_pcb_import".into(), description: "Read-only bounded standalone PCB S-expression preview. Exact nm front zero-angle footprints with exact cardinal rectangular SMD pads (-360..360 degrees), straight outer tracks, through vias and polygon Edge.Cuts outline/cutouts only. Returns per-list disposition/reasons and source IDs, explicit full_fidelity=false, source SHA256 plus current project/full cursor guards. Unknown copper/outline semantics block import. Regular file <=8MiB; no source writes, mutation or native-path activation.".into(), input_schema: input_schema::<viamind_engine::PreviewPcbImport>() });
        commands.push(HostCommand { name: "import_pcb".into(), description: "Replace a clean shared session with reparsed supported standalone PCB geometry guarded by exact expected_project, full expected_cursor and lowercase expected_source_sha256 from preview. Reject dirty native work, stale context, changed source or blocked fidelity. Imported project has fresh project UUID, preserved supported native object UUIDs, new event stream, empty history/selection, path null and dirty=true until native Save As. Source files never written. Lifecycle replacement is not an undoable edit; subsequent native edits are undoable.".into(), input_schema: input_schema::<viamind_engine::ImportPcb>() });
        commands.push(HostCommand {
            name: "new_project".into(),
            description: "Create a new unnamed project in this shared session (name defaults to Untitled). Resets history and selection; old event cursors require get_session_state resynchronization. Unsaved changes reject unless discard_dirty is true with the current expected_cursor. Stale cursors reject.".into(),
            input_schema: input_schema::<NewProject>(),
        });
        commands.push(HostCommand { name: "plan_pad_walkaround".into(), description: "Read-only bounded static cardinal walkaround plan with complete shared scoped joint DRC on the captured board snapshot. Same input as route_pad_walkaround: exact project/full cursor, same-net same-side quarter-turn explicit pad IDs, positive grid/width Nm and optional rules. Rectangular outer outline and rectangular cutouts only; cutout exclusions use route half-width plus board-edge clearance plus1Nm, separately from copper rules; combined conservative cutout/foreign copper/ALL via-hole boxes share existing limits; explicit obstacle/graph/work budgets and busy refusal. Result includes exact points/stubs, work counters, derived net/layer and source_cursor. No document/history/selection changes; commit recomputes. No shove or physical certification.".into(), input_schema: input_schema::<viamind_engine::RoutePadWalkaround>() });
        commands.push(HostCommand { name: "plan_pad_via_walkaround".into(), description: "Read-only checked two-side static cardinal walkaround through one explicit fixed grid-aligned new through via. Same typed input as route_pad_via_walkaround. One bounded board snapshot and retained admission; aggregate obstacle/graph/intersection/result budgets cover both legs. Rectangular outer outline/cutouts, exact quarter-turn opposite-side same-net pad endpoints. Success requires complete joint DRC of BOTH paths plus via, including annular/drill/edge/hole spacing. Returns exact paths, derived sides/net, full cursor and aggregate work; no mutation or automatic via relocation. Commit recomputes atomically.".into(), input_schema: input_schema::<viamind_engine::RoutePadViaWalkaround>() });
        commands.push(HostCommand { name:"query_route_endpoints".into(),description:"Read bounded immutable UUID-ordered explicit pad routing endpoints with exact quarter-turn integer centers, derived net/outer side, rendering corners and unsupported reasons. Legacy previews omitted; arbitrary rotations/unassigned nets cannot attach. offset 0, limit 1..1000 default 100, optional exact net. Max 100000 board structural objects / 2 MiB text / 512-byte labels (route net at most 256 bytes); no unrelated schematic capture. Preserve model/selection/history; source_cursor changes require restarting pages. No continuity or guide-completion claim.".into(),input_schema:input_schema::<viamind_engine::RouteEndpointQuery>() });
        commands.push(HostCommand {name:"query_copper_properties".into(), description:"Read one exact persisted track/via property object, project/full source cursor and bounded document net minima. Strict target is track {id} or via {id}. Selected-object net must fit128characters/256UTF-8bytes. No selection/history/status mutation or board/schematic/assembly clone; read-only observation. Use guarded set_track_properties/set_via_properties to edit explicit sizes/net/layer; no geometry movement or certification.".into(),input_schema:input_schema::<viamind_engine::CopperPropertyQuery>()});
        commands.push(HostCommand { name: "query_route_terminals".into(), description: "Read immutable stable explicit pad, existing track start/end and through-via center terminal identities. Via has one row per outer layer; exact stored Nm centers, derived net/layer, bounded labels and unsupported reasons. Optional exact net/layer filter; offset0, limit1..1000 default100; deterministic identity/layer ordering and full source cursor. Reject over100000 terminal rows before filtering/allocation; existing board structural/text/document-policy bounds apply. No schematic/assembly capture, mutation or continuity claim.".into(), input_schema: input_schema::<viamind_engine::RouteTerminalQuery>() });
        commands.push(HostCommand {
            name: "query_linked_pin_pads".into(),
            description: "Read-only stable component pin/explicit SMD pad mapping by exact unique terminal numbers. One component_id (part or symbol UUID), offset0, limit1..100 default100. Whole-component number bijection required for navigation; unlinked, legacy preview, mismatched and empty coverage explicit. Exact mirrored quarter-turn integer centers only; unsupported geometry has reasons. Max10000 combined sheets/symbols/parts/pins/pads,4096bytes/string,2MiB inspected text. Source_cursor pages; no electrical continuity, inferred links or session/selection/history changes.".into(),
            input_schema: input_schema::<viamind_engine::PinPadLinkQuery>(),
        });
        commands.push(HostCommand {
            name: "query_linked_components".into(),
            description: "Read-only stable UUID schematic-symbol/PCB-part mapping. Deterministic source-cursor pages, explicit unlinked/unresolved rows, unknown requested IDs and exact integer positions with bounded navigation flags. Optional object_ids up to100 unique IDs; offset0, limit1..100 default100. At most10000 combined sheets/symbols/parts,4096bytes per reference,2MiB aggregate. No reference inference, model/history/selection/dirty changes or pin/net cross-probing. Restart pages after cursor changes.".into(),
            input_schema: input_schema::<viamind_engine::ComponentLinkQuery>(),
        });
        commands.push(HostCommand {
            name: "run_erc".into(),
            description: "Read-only bounded native pin ERC over one exact schematic graph snapshot. Configurable symmetric allowed/warning/error pin-type matrix plus input connection/drive, missing power_output and no-connect attachment checks. Returns effective matrix, stable sheet/symbol/pin/net IDs, deterministic issues, source_cursor, complete-for-selected-checks flag and explicit full_erc=false coverage. Labels never supply power. Max10000 graph objects, 1000000 contact candidates, 250000 pin pairs and max_violations1..10000(default1000); partial policy prefixes explicitly incomplete. Graph ambiguity/limits reject. No hierarchy, buses, hidden power pins or all-ERC claim; preserves model/history/selection/dirty state.".into(),
            input_schema: input_schema::<viamind_engine::ErcInput>(),
        });
        commands.push(HostCommand {
            name: "preview_schematic_pcb_component".into(),
            description: "Read-only bounded preview for explicit creation of one linked PCB component from an unlinked schematic symbol and a validated native rectangular SMD footprint. Requires current expected_project/source_cursor plus placement with integer Nm position, side and quarter-turn rotation. Returns exact guarded create_schematic_pcb_component command, symbol properties and numbered stable graph net assignments. Rejects duplicate reference/link, mismatched assignment, pin/pad ambiguity, transformed corner bounds and result budgets. Preserves session and all existing geometry; no clearance/fabrication certification.".into(),
            input_schema: input_schema::<viamind_engine::SchematicComponentRequest>(),
        });
        commands.push(HostCommand {
            name: "preview_schematic_pcb_sync".into(),
            description: "Read-only bounded forward/back synchronization preview for explicitly UUID-linked existing PCB parts and schematic symbols. Returns exact guarded command, deterministic property/pad-net changes, physical net identities with schematic labels and unlinked exclusions. Forward requires explicit bijective numbered pads and matching footprint assignment; routed-net changes reject. Back updates references/values/footprint assignment without topology edits. Captures at most 10000 objects, 4 MiB text, 8 MiB serialized guard; exact graph limits also apply. No creation/deletion/inferred links. Does not change session/history/selection/dirty state.".into(),
            input_schema: input_schema::<viamind_engine::PcbSyncRequest>(),
        });
        commands.push(HostCommand {
            name: "query_schematic_connectivity".into(),
            description: "Read exact native schematic connected components from a bounded captured snapshot. Wire endpoint/T contacts and explicit junctions connect; interior crossings remain separate. Transformed pin endpoints, literal case-sensitive local/global/power labels; ambiguous crossing labels reject with IDs. Deterministic net/member IDs, aliases and coverage; offset0, limit1..100 default100. Restart pages when source_cursor changes. Max10000 objects including sheets/symbols/pins, 2MiB schematic text and 1000000 spatial candidates. No board annotation, ERC, buses, hierarchy or hidden power-pin inference. Read-only design/history/selection.".into(),
            input_schema: input_schema::<viamind_engine::SchematicConnectivityQuery>(),
        });
        commands.push(HostCommand {
            name: "preview_schematic_annotation".into(),
            description: "Read-only bounded flat schematic annotation preview. Returns changes and exact guarded annotate_schematic command input. Explicit complete sheet_order or UUID default; x_then_y/y_then_x, keep_existing/reset_all, inclusive start1..999999999. Does not alter history, dirty, selection or events. Reprepare if schematic changes; no PCB synchronization.".into(),
            input_schema: input_schema::<viamind_engine::AnnotationOptions>(),
        });
        commands.push(HostCommand {
            name:"query_schematic".into(),
            description:"Read UUID-ordered bounded schematic pages. Omit sheet_id for sheet summaries; supply it for tagged symbol/wire/junction/label objects. Integer nanometres. offset defaults0, limit1..100 defaults100. Read-only; restart pages after edits. get_session_state supplies exact complete sheet snapshots before guarded edits. Electrical connectivity is not evaluated.".into(),
            input_schema:input_schema::<viamind_engine::SchematicQuery>(),
        });
        commands.push(HostCommand {
            name: "import_footprint".into(),
            description: "Load a bounded native viamind-footprint version1 JSON library from a local file and apply with exact project/part guards. Rectangular SMD local Nm geometry only. Matching numbers retain pad IDs/nets; component placement is unchanged. One undoable edit. Does not read KiCad files.".into(),
            input_schema: input_schema::<viamind_engine::FootprintFile>(),
        });
        commands.push(HostCommand {
            name: "export_footprint".into(),
            description: "Export committed explicit rectangular SMD geometry and footprint name to a new native library file. Exact project/part guards; no UUIDs, nets or component placement are exported. Refuses existing paths and protects active native project. Read-only design/history operation; parent directory must exist.".into(),
            input_schema: input_schema::<viamind_engine::FootprintFile>(),
        });
        commands.push(HostCommand {
            name: "open_project".into(),
            description: "Open a native file on this machine in the shared desktop/agent session. Validates before replacement; failed reads leave the current document intact. Resets history and selection. Unsaved changes require discard_dirty with the current expected_cursor from get_session_state. Concurrent edits during loading reject; old event cursors require resynchronization.".into(),
            input_schema: input_schema::<OpenProject>(),
        });
        commands.push(HostCommand {
            name: "get_document_status".into(),
            description: "Read the shared active native file path (null if unnamed) and dirty state. Dirty means the design differs from its saved baseline; selection does not count.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        });
        commands.push(HostCommand {
            name: "save_project".into(),
            description: "Save an atomic native project snapshot on this machine. Omit path to replace the active file; for a new path, existing files are preserved unless overwrite is true. Returns absolute path, dirty (newer edits may remain unsaved) and saved_cursor. Updates shared file status; preserves undo/redo and emits no design transaction. Parent directory must exist.".into(),
            input_schema: input_schema::<SaveProject>(),
        });
        commands.push(HostCommand {
            name: "get_session_state".into(),
            description: "Read an atomic project, selection and event cursor snapshot. Initialize or resynchronize your local view here, then call watch_events after this cursor to receive every subsequent change.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        });
        commands.push(HostCommand {
            name: "watch_events".into(),
            description: "Read design and selection changes in session order. Pass the last cursor as after, including after reconnecting. Omitted after starts at current tail. limit defaults to 100 (1–1000); timeout_ms defaults to 0 (0–30000), waiting only if no events are available. Returns events, cursor and has_more. History retains 1024 records; on expired/future/restarted cursor errors call get_session_state to resynchronize. Read-only; use one outstanding watch per observer.".into(),
            input_schema: input_schema::<WatchEvents>(),
        });
        commands.push(HostCommand {
            name: "configure_auto_drc".into(),
            description: "Configure transient shared automatic scoped DRC. One debounced worker checks bounded snapshots outside the engine lock; edits supersede old results. Does not modify design, dirty state or history. Snapshot limits may skip checks; reports retain fabrication coverage exclusions. Worker timings are not desktop edit-to-visible latency.".into(),
            input_schema: input_schema::<viamind_engine::AutoDrcConfig>(),
        });
        commands.push(HostCommand {
            name: "get_auto_drc_status".into(),
            description: "Read shared automatic DRC config, generation, pending/running/ready/skipped/failed state, current scoped report, limits and measured worker latency. A report is present only when current. No project clone or design transaction.".into(),
            input_schema: json!({"type":"object","properties":{},"additionalProperties":false}),
        });
        commands.push(HostCommand {
            name: "get_auto_drc_event_state".into(),
            description: "Read an atomic current automatic scoped DRC status and its independent event cursor. Initialize/resynchronize here, then watch_auto_drc_events after this cursor. Detailed reports exist only in current ready status; match generation. Preserves document cursors, history and native bytes.".into(),
            input_schema: json!({"type":"object","properties":{},"additionalProperties":false}),
        });
        commands.push(HostCommand {
            name: "watch_auto_drc_events".into(),
            description: "Read automatic scoped DRC transition summaries from an independent bounded128-record journal. Reuse after cursor across initialized connections/reconnects; omitted/null after starts at current tail. limit defaults100 (1..128), timeout_ms defaults0 (0..30000). Returns events,cursor,has_more. Wrong/future/expired cursors require get_auto_drc_event_state. No full report history, design cursor mutation, or protocol resource subscription. Use one outstanding watch per observer.".into(),
            input_schema: input_schema::<viamind_engine::WatchAutoDrcEvents>(),
        });
        commands.push(HostCommand {
            name: "run_drc".into(),
            description: "Read-only scoped batch DRC of a captured design: rectangular SMD pads, round-ended straight tracks and annular through-via copper; same-layer different-net/unassigned clearance, polygon substrate/edge clearance, minimum via drill/annular width, physical drilled-hole spacing and foreign copper-to-hole clearance. Returns stable tagged part/pad, track and via IDs, measured/required Nm, source_cursor and explicit coverage/exclusions. All six distance rules finite 0..100 mm; max_violations 1..10000. Same assigned nets exempt copper/foreign-hole checks, but not physical hole spacing. Legacy previews and remaining fabrication/connectivity checks excluded. Limited work/results are explicitly incomplete. Not full fabrication DRC.".into(),
            input_schema: input_schema::<viamind_drc::DrcInput>(),
        });
        commands.push(HostCommand {
            name: "query_parts".into(),
            description: "Read a bounded page of board parts using combined filters (IDs, exact fields, text, side, origin bounds in mm). Read-only. UUID order; limit defaults to 100 (max 1000). Repeat filters with next_after as after for the next page; restart if the design changes. Returns parts, total matches and next_after; positions are integer nanometres.".into(),
            input_schema: input_schema::<PartQuery>(),
        });
        commands.push(HostCommand {
            name: "query_nets".into(),
            description: "Read named-net summaries in exact case-sensitive name order from explicit pads only. Read-only; terminal and conservative remaining guide counts exclude legacy preview pads. Whole-board copper_proof reports bounded exact-endpoint coverage/fallback, not full connectivity or DRC certification. limit defaults to 100 (max1000), after is the last net name. Repeat pages only while the design is unchanged.".into(),
            input_schema: input_schema::<NetQuery>(),
        });
        commands.push(HostCommand {
            name: "query_connectivity".into(),
            description: "Read exact named-net terminal membership and unrouted connection guides. Explicit pads only; bounded exact same-net/side track endpoints and through-via bridges suppress proven connected islands. copper_proof reports whole-board coverage/fallback. Unsupported transforms, contacts and excessive inputs retain conservative extra guides; no full physical connectivity certification. Coordinates are transformed pad centres in mm. Exact dense_prim for <=256 terminals, approximate coordinate_chain for larger nets. Guides are not copper or DRC. Arrays share offset and limit (default100,max1000); repeat pages only while unchanged.".into(),
            input_schema: input_schema::<ConnectivityQuery>(),
        });
        commands.push(HostCommand {
            name: "set_selection".into(),
            description: "Select persisted board part, track and via IDs in the shared desktop session. Modes: replace (default), add, remove, toggle. Empty replace clears selection. Transient: preserves design undo and redo history.".into(),
            input_schema: input_schema::<SetSelection>(),
        });
        commands.push(HostCommand {
            name: "get_selection".into(),
            description: "Read the shared selection: sorted part/track/via IDs, revision and last origin.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        });
        commands.push(HostCommand {
            name: "snapshot_schematic_wire_sheet".into(),
            description: "Read one bounded validated schematic sheet and exact project/stream/cursor for wire drawing; no mutation.".into(),
            input_schema: input_schema::<viamind_engine::SchematicWireSheetQuery>(),
        });
        commands
    }

    fn execute(&self, name: &str, input: Value, agent: &AgentId) -> Result<Value, String> {
        if name == "inspect_fabrication_files" {
            let input = serde_json::from_value(input).map_err(|e| e.to_string())?;
            return serde_json::to_value(self.engine.inspect_fabrication_files(input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "load_footprint_definition" {
            preflight_footprint_definition(&input, false)?;
            let input = serde_json::from_value(input).map_err(|e| e.to_string())?;
            return serde_json::to_value(self.engine.load_footprint_definition(input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "save_footprint_definition" {
            preflight_footprint_definition(&input, true)?;
            let input = serde_json::from_value(input).map_err(|e| e.to_string())?;
            return serde_json::to_value(self.engine.save_footprint_definition(input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "preview_footprint_conversion" {
            let input = serde_json::from_value(input).map_err(|e| e.to_string())?;
            return serde_json::to_value(self.engine.preview_footprint_conversion(input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "convert_footprint_file" {
            let input = serde_json::from_value(input).map_err(|e| e.to_string())?;
            return serde_json::to_value(self.engine.convert_footprint_file(input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "preview_pcb_import" {
            let input = serde_json::from_value(input).map_err(|e| e.to_string())?;
            return serde_json::to_value(self.engine.preview_pcb_import(input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "import_pcb" {
            let input = serde_json::from_value(input).map_err(|e| e.to_string())?;
            return serde_json::to_value(self.engine.import_pcb(input, Self::origin(agent)).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "preview_schematic_pcb_component" {
            let input = serde_json::from_value(input).map_err(|e| e.to_string())?;
            return serde_json::to_value(self.engine.preview_schematic_pcb_component(input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "snapshot_schematic_wire_sheet" {
            let input = serde_json::from_value(input).map_err(|e| format!("invalid wire snapshot: {e}"))?;
            return serde_json::to_value(self.engine.snapshot_schematic_wire_sheet(&input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "preview_schematic_pcb_sync" {
            let input: viamind_engine::PcbSyncRequest = serde_json::from_value(input).map_err(|e| e.to_string())?;
            return serde_json::to_value(self.engine.preview_schematic_pcb_sync(input.direction).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string());
        }
        if name == "query_named_net_rules" {
            if input != json!({}) {
                return Err("query_named_net_rules expects an empty object".into());
            }
            return serde_json::to_value(self.engine.query_named_net_rules().map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "plan_cardinal_track_cleanup" {
            let input = serde_json::from_value(input).map_err(|e| format!("invalid cleanup input: {e}"))?;
            return serde_json::to_value(self.engine.plan_cardinal_track_cleanup(&input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "plan_pad_walkaround" {
            let input = serde_json::from_value(input).map_err(|e| format!("invalid walkaround input: {e}"))?;
            return serde_json::to_value(self.engine.plan_pad_walkaround(&input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "plan_pad_via_walkaround" {
            let input = serde_json::from_value(input).map_err(|e| format!("invalid via walkaround input: {e}"))?;
            return serde_json::to_value(self.engine.plan_pad_via_walkaround(&input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "query_route_endpoints" {
            let input = serde_json::from_value(input).map_err(|e| format!("invalid endpoint query: {e}"))?;
            return serde_json::to_value(self.engine.query_route_endpoints(&input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "query_copper_properties" {
            let input: viamind_engine::CopperPropertyQuery = serde_json::from_value(input).map_err(|e| e.to_string())?;
            return serde_json::to_value(self.engine.query_copper_properties(&input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "query_route_terminals" {
            if input.get("net").and_then(Value::as_str).is_some_and(|n| n.len() > 256) {
                return Err("terminal query net requires at most256bytes before typed decoding".into());
            }
            let input = serde_json::from_value(input).map_err(|e| format!("invalid terminal query: {e}"))?;
            return serde_json::to_value(self.engine.query_route_terminals(&input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "query_linked_pin_pads" {
            let input = serde_json::from_value(input).map_err(|e| format!("invalid pin/pad query: {e}"))?;
            return serde_json::to_value(self.engine.query_linked_pin_pads(&input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "query_linked_components" {
            let input: viamind_engine::ComponentLinkQuery = serde_json::from_value(input).map_err(|e| e.to_string())?;
            return serde_json::to_value(self.engine.query_linked_components(&input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "run_erc" {
            let input: viamind_engine::ErcInput = serde_json::from_value(input).map_err(|e| e.to_string())?;
            return serde_json::to_value(self.engine.run_erc(&input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "query_schematic_connectivity" {
            let input: viamind_engine::SchematicConnectivityQuery = serde_json::from_value(input).map_err(|e| e.to_string())?;
            return serde_json::to_value(self.engine.query_schematic_connectivity(&input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "preview_schematic_annotation" {
            let options = serde_json::from_value(input).map_err(|e| format!("invalid annotation options: {e}"))?;
            return serde_json::to_value(self.engine.preview_schematic_annotation(options).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "query_schematic" {
            let input: viamind_engine::SchematicQuery = serde_json::from_value(input).map_err(|e| e.to_string())?;
            return serde_json::to_value(self.engine.query_schematic(&input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "import_footprint" || name == "export_footprint" {
            let input: viamind_engine::FootprintFile = serde_json::from_value(input).map_err(|e| e.to_string())?;
            return if name == "import_footprint" {
                serde_json::to_value(self.engine.import_footprint(input, Self::origin(agent)).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
            } else {
                Ok(json!({"path":self.engine.export_footprint(input).map_err(|e| e.to_string())?}))
            };
        }
        if name == "new_project" {
            let input: NewProject = serde_json::from_value(input).map_err(|e| format!("invalid new_project input: {e}"))?;
            let result = self.engine.new_project(input, Self::origin(agent)).map_err(|e| e.to_string())?;
            return serde_json::to_value(result).map_err(|e| e.to_string());
        }
        if name == "open_project" {
            let input: OpenProject = serde_json::from_value(input).map_err(|e| format!("invalid open_project input: {e}"))?;
            let result = self.engine.open_project(input, Self::origin(agent)).map_err(|e| e.to_string())?;
            return serde_json::to_value(result).map_err(|e| e.to_string());
        }
        if name == "get_document_status" {
            if !input.as_object().is_some_and(serde_json::Map::is_empty) {
                return Err("get_document_status takes an empty object".into());
            }
            return serde_json::to_value(self.engine.document_status()).map_err(|error| error.to_string());
        }
        if name == "save_project" {
            let input: SaveProject = serde_json::from_value(input).map_err(|error| format!("invalid save_project input: {error}"))?;
            let result = self.engine.save_project(input).map_err(|error| error.to_string())?;
            return serde_json::to_value(result).map_err(|error| error.to_string());
        }
        if name == "get_session_state" {
            if !input.as_object().is_some_and(serde_json::Map::is_empty) {
                return Err("get_session_state takes an empty object".into());
            }
            return serde_json::to_value(self.engine.session_state()).map_err(|e| e.to_string());
        }
        if name == "watch_events" {
            let input: WatchEvents = serde_json::from_value(input).map_err(|e| format!("invalid watch_events input: {e}"))?;
            let result = self.engine.watch_events(input).map_err(|e| e.to_string())?;
            return serde_json::to_value(result).map_err(|e| e.to_string());
        }
        if name == "configure_auto_drc" {
            let config = serde_json::from_value(input).map_err(|e| format!("invalid configure_auto_drc input: {e}"))?;
            return serde_json::to_value(self.engine.configure_auto_drc(config).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "get_auto_drc_status" {
            if !input.as_object().is_some_and(serde_json::Map::is_empty) {
                return Err("get_auto_drc_status takes an empty object".into());
            }
            return serde_json::to_value(self.engine.auto_drc_status()).map_err(|e| e.to_string());
        }
        if name == "get_auto_drc_event_state" {
            if !input.as_object().is_some_and(serde_json::Map::is_empty) {
                return Err("get_auto_drc_event_state takes an empty object".into());
            }
            return serde_json::to_value(self.engine.auto_drc_event_state()).map_err(|e| e.to_string());
        }
        if name == "watch_auto_drc_events" {
            let input = serde_json::from_value(input).map_err(|e| format!("invalid watch_auto_drc_events input: {e}"))?;
            return serde_json::to_value(self.engine.watch_auto_drc_events(input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        if name == "run_drc" {
            let input: viamind_drc::DrcInput = serde_json::from_value(input).map_err(|e| format!("invalid run_drc input: {e}"))?;
            input.validate().map_err(|e| e.to_string())?;
            let snapshot = self.engine.with_engine(|e| {
                viamind_doc::named_net_rule_budget(&e.project().board.net_rules).map_err(str::to_owned)?;
                Ok::<_, String>(e.session_state())
            })?;
            let report = viamind_drc::run_drc(&snapshot.project, &input).map_err(|e| e.to_string())?;
            return Ok(json!({"source_cursor":snapshot.cursor,"report":report}));
        }
        if name == "query_parts" {
            let query: PartQuery = serde_json::from_value(input).map_err(|e| format!("invalid query_parts input: {e}"))?;
            let result = self.engine.query_parts(&query).map_err(|e| e.to_string())?;
            return serde_json::to_value(result).map_err(|e| e.to_string());
        }
        if name == "query_nets" {
            let query: NetQuery = serde_json::from_value(input).map_err(|e| format!("invalid query_nets input: {e}"))?;
            let result = self.engine.query_nets(&query).map_err(|e| e.to_string())?;
            return serde_json::to_value(result).map_err(|e| e.to_string());
        }
        if name == "query_connectivity" {
            let query: ConnectivityQuery = serde_json::from_value(input).map_err(|e| format!("invalid query_connectivity input: {e}"))?;
            let result = self.engine.query_connectivity(&query).map_err(|e| e.to_string())?;
            return serde_json::to_value(result).map_err(|e| e.to_string());
        }
        if name == "get_selection" {
            if !input.as_object().is_some_and(serde_json::Map::is_empty) {
                return Err("get_selection takes an empty object".into());
            }
            return serde_json::to_value(self.engine.selection()).map_err(|e| e.to_string());
        }
        if name == "set_selection" {
            let input: SetSelection = serde_json::from_value(input).map_err(|e| format!("invalid set_selection input: {e}"))?;
            let selection = self.engine.set_selection(input, Self::origin(agent)).map_err(|e| e.to_string())?;
            return serde_json::to_value(selection).map_err(|e| e.to_string());
        }
        let result = self.engine.execute(name, input, Self::origin(agent)).map_err(|e| e.to_string())?;
        serde_json::to_value(result).map_err(|e| format!("internal error: could not encode the result ({e})"))
    }

    fn undo(&self, agent: &AgentId) -> Result<Value, String> {
        match self.engine.undo_own(Self::origin(agent)).map_err(|e| e.to_string())? {
            Some(tx) => Ok(json!({ "undone": tx })),
            None => Err("nothing to undo: you have no changes left to undo on this connection".into()),
        }
    }

    fn redo(&self, agent: &AgentId) -> Result<Value, String> {
        match self.engine.redo_own(Self::origin(agent)).map_err(|e| e.to_string())? {
            Some(tx) => Ok(json!({ "redone": tx })),
            None => Err("nothing to redo: undo one of your changes first".into()),
        }
    }

    fn snapshot(&self) -> Value {
        self.engine.read(|project| serde_json::to_value(project).unwrap_or_else(|e| json!({ "error": e.to_string() })))
    }
}
