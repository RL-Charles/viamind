//! Viamind command engine: typed commands, transactions, undo and the event log.
//!
//! Every change to a [`viamind_doc::Project`] is a [`Command`]: a serde +
//! schemars input type whose [`apply`](Command::apply) returns the [`Delta`]s it
//! made. Deltas are full before/after snapshots, so undo is generic
//! ([`invert`]) and the same deltas are the change notifications. A
//! [`CommandRegistry`] lists commands as [`CommandSpec`]s (name, description,
//! JSON Schema) and runs them from JSON, which is how agents call them over MCP.
//!
//! An [`Engine`] owns the project and runs every edit as a transaction with
//! per-transaction undo/redo; an [`EngineHandle`] shares one engine between the
//! UI thread and the MCP server.
//!
//! ```
//! use serde_json::json;
//! use viamind_doc::Project;
//! use viamind_engine::{EngineHandle, Origin};
//!
//! let engine = EngineHandle::new(Project::new("demo"));
//! let input = json!({ "designator": "R1", "value": "10k", "footprint": "R_0603", "x_mm": 10.0, "y_mm": 5.0 });
//! let result = engine.execute("add_part", input, Origin::agent("claude")).unwrap();
//! let id = result.created[0];
//! assert_eq!(engine.read(|p| p.board.part(&id).unwrap().designator.clone()), "R1");
//!
//! engine.undo(Origin::User).unwrap(); // the user undoes the agent's edit
//! assert!(engine.read(|p| p.board.parts.is_empty()));
//! ```

pub mod assembly;
pub use assembly::SetPartAssembly;
pub mod annotation;
pub use annotation::{AnnotateSchematic, AnnotationChange, AnnotationMode, AnnotationOptions, AnnotationOrder, AnnotationPreview};
pub mod auto_drc;
mod auto_drc_events;
pub use auto_drc::{AutoDrcConfig, AutoDrcPhase, AutoDrcStatus};
pub use auto_drc_events::{
    AUTO_DRC_EVENT_CAPACITY, AutoDrcEvent, AutoDrcEventBatch, AutoDrcEventCursor, AutoDrcEventError, AutoDrcEventState, AutoDrcReportSummary,
    WatchAutoDrcEvents,
};
pub mod command;
pub mod commands;
pub mod connectivity;
mod copper_connectivity;
pub use copper_connectivity::CopperConnectivityCoverage;
pub mod delta;
pub mod document;
pub mod engine;
mod erc;
pub mod event;
pub use erc::{ErcCheck, ErcCoverage, ErcInput, ErcReport, ErcSeverity, ErcViolation, PinCompatibilityRule};
mod component_links;
mod pin_pad_links;
pub use pin_pad_links::{LinkedPinPad, PinPadCoverage, PinPadLinkQuery, PinPadLinkResult};
pub mod footprint;
pub mod handle;
pub mod log;
pub mod pads;
pub use component_links::{ComponentLinkQuery, ComponentLinkResult, ComponentLinkStatus, LinkedComponent};
mod pcb_sync;
mod schematic_component;
pub use pcb_sync::{
    LinkSchematicSymbol, PcbSyncChange, PcbSyncDirection, PcbSyncNet, PcbSyncPreview, PcbSyncRequest, SchematicLinkTarget, SynchronizeSchematicPcb,
};
pub use schematic_component::{
    ComponentPadNet, CreateSchematicPcbComponent, SchematicComponentPlacement, SchematicComponentPreview, SchematicComponentRequest,
};
pub mod schematic;
mod schematic_wires;
pub use schematic_wires::{DrawSchematicWirePolyline, SchematicWireSheetQuery, SchematicWireSnapshot};
mod schematic_connectivity;
pub use footprint::{ApplyFootprint, FootprintFile};
pub use schematic::{DeleteSchematicSheet, EditSchematicSheet, SchematicEdit, SchematicQuery, SchematicQueryResult, SymbolInput, SymbolPinInput};
pub use schematic_connectivity::{
    SchematicConnectivityCoverage, SchematicConnectivityQuery, SchematicConnectivityResult, SchematicMemberKind, SchematicNet, SchematicNetMember,
    SchematicNetName,
};
pub mod route_endpoints;
pub mod routing;
pub use route_endpoints::{PadEndpointId, RouteEndpoint, RouteEndpointPage, RouteEndpointQuery, RouteEndpointSnapshot, RoutePadPolyline};
pub use routing::RoutePolyline;
pub mod placement;
pub mod query;
pub mod registry;
pub mod selection;
pub mod session;
pub mod transaction;

pub use command::{Command, CommandError, CommandSpec, input_schema};
pub use commands::{AddPart, DeletePart, ExpectedOutline, ImportBoardOutlineDxf, MovePart, OutlineMm, OutlinePoint, SetBoardOutline, UpdatePart};
pub use connectivity::{ConnectionGuide, ConnectivityQuery, ConnectivityResult, NetQuery, NetQueryResult, NetSummary, Terminal, connection_guides};
pub use delta::{Delta, apply_all, coalesce, invert};
pub use document::{DocumentStatus, NewProject, OpenProject, ReplaceGuard, SaveProject, SaveResult};
pub use engine::{Engine, Tx};
pub use event::{ChangeEvent, ChangeKind, Waker};
pub use handle::EngineHandle;
pub use log::{EventLog, LogError};
pub use pads::{PadInput, SetPartPads};
pub use placement::{OriginAlignment, PartPlacement, PlacementAxis, TransformParts};
pub use query::{PartQuery, PartQueryBounds, PartQueryResult};
pub use registry::{CommandRegistry, EngineError};
/// Re-exported so command input types can derive `JsonSchema` with a matching version.
pub use schemars;
pub use selection::{SelectionMode, SelectionSnapshot, SetSelection};
pub use session::{EventBatch, EventCursor, SESSION_EVENT_CAPACITY, SessionError, SessionEvent, SessionEventRecord, SessionSnapshot, WatchEvents};
pub use transaction::{CommandCall, ExecResult, Origin, Transaction, TxId, TxInfo};

pub mod copper;
pub use copper::{AddTrack, AddVia, DeleteTrack, DeleteVia};

pub mod route_via;
pub use route_via::RoutePadViaPolyline;

pub mod route_posture;
pub use route_posture::RoutePadPosture;
pub mod walkaround;
pub use walkaround::{CheckedWalkaround, RoutePadWalkaround};
pub mod via_walkaround;
pub use via_walkaround::{CheckedViaWalkaround, RoutePadViaWalkaround};
pub mod cardinal_cleanup;
pub use cardinal_cleanup::{CardinalTrackReplacement, CheckedCardinalCleanup, CleanupCardinalTracks, CleanupCounts, CleanupTracks};
mod schematic_budget;

pub mod symbol_library;
pub use symbol_library::{
    ExportSymbolTemplate, LoadSymbolTemplate, PlaceSymbolTemplate, SaveSymbolTemplate, SymbolSheetQuery, SymbolSheetSnapshot,
    SymbolTemplateExportResult, SymbolTemplateSaveResult,
};

mod pcb_import;
pub use pcb_import::{ImportPcb, PcbImportPreview, PcbImportResult, PreviewPcbImport};

mod fabrication_view;
pub use fabrication_view::{FabricationInspection, InspectFabricationFiles, InspectedFabricationFile};

mod footprint_conversion;
pub use footprint_conversion::{ConvertFootprintFile, ConvertedFootprintFile, FootprintConversionPreview, PreviewFootprintConversion};

mod checked_output_job;
pub use checked_output_job::checked_output_job_report;

mod net_rules;
pub use net_rules::{NamedNetRuleRow, NamedNetRuleSnapshot, SetNamedNetRules};

mod footprint_definition;
pub use footprint_definition::{FootprintDefinitionSaved, LoadFootprintDefinition, SaveFootprintDefinition, validate_definition};

mod agent_presence;
pub use agent_presence::{
    AGENT_PRESENCE_CAPACITY, AGENT_PRESENCE_LEASE, AgentPresence, AgentPresenceSnapshot, ClearAgentPresence, PresencePoint, SetAgentPresence,
};
