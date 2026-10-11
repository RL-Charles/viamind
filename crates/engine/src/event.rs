//! Change notifications: one typed diff per committed transaction.

use std::fmt;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use serde::{Deserialize, Serialize};

use crate::delta::Delta;
use crate::transaction::{Origin, TxId};

/// What happened to a transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// A different document replaced this session; rebuild from the shared handle.
    Replaced,
    /// The transaction was committed.
    Applied,
    /// The transaction was undone.
    Undone,
    /// The transaction was redone after an undo.
    Redone,
}

/// One change to the project, sent to every subscriber after it happens.
///
/// The UI and the MCP server consume this same type: the UI to repaint and
/// refresh selections, agents to see what the user (or another agent) changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeEvent {
    /// The transaction this event is about (for undo/redo, the original transaction).
    pub tx: TxId,
    /// Who caused this event: the author of an applied transaction, or whoever
    /// asked for the undo/redo.
    pub origin: Origin,
    /// Applied, undone or redone.
    pub kind: ChangeKind,
    /// The change as it was just made to the project, one delta per object. For
    /// [`ChangeKind::Undone`] these are the inverted deltas of the transaction.
    pub deltas: Vec<Delta>,
}

/// A callback run after every change (e.g. egui's `request_repaint`).
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// Fan-out of change events to channel subscribers and wakers.
#[derive(Default)]
pub(crate) struct Notifier {
    subscribers: Vec<Sender<ChangeEvent>>,
    wakers: Vec<Waker>,
    wake_pending: bool,
}

impl fmt::Debug for Notifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Notifier").field("subscribers", &self.subscribers.len()).field("wakers", &self.wakers.len()).finish()
    }
}

impl Notifier {
    pub(crate) fn subscribe(&mut self) -> Receiver<ChangeEvent> {
        let (sender, receiver) = channel();
        self.subscribers.push(sender);
        receiver
    }

    pub(crate) fn on_change(&mut self, waker: Waker) {
        self.wakers.push(waker);
    }

    /// Sends `event` to every live subscriber (dropping closed ones) and marks the
    /// wakers as due.
    pub(crate) fn send(&mut self, event: ChangeEvent) {
        if let Some((last, rest)) = self.subscribers.split_last() {
            let mut closed = Vec::new();
            for (i, subscriber) in rest.iter().enumerate() {
                if subscriber.send(event.clone()).is_err() {
                    closed.push(i);
                }
            }
            if last.send(event).is_err() {
                closed.push(rest.len());
            }
            for i in closed.into_iter().rev() {
                self.subscribers.remove(i);
            }
        }
        self.wake_pending = true;
    }

    pub(crate) fn wake(&mut self) {
        self.wake_pending = true;
    }

    /// The wakers to run now if anything changed since the last call.
    pub(crate) fn take_due_wakers(&mut self) -> Vec<Waker> {
        if std::mem::take(&mut self.wake_pending) { self.wakers.clone() } else { Vec::new() }
    }

    pub(crate) fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }
}
