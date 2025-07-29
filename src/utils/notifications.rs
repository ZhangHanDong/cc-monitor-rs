use notify_rust::Notification;
#[cfg(not(target_os = "macos"))]
use notify_rust::Urgency;
use crate::i18n;

pub fn show_notification(title: &str, message: &str) {
    #[cfg(target_os = "macos")]
    {
        // Use macOS-specific notification with sound
        if let Err(e) = Notification::new()
            .summary(title)
            .body(message)
            .sound_name("Glass")
            .show()
        {
            eprintln!("Failed to show notification: {}", e);
        }
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        // Generic notification for other platforms
        if let Err(e) = Notification::new()
            .summary(title)
            .body(message)
            .show()
        {
            eprintln!("Failed to show notification: {}", e);
        }
    }
}

pub fn show_critical_notification(title: &str, message: &str) {
    #[cfg(target_os = "macos")]
    {
        // Use macOS-specific notification with alert sound
        if let Err(e) = Notification::new()
            .summary(title)
            .body(message)
            .sound_name("Basso")  // Alert sound
            .show()
        {
            eprintln!("Failed to show notification: {}", e);
        }
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        // Generic notification for other platforms
        if let Err(e) = Notification::new()
            .summary(title)
            .body(message)
            .show()
        {
            eprintln!("Failed to show notification: {}", e);
        }
    }
}

pub fn show_usage_notification(tokens: i64, cost: f64, remaining_time: &str) {
    let message = format!(
        "Token {}: {}\n{}: ${:.2}\n{}: {}",
        i18n::get(i18n::keys::TRAY_USAGE),
        format_number(tokens),
        i18n::get(i18n::keys::TRAY_COST),
        cost,
        i18n::get(i18n::keys::TRAY_REMAINING),
        remaining_time
    );
    
    show_notification(&i18n::get(i18n::keys::NOTIF_USAGE_STATUS), &message);
}

fn format_number(num: i64) -> String {
    if num >= 1_000_000 {
        format!("{:.1}M", num as f64 / 1_000_000.0)
    } else if num >= 1_000 {
        format!("{:.1}K", num as f64 / 1_000.0)
    } else {
        num.to_string()
    }
}