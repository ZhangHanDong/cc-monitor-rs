use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::runtime::Runtime;
use makepad_widgets::log;
use anyhow::Result;

use crate::monitor::{NetworkMonitor, CcusageMonitor, MonitorData, CcusageData};
use crate::ui_updates::{enqueue_monitor_update, MonitorUpdate};
use crate::utils::notifications::{show_notification, show_usage_notification};
use crate::i18n;

/// The single global Tokio runtime that is used by all async tasks.
static TOKIO_RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// The sender used to send requests to the async worker thread.
static REQUEST_SENDER: OnceLock<UnboundedSender<MonitorRequest>> = OnceLock::new();

/// Requests that can be sent to the background monitor worker.
#[derive(Debug, Clone)]
pub enum MonitorRequest {
    /// Start monitoring with the given interval in seconds
    StartMonitoring { interval_secs: u64 },
    /// Stop monitoring
    StopMonitoring,
    /// Force an immediate update
    ForceUpdate,
}

/// Start the background Tokio runtime and monitoring tasks.
pub fn start_monitor_tokio() -> Result<()> {
    // Create a Tokio runtime, and save it in a static variable to ensure it isn't dropped.
    let rt = TOKIO_RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().unwrap());

    // Create a channel to be used between UI thread(s) and the async worker thread.
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<MonitorRequest>();
    REQUEST_SENDER.set(sender).expect("BUG: REQUEST_SENDER already set!");

    // Start the async worker task directly on the runtime
    let _worker_handle = rt.spawn(async_worker(receiver));

    // Send initial start monitoring request (1 second for more real-time updates)
    submit_monitor_request(MonitorRequest::StartMonitoring { interval_secs: 1 })?;

    Ok(())
}

/// Submit a request to the background monitor worker.
pub fn submit_monitor_request(request: MonitorRequest) -> Result<()> {
    if let Some(sender) = REQUEST_SENDER.get() {
        sender.send(request)?;
    } else {
        anyhow::bail!("Monitor worker not initialized");
    }
    Ok(())
}

/// The main async worker that handles monitoring tasks.
async fn async_worker(mut receiver: UnboundedReceiver<MonitorRequest>) -> Result<()> {
    log!("Monitor async worker started");
    
    let network_monitor = NetworkMonitor::new();
    let ccusage_monitor = Arc::new(Mutex::new(CcusageMonitor::new()));
    let mut monitoring_active = false;
    let mut interval = tokio::time::interval(Duration::from_secs(3));
    let mut last_network_status: Option<bool> = None;
    let mut last_notification_time = std::time::Instant::now();
    let notification_interval = Duration::from_secs(3600); // 1 hour

    loop {
        tokio::select! {
            // Handle incoming requests
            Some(request) = receiver.recv() => {
                match request {
                    MonitorRequest::StartMonitoring { interval_secs } => {
                        log!("Starting monitoring with interval: {} seconds", interval_secs);
                        monitoring_active = true;
                        interval = tokio::time::interval(Duration::from_secs(interval_secs));
                    }
                    MonitorRequest::StopMonitoring => {
                        log!("Stopping monitoring");
                        monitoring_active = false;
                    }
                    MonitorRequest::ForceUpdate => {
                        log!("Forcing immediate update");
                        perform_monitor_update(&network_monitor, &ccusage_monitor, &mut last_network_status, &mut last_notification_time, notification_interval).await;
                    }
                }
            }
            
            // Periodic monitoring tick
            _ = interval.tick(), if monitoring_active => {
                perform_monitor_update(&network_monitor, &ccusage_monitor, &mut last_network_status, &mut last_notification_time, notification_interval).await;
            }
            
            // If channel is closed, exit
            else => {
                log!("Monitor worker channel closed, exiting");
                break;
            }
        }
    }

    Ok(())
}

/// Perform a single monitoring update.
async fn perform_monitor_update(
    network_monitor: &NetworkMonitor,
    ccusage_monitor: &Arc<Mutex<CcusageMonitor>>,
    last_network_status: &mut Option<bool>,
    last_notification_time: &mut std::time::Instant,
    notification_interval: Duration,
) {
    log!("Performing monitor update");

    // Run network monitoring in blocking task
    let (ping_success, latency) = tokio::task::spawn_blocking({
        let monitor = network_monitor.clone();
        move || monitor.ping_google()
    }).await.unwrap_or((false, None));

    let (conn_success, speed, _) = tokio::task::spawn_blocking({
        let monitor = network_monitor.clone();
        move || monitor.check_connection()
    }).await.unwrap_or((false, None, None));

    log!("Network status - ping: {}, conn: {}, latency: {:?}, speed: {:?}", 
         ping_success, conn_success, latency, speed);

    // Run ccusage monitoring in blocking task
    let ccusage_data = tokio::task::spawn_blocking({
        let monitor = ccusage_monitor.clone();
        move || {
            let mut monitor = monitor.lock().unwrap();
            monitor.get_ccusage_info()
        }
    }).await.unwrap_or_else(|_| CcusageData::default());

    log!("Ccusage data: session={}, tokens={}, cost={}", 
         ccusage_data.latest_session, ccusage_data.tokens, ccusage_data.cost);

    let daily_costs = tokio::task::spawn_blocking({
        let monitor = ccusage_monitor.clone();
        move || {
            let monitor = monitor.lock().unwrap();
            monitor.analyze_daily_costs()
        }
    }).await.unwrap_or_default();

    // Check for network status changes
    let network_connected = ping_success || conn_success;
    if let Some(last_status) = last_network_status {
        if !*last_status && network_connected {
            // Network restored notification
            tokio::task::spawn_blocking(|| {
                show_notification("Claude Code 监测器", "🎉 网络连接已恢复");
            });
        } else if *last_status && !network_connected {
            // Network lost notification
            tokio::task::spawn_blocking(|| {
                show_notification("Claude Code 监测器", "🚨 网络连接中断");
            });
        }
    }
    *last_network_status = Some(network_connected);

    // Create monitor data
    let monitor_data = MonitorData {
        network_connected,
        latency,
        connection_speed: speed,
        ccusage_data: ccusage_data.clone(),
        daily_costs,
        last_update: chrono::Local::now(),
    };

    // Check if we should send periodic notification
    let now = std::time::Instant::now();
    if now.duration_since(*last_notification_time) >= notification_interval {
        *last_notification_time = now;
        
        // Calculate remaining time
        let remaining_time = if let Some(reset_time) = ccusage_data.reset_time {
            let now = chrono::Local::now();
            if reset_time > now {
                let duration = reset_time - now;
                let hours = duration.num_hours();
                let minutes = duration.num_minutes() % 60;
                format!("{} {} {} {}", 
                    hours, i18n::get(i18n::keys::COMMON_HOUR),
                    minutes, i18n::get(i18n::keys::COMMON_MINUTE))
            } else {
                i18n::get(i18n::keys::USAGE_EXPIRED)
            }
        } else {
            i18n::get(i18n::keys::COMMON_UNKNOWN)
        };
        
        // Send usage notification
        tokio::task::spawn_blocking({
            let tokens = ccusage_data.tokens_num;
            let cost = ccusage_data.cost_num;
            let remaining = remaining_time.clone();
            move || {
                show_usage_notification(tokens, cost, &remaining);
            }
        });
    }
    
    // Enqueue update for UI
    enqueue_monitor_update(MonitorUpdate::DataUpdate(Box::new(monitor_data)));
}