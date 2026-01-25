//! SQLite persistence layer for user-saved pipeline configurations.
//!
//! Provides CRUD operations for pipeline configs and seeds example data on first run.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use tracing::{error, info};

use crate::dto::{EdgeInfo, NodeInfo, PipelineInfo, SavePipelineRequest};

/// Initializes the database, creating tables if needed.
pub fn init_db(path: &str) -> Result<Connection> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).context("failed to create db directory")?;
    }
    let conn = Connection::open(path).context("failed to open database")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS user_pipelines (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            config_json TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );"
    ).context("failed to create table")?;
    info!("Database initialized at {}", path);
    Ok(conn)
}

/// Lists all user-saved pipeline configurations.
pub fn list_user_pipelines(conn: &Connection) -> Vec<PipelineInfo> {
    let mut stmt = match conn.prepare("SELECT id, name, description, config_json FROM user_pipelines") {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to prepare list query: {}", e);
            return vec![];
        }
    };

    let rows = match stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let name: String = row.get(1)?;
        let description: String = row.get(2)?;
        let config_json: String = row.get(3)?;
        Ok((id, name, description, config_json))
    }) {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to query user pipelines: {}", e);
            return vec![];
        }
    };

    rows.filter_map(|row| {
        let (id, name, description, config_json) = row.ok()?;
        let config: StoredConfig = serde_json::from_str(&config_json).ok()?;
        Some(PipelineInfo {
            id,
            name,
            description,
            nodes: config.nodes,
            edges: config.edges,
        })
    }).collect()
}

/// Saves or updates a pipeline configuration.
pub fn save_pipeline(conn: &Connection, req: &SavePipelineRequest) -> Result<()> {
    let config = StoredConfig {
        nodes: req.nodes.clone(),
        edges: req.edges.clone(),
    };
    let config_json = serde_json::to_string(&config).context("failed to serialize config")?;
    conn.execute(
        "INSERT OR REPLACE INTO user_pipelines (id, name, description, config_json, updated_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now'))",
        params![req.id, req.name, req.description, config_json],
    ).context("failed to save pipeline")?;
    info!("Saved pipeline config: {} ({})", req.name, req.id);
    Ok(())
}

/// Deletes a pipeline configuration by ID.
pub fn delete_pipeline(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM user_pipelines WHERE id = ?1", params![id])
        .context("failed to delete pipeline")?;
    info!("Deleted pipeline config: {}", id);
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredConfig {
    nodes: Vec<NodeInfo>,
    edges: Vec<EdgeInfo>,
}

/// Seed example configs if the database is empty
pub fn seed_examples(conn: &Connection) -> Result<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM user_pipelines", [], |r| r.get(0))?;
    if count > 0 {
        info!("Database already has {} configs, skipping seed", count);
        return Ok(());
    }

    info!("Seeding example configs...");

    let examples = vec![
        // 1. Blog Post Writer (Prompt Chaining)
        ExampleConfig {
            id: "blog-post-writer",
            name: "Blog Post Writer",
            description: "Sequential content creation: outline → draft → polish. Demonstrates prompt chaining for iterative refinement.",
            nodes: vec![
                ("llm1", "llm", "Create a structured outline with an engaging intro, 3 main points with supporting details, and a compelling conclusion."),
                ("gate1", "gate", ""),
                ("llm2", "llm", "Expand the outline into a full draft. Write engaging prose, add examples, and ensure smooth transitions between sections."),
                ("llm3", "llm", "Polish the draft: improve flow, strengthen the opening hook, add a call-to-action, and ensure consistent tone throughout."),
            ],
            edges: vec![
                ("input", "llm1", None),
                ("llm1", "gate1", None),
                ("gate1", "llm2", None),
                ("llm2", "llm3", None),
                ("llm3", "output", None),
            ],
        },

        // 2. Customer Support Bot (Routing)
        ExampleConfig {
            id: "customer-support",
            name: "Customer Support Bot",
            description: "Routes queries to specialized handlers. Demonstrates routing for domain-specific expertise.",
            nodes: vec![
                ("router", "router", ""),
                ("technical_llm", "llm", "You are a technical support specialist. Diagnose issues systematically, provide step-by-step troubleshooting, and escalate complex problems with detailed notes."),
                ("billing_llm", "llm", "You are a billing specialist. Handle payment inquiries, explain charges clearly, process refund requests, and resolve subscription issues professionally."),
                ("general_llm", "llm", "You are a general support agent. Answer FAQs warmly, guide users to resources, and identify when specialized help is needed."),
            ],
            edges: vec![
                ("input", "router", None),
                ("router", "technical_llm,billing_llm,general_llm", Some("conditional")),
                ("technical_llm,billing_llm,general_llm", "output", None),
            ],
        },

        // 3. Document Reviewer (Parallelization)
        ExampleConfig {
            id: "document-reviewer",
            name: "Document Reviewer",
            description: "Parallel analysis of grammar, style, and facts. Demonstrates parallelization for comprehensive coverage.",
            nodes: vec![
                ("coordinator", "coordinator", "Break the document into logical sections for parallel review."),
                ("grammar_llm", "llm", "Review for grammar, spelling, and punctuation. List each issue with its location and suggested correction."),
                ("style_llm", "llm", "Evaluate writing style: tone consistency, clarity, readability, and engagement. Suggest specific improvements."),
                ("facts_llm", "llm", "Verify factual claims and check for logical inconsistencies. Flag any statements that need citations or clarification."),
                ("aggregator", "aggregator", "Combine all reviews into a prioritized feedback report. Group by severity: critical, important, minor suggestions."),
            ],
            edges: vec![
                ("input", "coordinator", None),
                ("coordinator", "grammar_llm,style_llm,facts_llm", Some("parallel")),
                ("grammar_llm,style_llm,facts_llm", "aggregator", None),
                ("aggregator", "output", None),
            ],
        },

        // 4. Research Assistant (Orchestrator-Worker)
        ExampleConfig {
            id: "research-assistant",
            name: "Research Assistant",
            description: "Dynamic task decomposition for complex research. Demonstrates orchestrator-worker for adaptive workflows.",
            nodes: vec![
                ("orchestrator", "orchestrator", "Analyze the research question. Identify key aspects to investigate. Dispatch workers for: foundational context, current data/trends, and comparative analysis."),
                ("context_worker", "worker", "Research the historical background and foundational concepts. Provide context that frames the current state of knowledge."),
                ("data_worker", "worker", "Find current statistics, recent studies, and emerging trends. Focus on data from the last 2-3 years."),
                ("synthesizer", "synthesizer", "Synthesize all findings into a coherent research summary. Highlight key insights, note conflicting information, and suggest areas for further investigation."),
            ],
            edges: vec![
                ("input", "orchestrator", None),
                ("orchestrator", "context_worker,data_worker", Some("dynamic")),
                ("context_worker,data_worker", "synthesizer", None),
                ("synthesizer", "output", None),
            ],
        },

        // 5. Code Generator (Evaluator-Optimizer)
        ExampleConfig {
            id: "code-generator",
            name: "Code Generator",
            description: "Generate code with self-critique loop. Demonstrates evaluator-optimizer for quality assurance.",
            nodes: vec![
                ("generator", "llm", "Write clean, well-documented code for the request. Include error handling, input validation, and clear comments. Follow best practices for the language."),
                ("evaluator", "evaluator", "Review the generated code for: correctness, edge cases, security vulnerabilities, performance, and readability. If issues found, provide specific feedback. If code meets quality standards, approve for output."),
            ],
            edges: vec![
                ("input", "generator", None),
                ("generator", "evaluator", None),
                ("evaluator", "generator", Some("conditional")),
                ("evaluator", "output", None),
            ],
        },
    ];

    let example_count = examples.len();
    for ex in examples {
        let nodes: Vec<NodeInfo> = ex.nodes.iter().map(|(id, node_type, prompt)| {
            NodeInfo {
                id: id.to_string(),
                node_type: node_type.to_string(),
                model: None,
                prompt: if prompt.is_empty() { None } else { Some(prompt.to_string()) },
            }
        }).collect();

        let edges: Vec<EdgeInfo> = ex.edges.iter().map(|(from, to, edge_type)| {
            let from_val = if from.contains(',') {
                serde_json::Value::Array(from.split(',').map(|s| serde_json::Value::String(s.to_string())).collect())
            } else {
                serde_json::Value::String(from.to_string())
            };
            let to_val = if to.contains(',') {
                serde_json::Value::Array(to.split(',').map(|s| serde_json::Value::String(s.to_string())).collect())
            } else {
                serde_json::Value::String(to.to_string())
            };
            EdgeInfo { from: from_val, to: to_val, edge_type: edge_type.map(|s| s.to_string()) }
        }).collect();

        let config = StoredConfig { nodes, edges };
        let config_json = serde_json::to_string(&config)?;

        conn.execute(
            "INSERT INTO user_pipelines (id, name, description, config_json) VALUES (?1, ?2, ?3, ?4)",
            params![ex.id, ex.name, ex.description, config_json],
        )?;
        info!("  Seeded: {}", ex.name);
    }

    info!("Seeded {} example configs", example_count);
    Ok(())
}

struct ExampleConfig {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    nodes: Vec<(&'static str, &'static str, &'static str)>,
    edges: Vec<(&'static str, &'static str, Option<&'static str>)>,
}
