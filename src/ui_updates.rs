use crossbeam_queue::SegQueue;
use makepad_widgets::SignalToUI;
use crate::monitor::MonitorData;

/// The possible updates that should be displayed by the monitor UI.
/// These updates are enqueued by background tasks and dequeued by the UI.
#[derive(Debug, Clone)]
pub enum MonitorUpdate {
    /// Update with new monitor data
    DataUpdate(Box<MonitorData>),
    /// Status message to display
    StatusMessage(String),
    /// Error message to display
    ErrorMessage(String),
}

/// Global queue for pending monitor updates
static PENDING_MONITOR_UPDATES: SegQueue<MonitorUpdate> = SegQueue::new();

/// Enqueue a new monitor update and signal the UI that an update is available.
pub fn enqueue_monitor_update(update: MonitorUpdate) {
    PENDING_MONITOR_UPDATES.push(update);
    SignalToUI::set_ui_signal();
}

/// Dequeue all pending monitor updates.
/// This should be called from the UI thread.
pub fn dequeue_monitor_updates() -> Vec<MonitorUpdate> {
    let mut updates = Vec::new();
    while let Some(update) = PENDING_MONITOR_UPDATES.pop() {
        updates.push(update);
    }
    updates
}