use std::process::Command;
use std::path::Path;

pub fn find_npx_path() -> Option<String> {
    // Try common npx locations
    let possible_paths = vec![
        "/usr/local/bin/npx",
        "/usr/bin/npx",
        "/opt/homebrew/bin/npx",
    ];
    
    // Check each path
    for path in possible_paths {
        if Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    
    // Try using 'which' command
    if let Ok(output) = Command::new("which").arg("npx").output() {
        if output.status.success() {
            if let Ok(path) = String::from_utf8(output.stdout) {
                let path = path.trim();
                if !path.is_empty() {
                    return Some(path.to_string());
                }
            }
        }
    }
    
    // Check in user's home directory for nvm installations
    if let Ok(home) = std::env::var("HOME") {
        let nvm_paths = vec![
            format!("{}/.nvm/versions/node/*/bin/npx", home),
            format!("{}/n/*/bin/npx", home),
        ];
        
        for pattern in nvm_paths {
            if let Ok(entries) = glob::glob(&pattern) {
                for path in entries.flatten() {
                    if path.exists() {
                        return Some(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    
    None
}