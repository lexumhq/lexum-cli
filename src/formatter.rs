use ved_tracer::TraceEvent;
use colored::*;
use std::collections::HashMap;

pub struct StoryFormatter {
    state_cache: HashMap<String, HashMap<String, String>>,
}

impl StoryFormatter {
    pub fn new() -> Self {
        Self {
            state_cache: HashMap::new(),
        }
    }

    pub fn format_trace(&mut self, events: &[TraceEvent]) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push("================ VED EXECUTION =================".bright_white().bold().to_string());
        
        // Setup initial boot message manually or expect it to be inferred
        // Since we don't have Boot explicit in TraceEvent easily, we process cycles.
        
        let mut current_cycle = std::usize::MAX;
        
        for ev in events {
            if ev.cycle != current_cycle {
                current_cycle = ev.cycle;
                lines.push(format!("\n---------------- CYCLE {} ----------------", current_cycle).bright_black().bold().to_string());
            }

            match ev.action.as_str() {
                "GOAL_FAILED" => {
                    lines.push(format!("✖ Goal not satisfied: {}", ev.details).red().bold().to_string());
                    lines.push("↳ Scheduling recovery".bright_black().to_string());
                }
                "GOAL_MET" => {
                    lines.push(format!("✔ Goal satisfied: {}", ev.details).green().bold().to_string());
                }
                "PROCESS_MESSAGE" => {
                    lines.push(format!("\n▶ Processing: {}.receive({})", ev.domain, ev.details).blue().bold().to_string());
                }
                "STATE_MUTATED" => {
                    // details is like "jobs_dispatched": 1, "online": 1
                    let new_state_str = ev.details.clone();
                    let parsed = match parse_state_map(&new_state_str) {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
                    
                    let domain_cache = self.state_cache.entry(ev.domain.clone()).or_insert_with(HashMap::new);
                    
                    for (k, v) in parsed {
                        let from = domain_cache.get(&k).cloned().unwrap_or_else(|| "0".to_string());
                        if from != v {
                            lines.push(format!("≈ State change: {} = {} → {}", k, from, v).bright_magenta().to_string());
                            domain_cache.insert(k, v);
                        } else {
                            // If we want to show unchanged, maybe skip to keep neat.
                        }
                    }
                }
                "ROUTING_MESSAGE" => {
                    // details: Target: X, Payload: Y, Priority: Z, Clock: W
                    // Let's parse Target and Payload.
                    let (target, payload) = parse_routing_details(&ev.details);
                    lines.push(format!("↳ Emitting message → {} → {}", payload, target).bright_yellow().to_string());
                }
                "EXECUTION_ERROR" => {
                    lines.push(format!("✖ Execution error: {}", ev.details).red().bold().to_string());
                }
                "DETERMINISTIC_QUIESCENCE" => {
                    // Optional to print? We can skip or dim it.
                }
                "DETERMINISM_FAULT" => {
                    lines.push(format!("⚠ DETERMINISM FAULT: {}", ev.details).red().on_yellow().bold().to_string());
                }
                _ => {
                    lines.push(format!("[{}] {}", ev.action, ev.details).bright_black().to_string());
                }
            }
        }
        
        lines
    }
}

fn parse_state_map(s: &str) -> Result<HashMap<String, String>, String> {
    let mut map = HashMap::new();
    // format is "key1": val1, "key2": val2
    if s.is_empty() { return Ok(map); }
    let pairs: Vec<&str> = s.split(", ").collect();
    for pair in pairs {
        let parts: Vec<&str> = pair.splitn(2, ": ").collect();
        if parts.len() == 2 {
            let k = parts[0].trim_matches('"').to_string();
            let v = parts[1].to_string();
            map.put(k, v);
        }
    }
    Ok(map)
}

trait PutExt { fn put(&mut self, k: String, v: String); }
impl PutExt for HashMap<String, String> { fn put(&mut self, k: String, v: String) { self.insert(k, v); } }

fn parse_routing_details(s: &str) -> (String, String) {
    // Target: Worker, Payload: ProcessJob, Priority: 0
    let mut target = "Unknown".to_string();
    let mut payload = "Unknown".to_string();
    for part in s.split(", ") {
        if part.starts_with("Target: ") {
            target = part.replace("Target: ", "");
        } else if part.starts_with("Payload: ") {
            payload = part.replace("Payload: ", "");
        }
    }
    (target, payload)
}
