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

impl CommandHost for EngineHost {
    fn commands(&self) -> Vec<HostCommand> {
        let mut commands: Vec<_> = self
            .engine
            .specs()
            .into_iter()
            .map(|spec| HostCommand { name: spec.name, description: spec.description, input_schema: spec.input_schema })
            .collect();
        commands.push(HostCommand {
            name: "new_project".into(),
            description: "Create a new unnamed project in this shared session (name defaults to Untitled). Resets history and selection; old event cursors require get_session_state resynchronization. Unsaved changes reject unless discard_dirty is true with the current expected_cursor. Stale cursors reject.".into(),
            input_schema: input_schema::<NewProject>(),
        });
        commands.push(HostCommand { name:"query_route_endpoints".into(),description:"Read bounded immutable UUID-ordered explicit pad routing endpoints with exact quarter-turn integer centers, derived net/outer side, rendering corners and unsupported reasons. Legacy previews omitted; arbitrary rotations/unassigned nets cannot attach. offset 0, limit 1..1000 default 100, optional exact net. Max 100000 board structural objects / 2 MiB text / 512-byte labels (route net at most 256 bytes); no unrelated schematic capture. Preserve model/selection/history; source_cursor changes require restarting pages. No continuity or guide-completion claim.".into(),input_schema:input_schema::<viamind_engine::RouteEndpointQuery>() });
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
        if name == "query_route_endpoints" {
            let input = serde_json::from_value(input).map_err(|e| format!("invalid endpoint query: {e}"))?;
            return serde_json::to_value(self.engine.query_route_endpoints(&input).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
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
        if name == "run_drc" {
            let input: viamind_drc::DrcInput = serde_json::from_value(input).map_err(|e| format!("invalid run_drc input: {e}"))?;
            input.validate().map_err(|e| e.to_string())?;
            let snapshot = self.engine.session_state();
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
