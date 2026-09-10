//! The `trace-doc` command: generate documentation from an execution trace using LLM.

use anyhow::Result;
use colored::Colorize;
use serde_json::Value;
use std::path::PathBuf;

use code_explorer_core::llm::{sanitize_llm_error_body, PROMPT_CONTEXT_SAFETY};
use code_explorer_core::trace;
use code_explorer_db::snapshot;

pub async fn run(trace_file: &str, output_file: Option<&str>, path: Option<&str>) -> Result<()> {
    let repo_path = if let Some(p) = path {
        PathBuf::from(p)
    } else {
        std::env::current_dir()?
    };

    // Load config
    let config = super::generate::load_llm_config();
    let config = match config {
        Some(c) => c,
        None => {
            println!(
                "{} No LLM configured. Create ~/.codeexplorer/chat-config.json with:",
                "ERROR".red()
            );
            println!();
            println!("  {{");
            println!("    \"provider\": \"gemini\",");
            println!("    \"api_key\": \"YOUR_API_KEY\",");
            println!(
                "    \"base_url\": \"https://generativelanguage.googleapis.com/v1beta/openai\","
            );
            println!("    \"model\": \"gemini-2.5-flash\",");
            println!("    \"max_tokens\": 8192,");
            println!("    \"reasoning_effort\": \"high\"");
            println!("  }}");
            return Ok(());
        }
    };

    let snap_path = repo_path.join(".codeexplorer").join("graph.bin");
    if !snap_path.exists() {
        println!(
            "{} No index found. Run 'code-explorer analyze' first.",
            "ERROR".red()
        );
        return Ok(());
    }

    let log_path = std::path::Path::new(trace_file);
    if !log_path.exists() {
        println!("{} Trace file not found: {}", "ERROR".red(), trace_file);
        return Ok(());
    }

    println!("{} Loading graph...", "->".cyan());
    let graph = snapshot::load_snapshot(&snap_path)
        .map_err(|e| anyhow::anyhow!("Failed to load graph: {}", e))?;

    println!("{} Parsing trace file: {}", "->".cyan(), trace_file);
    let trace_content = std::fs::read_to_string(log_path)?;

    let steps = trace::parse_trace(&trace_content)?;

    if steps.is_empty() {
        println!("{} No valid steps found in trace file.", "WARN".yellow());
        return Ok(());
    }

    let name_to_ids = trace::build_name_index(&graph);

    let mut matched_steps = 0;
    let mut context_blocks = Vec::new();

    for (i, step) in steps.iter().enumerate() {
        let method_name_opt = step
            .get("method")
            .or(step.get("name"))
            .and_then(|v| v.as_str());

        let mut context_block = format!(
            "### Step {}: {}\n",
            i + 1,
            method_name_opt.unwrap_or("Unknown")
        );
        context_block.push_str(&format!(
            "Trace data: {}\n",
            serde_json::to_string(step).unwrap_or_default()
        ));

        if let Some(full_method_name) = method_name_opt {
            if let Some(node_id) =
                trace::resolve_method_node(&graph, &name_to_ids, full_method_name)
            {
                if let Some(node) = graph.get_node(&node_id) {
                    matched_steps += 1;
                    // Path traversal guard: the graph node's file_path comes
                    // from a snapshot that could contain `..` segments
                    // (corrupted or hand-crafted), and we must not let them
                    // escape the repo root and exfiltrate arbitrary files
                    // into the LLM prompt.
                    let full_path = repo_path.join(&node.properties.file_path);
                    let source_safe =
                        match (full_path.canonicalize().ok(), repo_path.canonicalize().ok()) {
                            (Some(canon), Some(root)) => canon.starts_with(&root),
                            _ => false,
                        };
                    if source_safe {
                        if let (Some(start), Some(end)) =
                            (node.properties.start_line, node.properties.end_line)
                        {
                            if let Some(source) =
                                trace::extract_source_lines(&full_path, start, end)
                            {
                                context_block.push_str(&format!(
                                    "Source code (`{}`):\n```\n{}\n```\n",
                                    node.properties.file_path, source
                                ));
                            }
                        }
                    }
                }
            }
        }
        context_blocks.push(context_block);
    }

    println!(
        "  Found {} steps, matched {} with source code",
        steps.len(),
        matched_steps
    );
    println!(
        "{} Generating documentation with LLM ({})...",
        "->".cyan(),
        config.model
    );

    let system_prompt = format!("{}\n\n{}", PROMPT_CONTEXT_SAFETY, "You are an expert technical writer and software architect. \
        Your task is to write a comprehensive business process documentation based on an execution trace. \
        You will receive the chronological steps of the execution, including the actual parameter values passed \
        at runtime, and the corresponding source code for each step. \
        Explain the business logic, what data is transformed, the conditions applied, and the outcome of the process. \
        Format the response in clean Markdown with appropriate headings, a high-level summary, and a step-by-step breakdown. \
        Do not output code blocks of the entire source code, only small snippets if they clarify a specific business rule.");

    let user_prompt = format!(
        "Please document the following execution trace:\n\n{}",
        context_blocks.join("\n\n")
    );

    let messages = vec![
        serde_json::json!({ "role": "system", "content": system_prompt }),
        serde_json::json!({ "role": "user", "content": user_prompt }),
    ];

    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let mut body = serde_json::json!({
        "model": config.model,
        "messages": messages,
        "max_tokens": config.max_tokens,
        "temperature": 0.2,
    });

    let effort = config.reasoning_effort.trim().to_lowercase();
    if !effort.is_empty() && effort != "none" {
        body["reasoning_effort"] = Value::String(effort);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()?;

    let mut request = client.post(&url).json(&body);
    if !config.api_key.is_empty() {
        request = request.header("Authorization", format!("Bearer {}", config.api_key));
    }

    let response = request.send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let body = sanitize_llm_error_body(&body, &[&config.api_key], 500);
        return Err(anyhow::anyhow!("LLM HTTP error {}: {}", status, body));
    }

    let result_json: Value = response.json().await?;
    let content = result_json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("No content returned.");

    if let Some(out) = output_file {
        std::fs::write(out, content)?;
        println!("{} Documentation saved to {}", "OK".green(), out);
    } else {
        println!("\n{}\n", "\u{2500}".repeat(60));
        println!("{}", content);
        println!("\n{}\n", "\u{2500}".repeat(60));
    }

    Ok(())
}
