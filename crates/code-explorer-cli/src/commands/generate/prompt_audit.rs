//! Metadata-only audit manifest for documentation prompts.
//!
//! This file deliberately records prompt families, model/profile settings and
//! role boundaries, not the full prompt bodies or repository excerpts. It gives
//! users a way to inspect how Code Explorer generated/enriched documentation without
//! creating a second artifact full of source code or secrets.

use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use serde_json::{json, Value};

use super::enrichment::{self, LlmConfig};

pub(super) struct PromptAuditOptions<'a> {
    pub(super) repo_name: &'a str,
    pub(super) target: &'a str,
    pub(super) enrich: bool,
    pub(super) enrich_profile: &'a str,
    pub(super) enrich_lang: &'a str,
    pub(super) enrich_citations: bool,
    pub(super) enrich_only: bool,
    pub(super) retry_queue: bool,
    pub(super) retry_at: Option<&'a str>,
    pub(super) traces_enabled: bool,
}

pub(super) fn write_prompt_audit(docs_dir: &Path, options: &PromptAuditOptions<'_>) -> Result<()> {
    let config = enrichment::load_llm_config();
    let payload = prompt_audit_payload(options, config.as_ref());
    let meta_dir = docs_dir.join("_meta");
    std::fs::create_dir_all(&meta_dir)?;
    std::fs::write(
        meta_dir.join("prompt-audit.json"),
        serde_json::to_string_pretty(&payload)?,
    )?;
    println!("  {} _meta/prompt-audit.json", "OK".green());
    Ok(())
}

fn prompt_audit_payload(options: &PromptAuditOptions<'_>, config: Option<&LlmConfig>) -> Value {
    let profile = config
        .map(|cfg| enrichment::get_profile_with_overrides(options.enrich_profile, cfg))
        .unwrap_or_else(|| enrichment::get_profile(options.enrich_profile));

    json!({
        "schemaVersion": 1,
        "generatedAt": chrono::Utc::now().to_rfc3339(),
        "generator": {
            "name": "code-explorer",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "project": {
            "name": options.repo_name,
        },
        "run": {
            "target": options.target,
            "enrichRequested": options.enrich,
            "enrichOnly": options.enrich_only,
            "retryQueue": options.retry_queue,
            "retryAt": options.retry_at,
            "tracesEnabled": options.traces_enabled,
        },
        "llm": llm_summary(config),
        "enrichment": {
            "language": options.enrich_lang,
            "citations": options.enrich_citations,
            "profile": {
                "name": options.enrich_profile,
                "maxEvidence": profile.max_evidence,
                "maxRetries": profile.max_retries,
                "timeoutSecs": profile.timeout_secs,
                "minGapMs": profile.min_gap_ms,
                "useJsonSchema": profile.use_json_schema,
                "reviewCritical": profile.review_critical,
            },
            "rolePolicy": {
                "system": "Static authoring rules, output contract, allowed source ids, and safety policy only.",
                "user": "Repository markdown and evidence excerpts, explicitly marked as untrusted context.",
                "tool": "No tool-role prompt content is produced by the docs generator.",
            },
            "contextPolicy": {
                "evidenceRole": "user",
                "fullPromptsStored": false,
                "evidenceExcerptsStoredInAudit": false,
                "sourceIdsOnly": true,
                "promptInjectionBoundary": "Repository content must be treated as evidence, never as instructions.",
                "untrustedContextMarkers": [
                    "BEGIN_UNTRUSTED_CONTEXT",
                    "END_UNTRUSTED_CONTEXT"
                ],
            },
            "promptFamilies": [
                {
                    "id": "docs.enrichment.structured.page",
                    "purpose": "Enrich one generated Markdown page with DeepWiki-style lead, section augments, related pages and closing summary.",
                    "systemRoleContainsEvidence": false,
                    "userRoleContainsEvidence": true,
                    "outputContract": "EnrichedPayload JSON; json_schema when profile.useJsonSchema=true, json_object otherwise.",
                },
                {
                    "id": "docs.enrichment.sectioned.lead_closing",
                    "purpose": "Generate lead and closing summary for large pages handled in per-section mode.",
                    "systemRoleContainsEvidence": false,
                    "userRoleContainsEvidence": true,
                    "outputContract": "LeadClosingPayload JSON object.",
                },
                {
                    "id": "docs.enrichment.sectioned.section",
                    "purpose": "Generate one SectionAugment for a bounded GNX section of a large page.",
                    "systemRoleContainsEvidence": false,
                    "userRoleContainsEvidence": true,
                    "outputContract": "SectionAugment JSON object.",
                },
                {
                    "id": "docs.enrichment.freeform.fallback",
                    "purpose": "Legacy fallback rewrite when structured JSON parsing cannot be salvaged.",
                    "systemRoleContainsEvidence": false,
                    "userRoleContainsEvidence": true,
                    "outputContract": "Markdown page content.",
                },
                {
                    "id": "docs.enrichment.review.critical",
                    "purpose": "Review enriched overview, architecture and functional guide pages when the active profile enables critical review.",
                    "systemRoleContainsEvidence": false,
                    "userRoleContainsEvidence": true,
                    "outputContract": "Revised Markdown, accepted only if preservation checks pass.",
                }
            ],
        },
        "artifacts": {
            "provenance": "_meta/provenance.json",
            "retryQueue": "_meta/queue.json",
            "llmResponseCache": "_meta/cache/llm/",
            "malformedResponseDebug": "_meta/debug/",
        },
        "privacy": {
            "storesLlmSecrets": false,
            "storesOauthTokens": false,
            "storesProviderEndpoint": false,
            "storesRepositoryPaths": false,
        },
    })
}

fn llm_summary(config: Option<&LlmConfig>) -> Value {
    match config {
        Some(cfg) => json!({
            "configured": true,
            "provider": safe_provider(cfg),
            "model": &cfg.model,
            "maxTokens": cfg.max_tokens,
            "reasoningEffortConfigured": &cfg.reasoning_effort,
            "reasoningEffortEffectiveForEnrichment": enrichment::clamp_enrichment_effort(&cfg.reasoning_effort),
            "bigContext": {
                "model": cfg.big_context_model.as_deref(),
                "thresholdBytes": cfg.big_context_threshold_bytes.unwrap_or(LlmConfig::BIG_CONTEXT_DEFAULT_THRESHOLD),
                "maxTokens": cfg.big_context_max_tokens,
            },
            "enrichmentKnobs": {
                "sectionMaxTokens": cfg.section_max_tokens(),
                "monolithicMaxTokensFloor": cfg.monolithic_max_tokens_floor(),
                "sectionContentSnippetBytes": cfg.section_content_snippet_bytes(),
            },
        }),
        None => json!({
            "configured": false,
            "provider": null,
            "model": null,
            "reasoningEffortConfigured": null,
            "reasoningEffortEffectiveForEnrichment": null,
        }),
    }
}

fn safe_provider(config: &LlmConfig) -> String {
    let provider = config.provider.trim();
    if !provider.is_empty() {
        return provider.to_string();
    }
    let lower = config.base_url.to_ascii_lowercase();
    if lower.contains("generativelanguage.googleapis.com") || lower.contains("googleapis.com") {
        "gemini".to_string()
    } else if lower.contains("openrouter.ai") {
        "openrouter".to_string()
    } else if lower.contains("anthropic.com") {
        "anthropic".to_string()
    } else if lower.contains("localhost") || lower.contains("127.0.0.1") {
        "local".to_string()
    } else if lower.contains("chatgpt.com") {
        "chatgpt".to_string()
    } else {
        "openai".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_options() -> PromptAuditOptions<'static> {
        PromptAuditOptions {
            repo_name: "sample",
            target: "html",
            enrich: true,
            enrich_profile: "fast",
            enrich_lang: "fr",
            enrich_citations: true,
            enrich_only: false,
            retry_queue: false,
            retry_at: None,
            traces_enabled: false,
        }
    }

    fn sample_config() -> LlmConfig {
        LlmConfig {
            provider: "chatgpt".to_string(),
            api_key: "secret-api-key".to_string(),
            base_url: "https://chatgpt.com/backend-api/codex".to_string(),
            model: "gpt-5.5".to_string(),
            max_tokens: 8192,
            reasoning_effort: "high".to_string(),
            big_context_model: Some("gpt-5.5".to_string()),
            big_context_threshold_bytes: Some(40_000),
            big_context_max_tokens: Some(16_384),
            enrichment: None,
        }
    }

    #[test]
    fn prompt_audit_payload_omits_secrets_and_provider_url() {
        let payload = prompt_audit_payload(&sample_options(), Some(&sample_config()));
        let text = serde_json::to_string(&payload).unwrap();

        assert_eq!(payload["llm"]["provider"], "chatgpt");
        assert_eq!(payload["llm"]["model"], "gpt-5.5");
        assert_eq!(
            payload["llm"]["reasoningEffortEffectiveForEnrichment"],
            "medium"
        );
        assert!(!text.contains("secret-api-key"));
        assert!(!text.contains("backend-api"));
        assert!(!text.contains("api_key"));
        assert!(!text.contains("base_url"));
        assert!(!text.contains("access_token"));
    }

    #[test]
    fn prompt_audit_records_evidence_role_boundary() {
        let payload = prompt_audit_payload(&sample_options(), None);

        assert_eq!(
            payload["enrichment"]["contextPolicy"]["evidenceRole"],
            "user"
        );
        assert_eq!(
            payload["enrichment"]["contextPolicy"]["untrustedContextMarkers"][0],
            "BEGIN_UNTRUSTED_CONTEXT"
        );
        assert_eq!(
            payload["enrichment"]["promptFamilies"][0]["systemRoleContainsEvidence"],
            false
        );
        assert_eq!(
            payload["enrichment"]["promptFamilies"][0]["userRoleContainsEvidence"],
            true
        );
    }
}
