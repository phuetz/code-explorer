# Code Explorer Modernization Roadmap

Code Explorer has a strong technical core: local-first code intelligence, a persistent knowledge graph, legacy ASP.NET MVC / EF6 depth, desktop UX, and an MCP surface for AI agents.

The next step is not feature sprawl. It is product modernization around the workflows that matter most.

## Competitive Positioning

### Where Code Explorer is already strong
- Local-first code intelligence with persistent graph context
- Strong impact analysis, generated documentation, and graph-backed Q&A
- Rare depth on legacy enterprise stacks, especially ASP.NET MVC 5 / EF6 / Telerik / AJAX mappings
- Desktop + CLI + MCP combination for both humans and agents

### Where competitors currently win
- **Sourcegraph** wins on “deep search” workflows, enterprise presentation, and multi-repo discovery
- **CodeSee** wins on collaborative maps, PR/change visualization, and guided architecture review
- **CodeScene** wins on hotspot storytelling, behavior-driven quality, and engineering management dashboards
- **NDepend** wins on architectural governance, dependency diagrams, and rule-based structure validation

### What Code Explorer should become
Code Explorer should be the best tool to:
1. Understand a complex codebase quickly
2. Explain and modernize legacy systems safely
3. Give AI agents high-quality structural context without cloud dependency

## Strategic Thesis

Code Explorer becomes market-leading if it combines:
- the workflow ergonomics of AI coding tools
- the visual clarity of architecture map tools
- the governance depth of engineering quality platforms
- and the privacy / locality / portability of a local desktop-first product

## 30-Step Execution Roadmap

### Phase 1 — Positioning and onboarding
1. Clarify the one-line value proposition everywhere.
2. Add explicit first-run journeys: explore, audit, modernize.
3. Add a sample/demo workspace flow.
4. Add persona-based onboarding for architect, maintainer, AI-agent user, and legacy .NET owner.
5. Add guided tours for graph, docs, chat, and impact.

### Phase 2 — Desktop workflow polish
6. Make desktop the primary narrative, not a sidecar.
7. Add resumable workspace/session context.
8. Replace raw views with task-focused dashboards.
9. Improve keyboard-first navigation and command flows.
10. Add first-class exports and sharable reports.

### Phase 3 — AI workflows
11. Promote chat from Q&A to guided workflows.
12. Add research templates: understand feature, trace incident, estimate refactor, prepare migration.
13. Add evidence, confidence, and next-step cards to multi-step answers.
14. Add one-click follow-up actions from AI outputs.
15. Add a unified search surface spanning NL, symbol, and graph query.

### Phase 4 — Visual code understanding
16. Add graph presets by task.
17. Add explicit architectural maps and domain boundaries.
18. Add review maps and change blast-radius cards.
19. Overlay hotspots, coupling, ownership, and tracing directly on maps.
20. Add annotations, saved maps, and guided architectural tours.

### Phase 5 — Governance and modernization
21. Add architecture rules and visible quality gates.
22. Add modernization scorecards for legacy systems.
23. Add prioritized refactoring plans from graph + git analytics.
24. Add team-facing health dashboards.
25. Add guided migration assistants for legacy web stacks.

### Phase 6 — Distribution and market readiness
26. Improve packaging and desktop release distribution.
27. Add benchmark repos and public demo assets.
28. Publish a comparison matrix vs Sourcegraph / CodeSee / CodeScene / NDepend.
29. Add privacy-safe opt-in telemetry/analytics architecture.
30. Launch around one flagship workflow: understand and modernize legacy systems with AI.

## Current Technical Progress

Recent modernization work already completed in the codebase:
- secure LLM secret handling without persisting API keys to disk
- bounded MCP / HTTP limits and safer desktop CLI invocation
- frontend E2E smoke suite with Playwright
- substantial lazy-loading and bundle-splitting across desktop UI

## Near-Term Priorities

If you want the highest leverage next:
- reduce the remaining heavy frontend chunks (`cytoscape`, `katex`, selected Shiki language chunks)
- add deeper frontend / desktop E2E coverage
- implement the first flagship workflow: “legacy modernization cockpit”
- turn the React chat into a source + graph workspace using the staged plan in [REACT_SOURCE_GRAPH_NAVIGATION.md](REACT_SOURCE_GRAPH_NAVIGATION.md)
