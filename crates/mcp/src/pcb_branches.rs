//! Transport-owned isolated PCB preparation. Admission precedes blocking spawn.
use crate::{CallContext, RegistryError, ToolError, ToolRegistry};
use serde_json::{Value, json};
use viamind_engine::{EngineHandle, input_schema, pcb_branches::*};

#[derive(Clone, Copy)]
enum Operation {
    Create,
    Apply,
    Inspect,
    List,
    Diff,
    Merge,
    Discard,
}
fn failed(error: impl std::fmt::Display) -> ToolError {
    ToolError::Failed(error.to_string())
}
fn decode<T: serde::de::DeserializeOwned>(input: Value) -> Result<T, ToolError> {
    serde_json::from_value(input).map_err(failed)
}
fn encode<T: serde::Serialize>(output: T) -> Result<Value, ToolError> {
    serde_json::to_value(output).map_err(failed)
}

async fn run(engine: EngineHandle, operation: Operation, input: Value, ctx: CallContext) -> Result<Value, ToolError> {
    EngineHandle::validate_pcb_branch_input(&input).map_err(failed)?;
    let actor = match operation {
        Operation::List | Operation::Diff => None,
        _ => {
            if ctx.agent.0.len() > 128 || ctx.agent.0.chars().any(char::is_control) {
                return Err(failed(
                    "PCB branch creator identity must fit128 UTF-8 bytes including connection suffix, without control characters; reconnect with a shorter client name",
                ));
            }
            Some(BranchActor::agent(ctx.connection.ok_or_else(|| failed("PCB branches require a trusted MCP connection"))?, &ctx.agent.0))
        }
    };
    // Acquired before spawn: canceling the awaiter cannot admit a second worker.
    let permit = if matches!(operation, Operation::Create | Operation::Apply) { Some(reserve_pcb_branch_work().map_err(failed)?) } else { None };
    tokio::task::spawn_blocking(move || execute(&engine, operation, input, actor, permit)).await.map_err(failed)?
}
fn execute(
    engine: &EngineHandle,
    operation: Operation,
    input: Value,
    actor: Option<BranchActor>,
    permit: Option<BranchWorkPermit>,
) -> Result<Value, ToolError> {
    match operation {
        Operation::Create => {
            encode(engine.create_pcb_branch(decode(input)?, actor.expect("actor operation"), permit.expect("work operation")).map_err(failed)?)
        }
        Operation::Apply => {
            encode(engine.apply_pcb_branch(decode(input)?, &actor.expect("actor operation"), permit.expect("work operation")).map_err(failed)?)
        }
        Operation::Inspect => encode(engine.inspect_pcb_branch(decode(input)?, &actor.expect("actor operation")).map_err(failed)?),
        Operation::List => {
            if !input.as_object().is_some_and(serde_json::Map::is_empty) {
                return Err(failed("list_pcb_branches takes an empty object"));
            }
            encode(engine.list_pcb_branches())
        }
        Operation::Diff => encode(engine.diff_pcb_branch(decode(input)?).map_err(failed)?),
        Operation::Merge => encode(engine.merge_pcb_branch(decode(input)?, &actor.expect("actor operation")).map_err(failed)?),
        Operation::Discard => encode(engine.discard_pcb_branch(decode(input)?, &actor.expect("actor operation")).map_err(failed)?),
    }
}
pub fn register(registry: &mut ToolRegistry, engine: EngineHandle) -> Result<(), RegistryError> {
    let definitions = [
        (
            "create_pcb_branch",
            Operation::Create,
            input_schema::<CreatePcbBranch>(),
            "Capture one isolated PCB candidate at exact project/full cursor. Label1..128 UTF8 bytes. At most4 branches,5000 whole-project structural rows,2MiB serialized project; one nonwaiting worker. No live document/history/selection/native mutation. Branches expire after5minutes idle. Voluntary preparation; ordinary live edits remain available.",
        ),
        (
            "apply_pcb_branch",
            Operation::Apply,
            input_schema::<ApplyPcbBranch>(),
            "Atomically prepare1..64 original PCB commands in an owned branch at exact revision; whole input64KiB, candidate2MiB/5000rows, coalesced128changes/2MiB checked after every step. Allowed add_part/move_part/delete_part/update_part/transform_parts/set_part_pads/add_track/delete_track/add_via/delete_via. Original source cursor is retained, earlier private steps visible. Errors/noops preserve branch and lease. No live mutation or DRC certification. Generated IDs retained for merge.",
        ),
        (
            "inspect_pcb_branch",
            Operation::Inspect,
            input_schema::<PcbBranchId>(),
            "Inspect bounded candidate board and metadata owned by this trusted transport connection. Same client display names do not share ownership. No mutation/lease renewal. Read-only agent mode exposes list/diff only.",
        ),
        (
            "list_pcb_branches",
            Operation::List,
            json!({"type":"object","properties":{},"additionalProperties":false}),
            "Read at most4 bounded branch metadata records including creator, original project/full cursor, revision, changed count, busy/stale and remaining lease. Does not renew leases or return private candidate geometry.",
        ),
        (
            "get_pcb_branch_diff",
            Operation::Diff,
            input_schema::<PcbBranchDiffInput>(),
            "Read bounded deterministic kind/UUID ordered coalesced before/after Part/Track/Via changes. offset0, limit1..128 default32. Reports original source/revision, total/next_offset, stale/busy. At most2MiB serialized deltas plus metadata. No lease renewal or DRC check.",
        ),
        (
            "merge_pcb_branch",
            Operation::Merge,
            input_schema::<MergePcbBranch>(),
            "Explicitly merge own branch using original project/full source cursor and exact branch revision. Refuses busy/stale/reopened or any before-object mismatch atomically; no rebase. Commits retained generated IDs in one normal undoable transaction attributed to creator, normal automatic DRC scheduling. Successful merge removes branch. This is voluntary preparation, not enforced approval security.",
        ),
        (
            "discard_pcb_branch",
            Operation::Discard,
            input_schema::<PcbBranchRevision>(),
            "Discard only this transport connection's branch at exact revision, including stale branches. Busy workers refuse. No document/history/selection/native mutation. Human desktop may inspect/merge/discard any branch.",
        ),
    ];
    for (name, operation, schema, description) in definitions {
        let engine = engine.clone();
        registry.register_with_context(name, description, schema, move |input, ctx| run(engine.clone(), operation, input, ctx))?;
        if matches!(operation, Operation::List | Operation::Diff) {
            registry.admit_observation(name);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    #[tokio::test]
    async fn oversized_or_control_creator_refuses_instead_of_losing_original_undo_attribution() {
        for name in ["x".repeat(129), "name\n#1".into()] {
            let engine = EngineHandle::new(viamind_doc::Project::new("identity"));
            let before = engine.session_state();
            let input = json!({"expected_project":before.project.id,"expected_cursor":before.cursor,"label":"guard"});
            let mut ctx = CallContext::new(name);
            ctx.connection = Some(1);
            assert!(run(engine.clone(), Operation::Create, input, ctx).await.unwrap_err().to_string().contains("creator identity"));
            assert!(engine.list_pcb_branches().is_empty());
            assert_eq!(engine.session_state(), before);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn canceling_real_create_awaiter_does_not_release_its_blocked_worker_permit() {
        let engine = EngineHandle::new(viamind_doc::Project::new("retained worker"));
        let input = json!({"expected_project":engine.read(|p|p.id),"expected_cursor":engine.event_cursor(),"label":"canceled awaiter"});
        let (locked_tx, locked_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let locked_engine = engine.clone();
        let locker = std::thread::spawn(move || {
            locked_engine.with_engine(|_| {
                locked_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            });
        });
        locked_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut ctx = CallContext::new("owned#1");
        ctx.connection = Some(1);
        let task = tokio::spawn(run(engine.clone(), Operation::Create, input, ctx));
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match reserve_pcb_branch_work() {
                    Ok(permit) => drop(permit),
                    Err(_) => break,
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(reserve_pcb_branch_work().is_err(), "actual blocked create owns permit after cancellation");
        release_tx.send(()).unwrap();
        locker.join().unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(permit) = reserve_pcb_branch_work() {
                    drop(permit);
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(engine.list_pcb_branches().len(), 1);
        assert!(engine.read(|p| p.board.parts.is_empty()));
    }
}
