//! Admission proofs exercise real handler workers and independent runtime shutdown.
use crate::{CallContext, ToolRegistry};
use serde_json::json;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
use std::time::Duration;
use viamind_engine::agent_policy::{AgentMode, AgentPolicyError};

#[test]
fn canceled_caller_and_destroyed_transport_runtime_keep_actual_publication_admitted() {
    let mut registry = ToolRegistry::new();
    let policy = registry.agent_policy();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    let published = Arc::new(AtomicBool::new(false));
    let handler_published = Arc::clone(&published);
    registry
        .register_with_context("publish", "held actual writer", json!({"type":"object"}), move |_, ctx| {
            assert_eq!(ctx.agent.as_str(), "original creator");
            let entered_tx = entered_tx.clone();
            let release_rx = Arc::clone(&release_rx);
            let published = Arc::clone(&handler_published);
            async move {
                tokio::task::spawn_blocking(move || {
                    entered_tx.send(()).unwrap();
                    release_rx.lock().unwrap().recv().unwrap();
                    published.store(true, Ordering::SeqCst);
                })
                .await
                .unwrap();
                Ok(json!({"published":true}))
            }
        })
        .unwrap();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let transport = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async move {
            let task = tokio::spawn(async move { registry.call("publish", json!({}), CallContext::new("original creator")).await });
            stop_rx.await.unwrap();
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
        });
        drop(runtime);
        shutdown_tx.send(()).unwrap();
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(policy.set_mode(AgentMode::ReadOnly), Err(AgentPolicyError::Busy(1)));
    stop_tx.send(()).unwrap();
    shutdown_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    transport.join().unwrap();
    assert!(!published.load(Ordering::SeqCst));
    assert_eq!(policy.set_mode(AgentMode::Approval), Err(AgentPolicyError::Busy(1)));
    release_tx.send(()).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while policy.status().active_writers != 0 {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(published.load(Ordering::SeqCst));
    assert_eq!(policy.set_mode(AgentMode::ReadOnly).unwrap().mode, AgentMode::ReadOnly);
}

#[tokio::test]
async fn every_mixed_view_path_presence_is_retained_writer_and_restricted_prehandler_refusal() {
    for name in ["render_board_view", "render_board_png", "render_schematic_view", "render_schematic_png"] {
        let mut registry = ToolRegistry::new();
        let policy = registry.agent_policy();
        let calls = Arc::new(AtomicUsize::new(0));
        let handler_calls = Arc::clone(&calls);
        let handler_policy = Arc::clone(&policy);
        registry
            .register_in_memory_view(name, "mixed", json!({"type":"object","properties":{"path":{"type":"string"}}}), false, move |input| {
                let calls = Arc::clone(&handler_calls);
                let policy = Arc::clone(&handler_policy);
                async move {
                    let has_path = input.get("path").is_some();
                    assert_eq!(policy.status().active_writers, usize::from(has_path));
                    if has_path {
                        assert_eq!(policy.set_mode(AgentMode::ReadOnly), Err(AgentPolicyError::Busy(1)));
                    }
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(json!({}))
                }
            })
            .unwrap();
        for input in [json!({}), json!({"path":"new.svg"}), json!({"path":null}), json!({"path":42})] {
            registry.call(name, input, CallContext::default()).await.unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        for mode in [AgentMode::Approval, AgentMode::ReadOnly] {
            policy.set_mode(mode).unwrap();
            assert!(registry.get(name).unwrap().input_schema["properties"].get("path").is_none());
            for input in [json!({"path":"new.svg"}), json!({"path":null}), json!({"path":42})] {
                assert!(registry.call(name, input, CallContext::default()).await.is_err());
            }
            registry.call(name, json!({}), CallContext::default()).await.unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 6);
    }
}

#[tokio::test]
async fn approval_clones_and_future_writers_default_deny_and_panics_retire_tokens() {
    let mut registry = ToolRegistry::new();
    let policy = registry.agent_policy();
    registry.register("writer", "default deny", json!({}), |_| async { panic!("intentional handler failure") }).unwrap();
    registry.register("candidate", "trusted private", json!({}), |_| async { Ok(json!({})) }).unwrap();
    registry.admit_private_candidate("candidate");
    registry.register_observation("observe", "trusted observation", json!({}), |_| async { Ok(json!({})) }).unwrap();
    assert!(registry.call("writer", json!({}), CallContext::default()).await.is_err());
    assert_eq!(policy.status().active_writers, 0);
    policy.set_mode(AgentMode::Approval).unwrap();
    let mut clone = registry.clone();
    clone.register("later", "future writer", json!({}), |_| async { panic!("must never run") }).unwrap();
    for name in ["writer", "later"] {
        assert!(clone.get(name).is_none());
        assert!(clone.call(name, json!({"mode":"live","human":true}), CallContext::new("human")).await.is_err());
    }
    clone.call("candidate", json!({}), CallContext::default()).await.unwrap();
    clone.call("observe", json!({}), CallContext::default()).await.unwrap();
    let restricted = clone.read_only();
    assert!(restricted.get("candidate").is_none());
    policy.set_mode(AgentMode::Live).unwrap();
    assert!(restricted.get("writer").is_none());
    assert!(restricted.get("candidate").is_none());
}
