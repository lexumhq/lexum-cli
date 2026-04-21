mod formatter;
use std::env;

use ved_runtime::domain_registry::{DomainInstance, DomainRegistry};
use ved_runtime::scheduler::Scheduler;
use ved_runtime::messaging::Message;
use ved_runtime::persistence::SnapshotManager;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("ved <command>");
        println!("Commands:");
        println!("  compile <file.ved>       - Compile a Ved file to bytecode");
        println!("  run <file.ved>           - Run a Ved file directly");
        println!("  view-trace <trace.json>  - View an execution trace");
        return;
    }

    use serde::Deserialize;

    #[derive(Deserialize, Default)]
    struct VedConfig {
        run: Option<RunConfig>,
    }

    #[derive(Deserialize)]
    struct RunConfig {
        max_cycles: Option<usize>,
        gas_limit: Option<usize>,
    }

    let mut max_cycles_cfg = 100;
    let mut gas_limit_cfg = 1000;

    if let Ok(yaml_contents) = std::fs::read_to_string("ved.yaml") {
        if let Ok(config) = serde_yaml::from_str::<VedConfig>(&yaml_contents) {
            if let Some(run_cfg) = config.run {
                if let Some(c) = run_cfg.max_cycles {
                    max_cycles_cfg = c;
                }
                if let Some(g) = run_cfg.gas_limit {
                    gas_limit_cfg = g;
                }
            }
        }
    }

    match args[1].as_str() {
        "view-trace" => {
            if args.len() < 3 {
                println!("Error: Missing trace file.\nUsage: ved view-trace <file.trace.json>");
                return;
            }
            let trace_path = &args[2];
            println!("[CLI] Loading execution trace: {}", trace_path);
            let content = std::fs::read_to_string(trace_path).unwrap_or_else(|e| {
                println!("Error reading trace file: {}", e);
                std::process::exit(1);
            });
            
            match ved_tracer::Tracer::format_trace_from_json(&content) {
                Ok(lines) => {
                    println!("\n--- EXECUTION TRACE VIEW ---");
                    for line in lines {
                        println!("{}", line);
                    }
                    println!("----------------------------");
                }
                Err(e) => {
                    println!("Failed to parse trace JSON: {}", e);
                }
            }
        }
        "run" => {
            if args.len() < 3 {
                println!("Error: Missing source file.\nUsage: ved run <file.ved>");
                return;
            }
            let source_path = &args[2];
            println!("[CLI] Reading source: {}", source_path);
            let source = std::fs::read_to_string(source_path).unwrap_or_else(|e| {
                println!("Error reading {}: {}", source_path, e);
                std::process::exit(1);
            });

            println!("[CLI] Compiling...");
            match ved_compiler::compile_source(&source) {
                Ok(program) => {
                    println!("[CLI] Compilation successful. Initiating Runtime.");
                    let mut registry = DomainRegistry::new();

                    for domain in program.domains {
                        println!("[Runtime] Initializing Domain: {}", domain.name);
                        let instance = DomainInstance::new(
                            domain.name.clone(),
                            domain.state_schema.clone(),
                            domain.clone(),
                        );
                        registry.register(instance);
                    }

                    let snapshot_file = format!("{}.snapshot.json", source_path);
                    let snapshot_mgr = SnapshotManager::new(&snapshot_file);
                    let mut is_resumed = false;

                    match snapshot_mgr.load() {
                        Ok(data) => {
                            println!("[CLI] Resuming from snapshot (cycle {})...", data.cycle);
                            if let Err(e) = snapshot_mgr.restore_into(data, &mut registry) {
                                println!("[CLI] Critical Error restoring snapshot: {}", e);
                                std::process::exit(1);
                            }
                            is_resumed = true;
                        }
                        Err(e) => {
                            println!("[CLI] No valid snapshot found ({}). Starting fresh.", e);
                        }
                    }

                    if !is_resumed {
                        let start_domain = if registry.instances.contains_key("Producer") {
                            "Producer".to_string()
                        } else if let Some(first_domain) = {
                            // Sort keys deterministically
                            let mut keys: Vec<&String> = registry.instances.keys().collect();
                            keys.sort();
                            keys.first().map(|k| k.to_string())
                        } {
                            first_domain
                        } else {
                            println!("[CLI] No domains loaded.");
                            return;
                        };

                        let first_trans = registry.instances.get(&start_domain).unwrap().bytecode.transitions.first();
                        let default_trans_name = if let Some(trans) = first_trans { trans.name.clone() } else { "run".to_string() };

                        let boot_msg = Message {
                            id: "boot_0".to_string(),
                            source_domain: "SYSTEM_BOOT".to_string(),
                            target_domain: start_domain.to_string(),
                            payload: default_trans_name,
                            priority: 0,
                            clock: 0,
                        };

                        println!("[CLI] Seeding boot message: {:?}", boot_msg);
                        let _ = registry.route_message(boot_msg);
                    }

                    let mut scheduler = Scheduler::new(registry).with_snapshots(snapshot_mgr);
                    
                    let mut max_cycles = max_cycles_cfg;
                    let mut output_format = "story";
                    
                    for arg in args.iter().skip(3) {
                        if arg.starts_with("--format=") {
                            output_format = arg.split('=').nth(1).unwrap_or("story");
                        } else if let Ok(c) = arg.parse::<usize>() {
                            max_cycles = c;
                        }
                    }

                    match output_format {
                        "raw" => println!("\n================ SCHEDULER START ================"),
                        _ => {
                            // The formatter prints its own beautiful boot sequences natively.
                        }
                    }

                    let run_metrics = scheduler.execute_until_quiescent(max_cycles, gas_limit_cfg);
                    
                    match output_format {
                        "raw" => {
                            for line in run_metrics.trace {
                                println!("{}", line);
                            }
                            println!("\n[CLI] Execution Metrics: Converged: {}, Steps: {}, Lo-Pri Executed: {}, Warnings: {}", run_metrics.converged, run_metrics.steps, run_metrics.low_priority_executed, run_metrics.warning_detected);
                            println!("================ SCHEDULER HALT ================\n");
                            println!("[CLI] Execution complete. Quiescence reached.");
                        }
                        _ => {
                            let mut formatter = formatter::StoryFormatter::new();
                            let formatted_lines = formatter.format_trace(&scheduler.tracer.events);
                            for line in formatted_lines {
                                println!("{}", line);
                            }
                            
                            println!("\n---------------- FINAL ----------------\n");
                            use colored::Colorize;
                            if run_metrics.converged {
                                println!("{}", "✔ System converged".green().bold());
                            } else if run_metrics.oscillating {
                                println!("{}", "⚠ System Oscillating (Failed to converge naturally)".yellow().bold());
                            } else {
                                println!("{}", "✖ System failed to converge".red().bold());
                            }
                            println!("∑ Total cycles: {}", run_metrics.steps);
                            if is_resumed {
                                println!("▣ Snapshot loaded & resaved");
                            } else {
                                println!("▣ Snapshot saved");
                            }
                            let trace_file = format!("{}.trace.json", source_path);
                            println!("☶ Trace saved: {}", trace_file);
                            
                            // Determinism Check
                            use std::collections::hash_map::DefaultHasher;
                            use std::hash::{Hash, Hasher};
                            let mut hasher = DefaultHasher::new();
                            for ev in &scheduler.tracer.events {
                                ev.cycle.hash(&mut hasher);
                                ev.domain.hash(&mut hasher);
                                ev.action.hash(&mut hasher);
                                ev.details.hash(&mut hasher);
                            }
                            let hash_val = hasher.finish();
                            println!("\n⟳ Determinism Check:");
                            println!("Run Hash: {:x}", hash_val);
                            println!();
                        }
                    }
                    
                    let trace_file = format!("{}.trace.json", source_path);
                    let json_trace = scheduler.tracer.dump_json();
                    if let Err(e) = std::fs::write(&trace_file, json_trace) {
                        if output_format == "raw" { println!("[CLI] Error writing trace file: {}", e); }
                    } else {
                        if output_format == "raw" { println!("[CLI] Wrote execution trace to {}", trace_file); }
                    }
                }
                Err(e) => {
                    println!("Error during compilation:\n{:?}", e);
                }
            }
        }
        "compile" => {
            if args.len() < 3 {
                println!("Error: Missing source file.\nUsage: ved compile <file.ved>");
                return;
            }
            let source_path = &args[2];
            let source = std::fs::read_to_string(source_path).unwrap();
            match ved_compiler::compile_source(&source) {
                Ok(program) => {
                    println!("[CLI] Compilation successful.");
                    use ved_compiler::codegen::BinaryPacker;
                    let bytes = BinaryPacker::serialize(&program);
                    let out_path = if source_path.ends_with(".ved") {
                        source_path.replace(".ved", ".vedc")
                    } else {
                        format!("{}.vedc", source_path)
                    };
                    
                    match std::fs::write(&out_path, &bytes) {
                        Ok(_) => println!("[CLI] Emitted raw bytecode binary to: {} ({} bytes)", out_path, bytes.len()),
                        Err(e) => println!("[CLI] Critical Error: Failed to write {}: {}", out_path, e),
                    }
                },
                Err(e) => println!("Error during compilation:\n{:?}", e),
            }
        }
        _ => println!("Unknown command: {}", args[1]),
    }
}
