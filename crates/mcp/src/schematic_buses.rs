//! Bus observations retain admission until bounded worker serialization retires.
use crate::{RegistryError, ToolError, ToolRegistry};
use viamind_engine::{EngineHandle, SchematicBusesQuery, UnfoldSchematicBusMember, input_schema};
pub(crate) fn register(registry: &mut ToolRegistry, engine: EngineHandle) -> Result<(), RegistryError> {
    let query_engine = engine.clone();
    registry.register_observation("query_schematic_buses", "Read complete explicit bus definitions, ordered expanded members, owned entries and actual same-sheet scalar label UUID/scope/net facts in one coherent snapshot. Bus/entry graphics never conduct; existing bracket labels stay literal. Maximum256 buses/4096 entries/10000 expanded members,10000 graph rows/2MiB text,100000 global IDs, cumulative1M visits/2MiB output/two retained workers. Whole-call refusal on limits; no partial page, hierarchy, implicit cross-sheet inference, ERC or PCB continuity claim. Available in all agent modes.",input_schema::<SchematicBusesQuery>(),move |input| {let engine=query_engine.clone(); async move {
        let query=serde_json::from_value(input).map_err(|e|ToolError::InvalidInput(format!("invalid bus query: {e}")))?;
        let work=engine.prepare_schematic_buses(&query).map_err(|e|ToolError::Failed(e.to_string()))?;
        tokio::task::spawn_blocking(move||work.run_value()).await.map_err(|e|ToolError::Failed(format!("bus query worker stopped: {e}")))?.map_err(|e|ToolError::Failed(e.to_string()))
    }})?;
    registry.register_observation("preview_schematic_bus_unfold", "Preview exact member unfolding against required project/full source cursor/sheet guards. Ordinary scalar wire and Local label connect only the explicit opposite endpoint and same-sheet exact Local-member components. Actual source/candidate graph UUID partitions and label scopes must preserve all unrelated topology; incidental shorts and budget exhaustion refuse. Preview is immutable and uses temporary identities. Same retained bounded workers as query_schematic_buses. Apply with unfold_schematic_bus_member, which recalculates from its guarded source. Available in all agent modes; no ERC/PCB certification.",input_schema::<UnfoldSchematicBusMember>(),move |input| {let engine=engine.clone();async move {
        let request=serde_json::from_value(input).map_err(|e|ToolError::InvalidInput(format!("invalid bus unfold preview: {e}")))?;
        let work=engine.prepare_schematic_bus_unfold(request).map_err(|e|ToolError::Failed(e.to_string()))?;
        tokio::task::spawn_blocking(move||work.run_value()).await.map_err(|e|ToolError::Failed(format!("bus unfold worker stopped: {e}")))?.map_err(|e|ToolError::Failed(e.to_string()))
    }})
}
