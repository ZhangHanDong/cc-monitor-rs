#![allow(dead_code)]

use std::process::Command;
use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use regex::Regex;
use std::collections::HashMap;
use serde_json::Value;
use crate::monitor::stats::DailyCost;
use crate::utils::process::find_npx_path;

#[derive(Clone, Debug, Default)]
pub struct CcusageData {
    pub latest_session: String,
    pub session_start: String,
    pub session_end: String,
    pub remaining_time: String,
    pub tokens: String,
    pub cost: String,
    pub status: String,
    pub model: String,
    // Numeric versions for calculations
    pub tokens_num: i64,
    pub cost_num: f64,
    pub reset_time: Option<chrono::DateTime<chrono::Local>>,
}

#[derive(Clone)]
pub struct CcusageMonitor {
    npx_path: Option<String>,
    failed_count: u32,
    max_failures: u32,
}

impl Default for CcusageMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl CcusageMonitor {
    pub fn new() -> Self {
        Self {
            npx_path: find_npx_path(),
            failed_count: 0,
            max_failures: 3,
        }
    }
    
    pub fn get_ccusage_info(&mut self) -> CcusageData {
        let npx_path = match &self.npx_path {
            Some(path) => path.clone(),
            None => {
                self.npx_path = find_npx_path();
                match &self.npx_path {
                    Some(path) => path.clone(),
                    None => {
                        return CcusageData::default();
                    }
                }
            }
        };
        
        // Use default (cached) mode for faster updates, only use calculate mode periodically
        let use_calculate_mode = self.failed_count == 0 && chrono::Local::now().timestamp() % 60 == 0; // Every minute
        
        let args = if use_calculate_mode {
            vec!["--yes", "ccusage@latest", "blocks", "--mode", "calculate", "--json"]
        } else {
            vec!["--yes", "ccusage@latest", "blocks", "--json"]
        };
        
        let start = std::time::Instant::now();
        let output = Command::new(&npx_path)
            .args(&args)
            .output();
        let elapsed = start.elapsed();
        
        if elapsed.as_millis() > 500 {
            eprintln!("ccusage command took {}ms (mode: {})", 
                elapsed.as_millis(), 
                if use_calculate_mode { "calculate" } else { "cached" }
            );
        }
            
        match output {
            Ok(output) if output.status.success() => {
                self.failed_count = 0;
                let stdout = String::from_utf8_lossy(&output.stdout);
                self.parse_ccusage_json(&stdout)
            }
            Ok(_output) => {
                self.failed_count += 1;
                if self.failed_count >= self.max_failures {
                    self.update_ccusage();
                    self.failed_count = 0;
                }
                CcusageData::default()
            }
            Err(_) => {
                self.failed_count += 1;
                if self.failed_count >= self.max_failures {
                    self.update_ccusage();
                    self.failed_count = 0;
                }
                CcusageData::default()
            }
        }
    }
    
    fn parse_ccusage_json(&self, output: &str) -> CcusageData {
        match serde_json::from_str::<Value>(output) {
            Ok(json) => {
                if let Some(blocks) = json["blocks"].as_array() {
                    // Find active block
                    for block in blocks.iter().rev() {
                        if block["isActive"].as_bool() == Some(true) {
                            return self.parse_active_block(block);
                        }
                    }
                    
                    // If no active block, get the most recent one
                    if let Some(last_block) = blocks.last() {
                        return self.parse_completed_block(last_block);
                    }
                }
            }
            Err(e) => {
                println!("Failed to parse JSON: {}", e);
            }
        }
        
        CcusageData::default()
    }
    
    fn parse_active_block(&self, block: &Value) -> CcusageData {
        let start_time_str = block["startTime"].as_str().unwrap_or("");
        let start_time = DateTime::parse_from_rfc3339(start_time_str)
            .ok()
            .map(|dt| dt.with_timezone(&Local));
            
        let (session_start, session_end, remaining_time) = if let Some(start) = start_time {
            let end = start + chrono::Duration::hours(5);
            let now = Local::now();
            
            let remaining = if let Some(remaining_mins) = block["projection"]["remainingMinutes"].as_u64() {
                let hours = remaining_mins / 60;
                let minutes = remaining_mins % 60;
                format!("{}时{}分", hours, minutes)
            } else if end > now {
                let diff = end - now;
                let hours = diff.num_hours();
                let minutes = (diff.num_minutes() % 60) as u32;
                format!("{}时{}分", hours, minutes)
            } else {
                "已过期".to_string()
            };
            
            (
                start.format("%H:%M:%S").to_string(),
                end.format("%H:%M:%S").to_string(),
                remaining,
            )
        } else {
            ("--".to_string(), "--".to_string(), "--".to_string())
        };
        
        let total_tokens = block["totalTokens"].as_u64().unwrap_or(0);
        let cost = block["costUSD"].as_f64().unwrap_or(0.0);
        
        let models = if let Some(models_array) = block["models"].as_array() {
            models_array.iter()
                .filter_map(|m| m.as_str())
                .map(|m| {
                    if m.contains("opus") { "opus-4" }
                    else if m.contains("sonnet") { "sonnet-4" }
                    else if m.contains("haiku") { "haiku-3" }
                    else { m }
                })
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            "--".to_string()
        };
        
        let reset_time = start_time.map(|st| st + chrono::Duration::hours(5));
        
        CcusageData {
            latest_session: start_time.map(|dt| dt.format("%m/%d/%Y, %I:%M:%S %p").to_string())
                .unwrap_or_else(|| "--".to_string()),
            session_start,
            session_end,
            remaining_time,
            tokens: total_tokens.to_string(),
            cost: format!("${:.2}", cost),
            status: "ACTIVE".to_string(),
            model: models,
            tokens_num: total_tokens as i64,
            cost_num: cost,
            reset_time,
        }
    }
    
    fn parse_completed_block(&self, block: &Value) -> CcusageData {
        let start_time_str = block["startTime"].as_str().unwrap_or("");
        let start_time = DateTime::parse_from_rfc3339(start_time_str)
            .ok()
            .map(|dt| dt.with_timezone(&Local));
            
        let (session_start, session_end) = if let Some(start) = start_time {
            let end = if let Some(end_str) = block["actualEndTime"].as_str() {
                DateTime::parse_from_rfc3339(end_str)
                    .ok()
                    .map(|dt| dt.with_timezone(&Local))
                    .unwrap_or(start + chrono::Duration::hours(5))
            } else {
                start + chrono::Duration::hours(5)
            };
            
            (
                start.format("%H:%M:%S").to_string(),
                end.format("%H:%M:%S").to_string(),
            )
        } else {
            ("--".to_string(), "--".to_string())
        };
        
        let total_tokens = block["totalTokens"].as_u64().unwrap_or(0);
        let cost = block["costUSD"].as_f64().unwrap_or(0.0);
        
        let models = if let Some(models_array) = block["models"].as_array() {
            models_array.iter()
                .filter_map(|m| m.as_str())
                .map(|m| {
                    if m.contains("opus") { "opus-4" }
                    else if m.contains("sonnet") { "sonnet-4" }
                    else if m.contains("haiku") { "haiku-3" }
                    else { m }
                })
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            "--".to_string()
        };
        
        CcusageData {
            latest_session: start_time.map(|dt| dt.format("%m/%d/%Y, %I:%M:%S %p").to_string())
                .unwrap_or_else(|| "--".to_string()),
            session_start,
            session_end,
            remaining_time: "已完成".to_string(),
            tokens: total_tokens.to_string(),
            cost: format!("${:.2}", cost),
            status: "COMPLETED".to_string(),
            model: models,
            tokens_num: total_tokens as i64,
            cost_num: cost,
            reset_time: None,
        }
    }
    
    fn parse_ccusage_output(&self, output: &str) -> CcusageData {
        let lines: Vec<&str> = output.lines().collect();
        
        // Look for ACTIVE session first
        for (i, line) in lines.iter().enumerate() {
            if line.contains("ACTIVE") || (line.contains("elapsed") && line.contains("remaining")) {
                if let Some(data) = self.parse_session_line(line, &lines, i, true) {
                    return data;
                }
            }
        }
        
        // If no active session, find the most recent completed one
        for line in lines.iter().rev() {
            if line.contains('│') && !line.contains("gap") && !line.contains("ACTIVE") 
                && !line.contains("PROJECTED") && !line.contains("Block Start") {
                if let Some(data) = self.parse_session_line(line, &lines, 0, false) {
                    return data;
                }
            }
        }
        
        CcusageData::default()
    }
    
    fn parse_session_line(&self, line: &str, lines: &[&str], line_idx: usize, is_active: bool) -> Option<CcusageData> {
        let parts: Vec<&str> = line.split('│').collect();
        if parts.len() < 6 {
            return None;
        }
        
        let session_info = self.clean_ansi_codes(parts[1]);
        let tokens = self.clean_ansi_codes(parts[4]);
        let cost = self.clean_ansi_codes(parts[5]);
        
        // Extract model information
        let mut models = Vec::new();
        if parts.len() > 3 {
            let model_info = self.clean_ansi_codes(parts[3]);
            if model_info.contains("opus-4") {
                models.push("opus-4");
            } else if model_info.contains("sonnet-4") {
                models.push("sonnet-4");
            } else if model_info.contains("haiku-3") {
                models.push("haiku-3");
            }
        }
        
        // Check next lines for additional models (for active sessions)
        if is_active {
            for j in (line_idx + 1)..(line_idx + 3).min(lines.len()) {
                if let Some(next_line) = lines.get(j) {
                    if next_line.contains('│') && !next_line.contains("PROJECTED") {
                        let next_parts: Vec<&str> = next_line.split('│').collect();
                        if next_parts.len() > 3 {
                            let next_model_info = self.clean_ansi_codes(next_parts[3]);
                            if next_model_info.contains("opus-4") && !models.contains(&"opus-4") {
                                models.push("opus-4");
                            } else if next_model_info.contains("sonnet-4") && !models.contains(&"sonnet-4") {
                                models.push("sonnet-4");
                            } else if next_model_info.contains("haiku-3") && !models.contains(&"haiku-3") {
                                models.push("haiku-3");
                            }
                        }
                    }
                }
            }
        }
        
        let model = if models.is_empty() {
            "--".to_string()
        } else {
            models.join(", ")
        };
        
        // Parse session time
        let session_regex = Regex::new(r"(\d+/\d+/\d{4},\s+\d+:\d+:\d+\s+[AP]M)|(\d{4}/\d+/\d+\s+\d+:\d+:\d+)").ok()?;
        let session_match = session_regex.find(&session_info)?;
        let session_start_str = session_match.as_str();
        
        let times = self.calculate_session_times(session_start_str);
        
        // For active sessions, extract cost from the correct column
        let actual_cost = if is_active && parts.len() > 6 {
            let cost_col = self.clean_ansi_codes(parts[6]);
            if cost_col.starts_with('$') {
                cost_col
            } else {
                cost.clone()
            }
        } else {
            cost.clone()
        };
        
        // Parse numeric values
        let tokens_num = tokens.replace(',', "").parse::<i64>().unwrap_or(0);
        let cost_num = actual_cost.replace(['$', ','], "").parse::<f64>().unwrap_or(0.0);
        
        Some(CcusageData {
            latest_session: session_start_str.to_string(),
            session_start: times.0.clone(),
            session_end: times.1.clone(),
            remaining_time: if is_active { times.2 } else { "已完成".to_string() },
            tokens: if tokens.is_empty() || tokens == "-" { "--".to_string() } else { tokens },
            cost: if actual_cost.is_empty() || actual_cost == "-" { "--".to_string() } else { actual_cost },
            status: if is_active { "ACTIVE".to_string() } else { "COMPLETED".to_string() },
            model,
            tokens_num,
            cost_num,
            reset_time: if is_active {
                // Parse session start time and calculate reset time
                chrono::NaiveDateTime::parse_from_str(session_start_str, "%m/%d/%Y, %I:%M:%S %p")
                    .ok()
                    .map(|dt| chrono::Local.from_local_datetime(&dt).unwrap() + chrono::Duration::hours(5))
                    .or_else(|| {
                        chrono::NaiveDateTime::parse_from_str(session_start_str, "%Y/%m/%d %H:%M:%S")
                            .ok()
                            .map(|dt| chrono::Local.from_local_datetime(&dt).unwrap() + chrono::Duration::hours(5))
                    })
            } else {
                None
            },
        })
    }
    
    fn clean_ansi_codes(&self, text: &str) -> String {
        let ansi_regex = Regex::new(r"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])").unwrap();
        ansi_regex.replace_all(text, "").trim().to_string()
    }
    
    fn calculate_session_times(&self, session_start_str: &str) -> (String, String, String) {
        let start_time = if session_start_str.contains("AM") || session_start_str.contains("PM") {
            DateTime::parse_from_str(session_start_str, "%m/%d/%Y, %I:%M:%S %p")
                .ok()
                .map(|dt| dt.with_timezone(&Local))
        } else {
            NaiveDateTime::parse_from_str(session_start_str, "%Y/%m/%d %H:%M:%S")
                .ok()
                .and_then(|dt| Local.from_local_datetime(&dt).single())
        };
        
        match start_time {
            Some(start) => {
                let end = start + chrono::Duration::hours(5);
                let now = Local::now();
                
                let remaining = if end > now {
                    let diff = end - now;
                    let hours = diff.num_hours();
                    let minutes = (diff.num_minutes() % 60) as u32;
                    format!("{}时{}分", hours, minutes)
                } else {
                    "已过期".to_string()
                };
                
                (
                    start.format("%H:%M:%S").to_string(),
                    end.format("%H:%M:%S").to_string(),
                    remaining,
                )
            }
            None => ("--".to_string(), "--".to_string(), "--".to_string()),
        }
    }
    
    fn update_ccusage(&mut self) {
        if let Some(npx_path) = &self.npx_path {
            let _ = Command::new(npx_path)
                .args(["--yes", "ccusage@latest", "--version"])
                .output();
        }
    }
    
    pub fn analyze_daily_costs(&self) -> HashMap<String, DailyCost> {
        let mut daily_costs = HashMap::new();
        
        if let Some(npx_path) = &self.npx_path {
            // Try daily command first
            if let Ok(output) = Command::new(npx_path)
                .args(["--yes", "ccusage@latest", "daily", "--order", "asc"])  // Remove calculate mode for speed
                .output() 
            {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    self.parse_daily_output(&stdout, &mut daily_costs);
                }
            }
            
            // If no data from daily, try blocks command
            if daily_costs.is_empty() {
                if let Ok(output) = Command::new(npx_path)
                    .args(["--yes", "ccusage@latest", "blocks"])  // Remove calculate mode for speed
                    .output()
                {
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        self.parse_blocks_output(&stdout, &mut daily_costs);
                    }
                }
            }
        }
        
        daily_costs
    }
    
    fn parse_daily_output(&self, output: &str, daily_costs: &mut HashMap<String, DailyCost>) {
        let date_regex = Regex::new(r"(\d{4})\s+(\d{2}-\d{2})|(\d{2}-\d{2})").unwrap();
        let cost_regex = Regex::new(r"\$?(\d+\.?\d*)").unwrap();
        
        for line in output.lines() {
            if line.contains('│') && !line.contains("Date") && !line.contains("Total") 
                && !line.contains('─') && !line.contains('═') {
                let parts: Vec<&str> = line.split('│').collect();
                if parts.len() >= 9 {
                    let date_str = self.clean_ansi_codes(parts[1]);
                    let cost_str = self.clean_ansi_codes(parts[8]);
                    
                    if let Some(date_match) = date_regex.find(&date_str) {
                        let month_day = if date_match.as_str().contains(' ') {
                            date_match.as_str().split(' ').nth(1).unwrap_or("")
                        } else {
                            date_match.as_str()
                        };
                        
                        if let Some(cost_match) = cost_regex.find(&cost_str) {
                            let cost_value: f64 = cost_match.as_str().parse().unwrap_or(0.0);
                            daily_costs.insert(
                                month_day.to_string(),
                                DailyCost {
                                    date: month_day.to_string(),
                                    cost: cost_value,
                                    sessions: 1,
                                }
                            );
                        }
                    }
                }
            }
        }
    }
    
    fn parse_blocks_output(&self, output: &str, daily_costs: &mut HashMap<String, DailyCost>) {
        let date_regex = Regex::new(r"(\d+/\d+/\d{4})|(\d{4}/\d+/\d+)").unwrap();
        let cost_regex = Regex::new(r"\$?(\d+\.?\d*)").unwrap();
        
        for line in output.lines() {
            if line.contains('│') && !line.contains("gap") && !line.contains("PROJECTED") 
                && !line.contains("Block Start") {
                let parts: Vec<&str> = line.split('│').collect();
                if parts.len() >= 6 {
                    let session_info = self.clean_ansi_codes(parts[1]);
                    let cost_str = self.clean_ansi_codes(parts[5]);
                    
                    if let Some(date_match) = date_regex.find(&session_info) {
                        if let Some(cost_match) = cost_regex.find(&cost_str) {
                            if cost_str != "-" {
                                let date_str = date_match.as_str();
                                let cost_value: f64 = cost_match.as_str().parse().unwrap_or(0.0);
                                
                                // Convert to MM/DD format
                                let formatted_date = if date_str.contains('/') {
                                    if let Ok(dt) = chrono::NaiveDate::parse_from_str(date_str, "%m/%d/%Y") {
                                        dt.format("%m/%d").to_string()
                                    } else if let Ok(dt) = chrono::NaiveDate::parse_from_str(date_str, "%Y/%m/%d") {
                                        dt.format("%m/%d").to_string()
                                    } else {
                                        continue;
                                    }
                                } else {
                                    continue;
                                };
                                
                                daily_costs.entry(formatted_date.clone())
                                    .and_modify(|e| {
                                        e.cost += cost_value;
                                        e.sessions += 1;
                                    })
                                    .or_insert(DailyCost {
                                        date: formatted_date,
                                        cost: cost_value,
                                        sessions: 1,
                                    });
                            }
                        }
                    }
                }
            }
        }
    }
}