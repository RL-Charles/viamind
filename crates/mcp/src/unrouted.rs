//! Admit the complete observation before queueing its blocking worker.
use crate::{RegistryError, ToolError, ToolRegistry};
use viamind_engine::{EngineHandle, UnroutedNetQuery, input_schema};

pub(crate) fn register(registry: &mut ToolRegistry, engine: EngineHandle) -> Result<(), RegistryError> {
    registry.register_observation(
        "query_unrouted_nets",
        "Read a complete bounded board net inventory in ONE call, with exact project/full source cursor and connected/disconnected/insufficient_terminals/unknown rows. Actual doubled-integer quarter-turn rectangular pads, cardinal round-ended tracks and intact through-via annuli, including interior contacts; not guide counts. Whole-call limits:100000 structural objects,10000 nets,2MiB relevant net text,1M contact/hole candidates,8MiB result; two nonwaiting retained workers. Unsupported diagonal tracks/arbitrary pad angles and drill-affected geometry give explicit unknown; excluded legacy parts make coverage incomplete. Includes pad/copper/rule-defined names; zero/one terminal is not an interterminal routing obligation. No pagination, physical continuity/DRC/DFM certification, selection/history/session/native mutation or publication. Available in every agent mode.",
        input_schema::<UnroutedNetQuery>(),
        move |input| {
            let engine = engine.clone();
            async move {
                let query = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput(format!("invalid unrouted-net query: {e}")))?;
                let work = engine.prepare_unrouted_nets(query).map_err(|e| ToolError::Failed(e.to_string()))?;
                tokio::task::spawn_blocking(move || work.run_value()).await
                    .map_err(|e| ToolError::Failed(format!("unrouted-net worker stopped: {e}")))?
                    .map_err(|e| ToolError::Failed(e.to_string()))
            }
        },
    )
}
