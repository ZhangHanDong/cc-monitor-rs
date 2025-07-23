#![allow(dead_code)]

use tray_icon::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};

// Platform-specific requirements:
// - Windows: Requires win32 event loop
// - macOS: Must be created on main thread
// - Linux: Requires GTK event loop, libappindicator
use std::{cell::RefCell, path::PathBuf, rc::Rc};
use anyhow::Result;
use crate::monitor::MonitorData;
use crate::i18n;

pub struct TrayManager {
    tray: TrayIcon,
    menu: Menu,
    status_item: MenuItem,
    network_item: MenuItem,
    usage_item: MenuItem,
    cost_item: MenuItem,
    model_item: MenuItem,
    remaining_item: MenuItem,
    status_active_item: MenuItem,
    quit_item: MenuItem,
}

impl TrayManager {
    pub fn new() -> Result<Self> {
        // Create menu items
        let status_item = MenuItem::new(&i18n::get(i18n::keys::APP_VERSION), true, None);
        let separator1 = PredefinedMenuItem::separator();

        let network_item = MenuItem::new(
            &format!("{}: {}", i18n::get(i18n::keys::TRAY_NETWORK), i18n::get(i18n::keys::TRAY_CHECKING)),
            false,
            None
        );
        let usage_item = MenuItem::new(
            &format!("{}: {}", i18n::get(i18n::keys::TRAY_USAGE), i18n::get(i18n::keys::TRAY_CHECKING)),
            false,
            None
        );
        let cost_item = MenuItem::new(
            &format!("{}: {}", i18n::get(i18n::keys::TRAY_COST), i18n::get(i18n::keys::TRAY_CHECKING)),
            false,
            None
        );
        let model_item = MenuItem::new(
            &format!("{}: {}", i18n::get(i18n::keys::TRAY_MODEL), i18n::get(i18n::keys::TRAY_CHECKING)),
            false,
            None
        );
        let remaining_item = MenuItem::new(
            &format!("{}: {}", i18n::get(i18n::keys::TRAY_REMAINING), i18n::get(i18n::keys::TRAY_CHECKING)),
            false,
            None
        );
        let status_active_item = MenuItem::new(
            &format!("{}: {}", i18n::get(i18n::keys::TRAY_STATUS), i18n::get(i18n::keys::TRAY_CHECKING)),
            false,
            None
        );

        let separator2 = PredefinedMenuItem::separator();
        let quit_item = MenuItem::new(&i18n::get(i18n::keys::TRAY_QUIT), true, None);

        // Create menu
        let menu = Menu::new();
        menu.append(&status_item)?;
        menu.append(&separator1)?;
        menu.append(&network_item)?;
        menu.append(&usage_item)?;
        menu.append(&cost_item)?;
        menu.append(&model_item)?;
        menu.append(&remaining_item)?;
        menu.append(&status_active_item)?;
        menu.append(&separator2)?;
        menu.append(&quit_item)?;

        // Create tray icon using embedded icon
        let icon = Self::create_icon()?;

        // Build tray with platform considerations
        let builder = TrayIconBuilder::new()
            .with_menu(Box::new(menu.clone()))
            .with_tooltip(&i18n::get(i18n::keys::APP_NAME))
            .with_icon(icon);

        // Linux sometimes requires a menu to show the icon
        #[cfg(target_os = "linux")]
        {
            // Menu is already set above, which helps with Linux visibility
        }

        let tray = builder.build()?;

        Ok(Self {
            tray,
            menu,
            status_item,
            network_item,
            usage_item,
            cost_item,
            model_item,
            remaining_item,
            status_active_item,
            quit_item,
        })
    }

    pub fn update_status(&self, data: &MonitorData) -> Result<()> {
        // Update network status
        let network_text = if data.network_connected {
            format!("{} {}",
                i18n::get(i18n::keys::TRAY_NETWORK_CONNECTED),
                data.latency.as_deref().unwrap_or(""))
        } else {
            i18n::get(i18n::keys::TRAY_NETWORK_DISCONNECTED)
        };
        self.network_item.set_text(network_text);

        // Update usage status
        let usage_text = format!("📊 Token: {} ({})",
            data.ccusage_data.tokens,
            data.ccusage_data.latest_session
        );
        self.usage_item.set_text(usage_text);

        // Update cost
        let cost_text = format!("💰 {}: {}", i18n::get(i18n::keys::TRAY_COST), data.ccusage_data.cost);
        self.cost_item.set_text(cost_text);

        // Update model
        let model_text = format!("🤖 {}: {}",
            i18n::get(i18n::keys::TRAY_MODEL),
            if data.ccusage_data.model.is_empty() { "--" } else { &data.ccusage_data.model }
        );
        self.model_item.set_text(model_text);

        // Update remaining time
        let remaining_text = format!("⏱️ {}: {}", i18n::get(i18n::keys::TRAY_REMAINING), data.ccusage_data.remaining_time);
        self.remaining_item.set_text(remaining_text);

        // Update active status
        let status_text = if data.ccusage_data.status == "ACTIVE" {
            i18n::get(i18n::keys::TRAY_STATUS_ACTIVE)
        } else {
            i18n::get(i18n::keys::TRAY_STATUS_COMPLETED)
        };
        self.status_active_item.set_text(status_text);

        // Update tray tooltip with current status
        let tooltip = format!(
            "{}\n{} | {} | {}",
            i18n::get(i18n::keys::APP_NAME),
            if data.network_connected { "🟢" } else { "🔴" },
            data.ccusage_data.tokens,
            data.ccusage_data.remaining_time
        );
        let _ = self.tray.set_tooltip(Some(tooltip));

        Ok(())
    }

    fn create_icon() -> Result<tray_icon::Icon> {

        fn possible_icon_paths() -> Vec<PathBuf> {
            let mut paths = vec![];

            paths.push(PathBuf::from("assets/ccm-logo.png"));

            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(app_dir) = exe_path.parent() {
                    let bundle_path = app_dir.join("../Resources/cc-monitor-rs/resources/ccm-logo.png");
                    paths.push(bundle_path);
                }
            }

            paths
        }

         for path in possible_icon_paths() {
            if path.exists() {
                let icon_data = std::fs::read(&path)?;
                let img = image::load_from_memory(&icon_data)?;
                let rgba = img.to_rgba8();
                let (width, height) = (rgba.width(), rgba.height());
                let icon_data = rgba.into_raw();

                return tray_icon::Icon::from_rgba(icon_data, width, height)
                    .map_err(|e| anyhow::anyhow!("Failed to create icon from {:?}: {}", path, e));
            }
        }

        // Fallback to programmatically generated icon if file not found
        let size = 32;
        let mut rgba = vec![0u8; size * size * 4];

        // Draw a simple "C" for Claude
        for y in 0..size {
            for x in 0..size {
                let idx = (y * size + x) * 4;

                // Background (transparent)
                rgba[idx] = 0;
                rgba[idx + 1] = 0;
                rgba[idx + 2] = 0;
                rgba[idx + 3] = 0;

                // Draw circle outline
                let cx = size as f32 / 2.0;
                let cy = size as f32 / 2.0;
                let r = (size as f32 / 2.0) - 2.0;

                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let dist = (dx * dx + dy * dy).sqrt();

                // Draw circle
                if (dist - r).abs() < 2.0 {
                    // Blue circle
                    rgba[idx] = 74;      // R
                    rgba[idx + 1] = 158;  // G
                    rgba[idx + 2] = 255;  // B
                    rgba[idx + 3] = 255;  // A
                }

                // Draw "C" inside
                if (10..=22).contains(&x) && (8..=24).contains(&y){
                    let in_c = ((10..=14).contains(&x) && (8..=24).contains(&y)) || // Left vertical
                               ((8..=12).contains(&y) && (10..=22).contains(&x)) || // Top horizontal
                               ((20..=24).contains(&y) && (10..=22).contains(&x));   // Bottom horizontal

                    if in_c {
                        rgba[idx] = 255;      // R
                        rgba[idx + 1] = 255;  // G
                        rgba[idx + 2] = 255;  // B
                        rgba[idx + 3] = 255;  // A
                    }
                }
            }
        }

        let icon = tray_icon::Icon::from_rgba(rgba, size as u32, size as u32)?;
        Ok(icon)
    }

    pub fn handle_menu_event(&self, event: &tray_icon::menu::MenuEvent) -> bool {
        // Check if quit was clicked
        event.id() == self.quit_item.id()
    }
}

// Thread-safe wrapper for the tray manager
pub struct TrayHandle {
    inner: Rc<RefCell<Option<TrayManager>>>,
}

impl TrayHandle {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(None)),
        }
    }

    pub fn init(&self) -> Result<()> {
        let tray = TrayManager::new()?;
        *self.inner.borrow_mut() = Some(tray);
        Ok(())
    }

    pub fn update_status(&self, data: &MonitorData) -> Result<()> {
        if let Some(tray) = self.inner.borrow().as_ref() {
            tray.update_status(data)?;
        }
        Ok(())
    }

    pub fn handle_menu_event(&self, event: &tray_icon::menu::MenuEvent) -> bool {
        if let Some(tray) = self.inner.borrow().as_ref() {
            tray.handle_menu_event(event)
        } else {
            false
        }
    }
}

impl Default for TrayHandle {
    fn default() -> Self {
        Self::new()
    }
}