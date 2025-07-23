use std::process::Command;
use std::time::{Duration, Instant};
use std::net::{TcpStream, ToSocketAddrs};
use std::env;
use crate::i18n;

#[derive(Clone)]
pub struct NetworkMonitor;

impl Default for NetworkMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkMonitor {
    pub fn new() -> Self {
        Self
    }
    
    // Get system proxy settings on macOS
    #[cfg(target_os = "macos")]
    fn get_system_proxy() -> Option<String> {
        // Try to get proxy from networksetup command
        let output = Command::new("networksetup")
            .args(["-getwebproxy", "Wi-Fi"])
            .output()
            .ok()?;
            
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut enabled = false;
            let mut server = String::new();
            let mut port = String::new();
            
            for line in stdout.lines() {
                if line.starts_with("Enabled: Yes") {
                    enabled = true;
                } else if line.starts_with("Server: ") {
                    server = line.strip_prefix("Server: ")?.trim().to_string();
                } else if line.starts_with("Port: ") {
                    port = line.strip_prefix("Port: ")?.trim().to_string();
                }
            }
            
            if enabled && !server.is_empty() && !port.is_empty() {
                return Some(format!("{}:{}", server, port));
            }
        }
        
        None
    }
    
    #[cfg(not(target_os = "macos"))]
    fn get_system_proxy() -> Option<String> {
        None
    }
    
    pub fn ping_google(&self) -> (bool, Option<String>) {
        // Use IP address instead of domain name to avoid DNS issues
        let output = if cfg!(target_os = "windows") {
            Command::new("ping")
                .args(["-n", "1", "8.8.8.8"])
                .output()
        } else {
            Command::new("ping")
                .args(["-c", "1", "-W", "2000", "8.8.8.8"])  // -W 2000 for 2s timeout
                .output()
        };
        
        match output {
            Ok(output) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    
                    // Parse latency from ping output
                    let latency = if cfg!(target_os = "windows") {
                        // Windows: "Average = XXms"
                        if let Some(line) = stdout.lines().find(|l| l.contains("Average")) {
                            if let Some(avg) = line.split('=').nth(1) {
                                Some(avg.trim().to_string())
                            } else {
                                Some("< 5ms".to_string())
                            }
                        } else {
                            Some("< 5ms".to_string())
                        }
                    } else {
                        // macOS/Linux: "round-trip min/avg/max/stddev = X.X/X.X/X.X/X.X ms"
                        if let Some(line) = stdout.lines().find(|l| l.contains("min/avg/max")) {
                            if let Some(times) = line.split('=').nth(1) {
                                if let Some(avg) = times.trim().split('/').nth(1) {
                                    // Round to integer
                                    if let Ok(avg_f) = avg.parse::<f64>() {
                                        Some(format!("{}ms", avg_f.round() as i64))
                                    } else {
                                        Some(format!("{}ms", avg))
                                    }
                                } else {
                                    Some("< 5ms".to_string())
                                }
                            } else {
                                Some("< 5ms".to_string())
                            }
                        } else if let Some(line) = stdout.lines().find(|l| l.contains("time=")) {
                            // Alternative format: "time=XX.XXX ms"
                            if let Some(time_part) = line.split("time=").nth(1) {
                                if let Some(time) = time_part.split_whitespace().next() {
                                    if let Ok(time_f) = time.parse::<f64>() {
                                        Some(format!("{}ms", time_f.round() as i64))
                                    } else {
                                        Some(format!("{}ms", time))
                                    }
                                } else {
                                    Some("< 5ms".to_string())
                                }
                            } else {
                                Some("< 5ms".to_string())
                            }
                        } else {
                            Some("< 5ms".to_string())
                        }
                    };
                    
                    (true, latency)
                } else {
                    (false, None)
                }
            }
            Err(_) => (false, None),
        }
    }
    
    pub fn check_connection(&self) -> (bool, Option<String>, Option<f64>) {
        let start = Instant::now();
        
        // Check if we need to use proxy
        let proxy_host = env::var("HTTP_PROXY")
            .or_else(|_| env::var("http_proxy"))
            .or_else(|_| env::var("HTTPS_PROXY"))
            .or_else(|_| env::var("https_proxy"))
            .ok()
            .or_else(Self::get_system_proxy);
            
        // If proxy is set, test connection to proxy instead
        let test_addr = if let Some(proxy) = proxy_host {
            // Extract host:port from proxy URL (e.g., http://127.0.0.1:7890)
            let addr = if let Some(addr) = proxy.strip_prefix("http://").or_else(|| proxy.strip_prefix("https://")) {
                addr.to_string()
            } else {
                proxy.clone()
            };
            
            // Log that we're using proxy
            eprintln!("Using proxy: {}", addr);
            addr
        } else {
            // Use a reliable IP address instead of domain to avoid DNS issues  
            "1.1.1.1:443".to_string()  // Cloudflare DNS
        };
        
        let addrs = test_addr.to_socket_addrs();
        
        match addrs {
            Ok(mut addrs_iter) => {
                if let Some(addr) = addrs_iter.next() {
                    match TcpStream::connect_timeout(&addr, Duration::from_secs(5)) {
                        Ok(_) => {
                            let elapsed = start.elapsed().as_millis() as f64;
                            let speed = if elapsed < 100.0 {
                                i18n::get(i18n::keys::NETWORK_EXCELLENT)
                            } else if elapsed < 200.0 {
                                i18n::get(i18n::keys::NETWORK_GOOD)
                            } else if elapsed < 500.0 {
                                i18n::get(i18n::keys::NETWORK_FAIR)
                            } else {
                                i18n::get(i18n::keys::NETWORK_SLOW)
                            };
                            
                            (true, Some(speed.to_string()), Some(elapsed))
                        }
                        Err(_) => (false, None, None),
                    }
                } else {
                    (false, None, None)
                }
            }
            Err(_) => (false, None, None),
        }
    }
}