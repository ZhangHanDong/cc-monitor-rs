use makepad_widgets::*;

use crate::monitor::MonitorData;
use crate::ui::main_screen::MainScreen;
use crate::background::{start_monitor_tokio, submit_monitor_request, MonitorRequest};
use crate::ui_updates::{dequeue_monitor_updates, MonitorUpdate};
use crate::tray::TrayHandle;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    use crate::ui::main_screen::MainScreen;

    App = {{App}} {
        ui: <Root>{
            main_window = <Window>{
                window: {
                    title: "Claude Code Monitor",
                    position: vec2(100, 100),
                    inner_size: vec2(600, 800)
                },
                show_bg: true,
                pass: {clear_color: #1a1a1a},
                block_signal_event: true,

                body = <MainScreen> {}
            }
        }
    }
}

#[derive(Live, LiveHook)]
pub struct App {
    #[live] ui: WidgetRef,
    
    #[rust] current_data: MonitorData,
    #[rust] time_update_timer: Timer,
    #[rust] tray_handle: Option<TrayHandle>,
}


impl LiveRegister for App {
    fn live_register(cx: &mut Cx) {
        makepad_widgets::live_design(cx);
        crate::ui::styles::live_design(cx);
        crate::ui::main_screen::live_design(cx);
        crate::ui::widgets::status_indicator::live_design(cx);
        crate::ui::widgets::usage_display::live_design(cx);
        crate::ui::widgets::chart::live_design(cx);
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        log!("App startup called");
        
        // Initialize main screen
        if let Some(mut main_screen) = self.ui.widget(id!(body)).borrow_mut::<MainScreen>() {
            main_screen.initialize(cx);
        }
        
        // Initialize system tray
        let tray_handle = TrayHandle::new();
        if let Err(e) = tray_handle.init() {
            error!("Failed to initialize system tray: {}", e);
        } else {
            self.tray_handle = Some(tray_handle);
        }
        
        // Start the background monitoring tasks
        if let Err(e) = start_monitor_tokio() {
            error!("Failed to start background monitoring: {}", e);
        }
        
        // Force initial update
        if let Err(e) = submit_monitor_request(MonitorRequest::ForceUpdate) {
            error!("Failed to request initial update: {}", e);
        }
        
        // Start 1 second timer for time updates
        self.time_update_timer = cx.start_interval(1.0);
        
        log!("Background monitoring started");
    }
    
    fn handle_signal(&mut self, cx: &mut Cx) {
        // Process all pending updates from background tasks
        let updates = dequeue_monitor_updates();
        
        for update in updates {
            match update {
                MonitorUpdate::DataUpdate(data) => {
                    log!("Received data update from background task");
                    self.current_data = *data;
                    
                    // Update UI
                    if let Some(mut main_screen) = self.ui.widget(id!(body)).borrow_mut::<MainScreen>() {
                        main_screen.update_data(cx, self.current_data.clone());
                    }
                    
                    // Update tray status
                    if let Some(ref tray) = self.tray_handle {
                        if let Err(e) = tray.update_status(&self.current_data) {
                            error!("Failed to update tray status: {}", e);
                        }
                    }
                }
                MonitorUpdate::StatusMessage(msg) => {
                    log!("Status: {}", msg);
                }
                MonitorUpdate::ErrorMessage(err) => {
                    error!("Error: {}", err);
                }
            }
        }
        
        cx.redraw_all();
    }
}

impl AppMain for App {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
        
        // Handle timer events for updating time display
        if self.time_update_timer.is_event(event).is_some() {
            // Update only the last update time label
            if let Some(mut main_screen) = self.ui.widget(id!(body)).borrow_mut::<MainScreen>() {
                main_screen.update_time_display(cx);
            }
            cx.redraw_all();
        }
        
        // Handle tray menu events
        if let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            if let Some(ref tray) = self.tray_handle {
                if tray.handle_menu_event(&event) {
                    // Quit was selected
                    cx.quit();
                }
            }
        }
    }
}

app_main!(App);