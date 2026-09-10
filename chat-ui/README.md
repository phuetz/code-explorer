# Code Explorer Chat

Web app de chat IA pour interroger un graphe de code Code Explorer. Notre alternative
open-source à Open WebUI, conçue pour être déployée chez des clients (agile-up.com)
sans contrainte de licence "fair-source".

> **Statut V0** — squelette UI mock. Le backend `code-explorer-mcp` n'est pas encore
> branché. Voir [Roadmap](#roadmap).

---

## Stack

- **Vite 7** + **React 19** + **TypeScript strict**
- **Tailwind CSS v4** (via `@tailwindcss/vite`, plus de `tailwind.config.js`)
- **Zustand** (state + persist localStorage)
- **react-markdown** + `remark-gfm` (rendu markdown GitHub-flavored — Shiki coloration syntaxique prévue V2)
- **lucide-react** (icônes)
- **License MIT** (commercial-friendly, contrairement à OWUI)

---

## Démarrage

```bash
npm install
npm run dev          # http://localhost:5174
npm run build        # bundle prod (~110 KB gzip)
npm run preview      # preview du build
npm run lint         # ESLint
```

---

## Architecture

```
src/
  App.tsx                   — entry, monte ChatPanel
  index.css                 — Tailwind v4 + reset
  main.tsx                  — bootstrap React
  api/
    mcp-client.ts           — wrapper fetch vers code-explorer-mcp HTTP (mock V0)
  components/
    chat/
      ChatPanel.tsx         — layout (sidebar + main + input)
      ChatSidebar.tsx       — liste sessions, create/delete
      ChatMessages.tsx      — scroll auto + empty state
      ChatMessage.tsx       — bulle user/assistant avec markdown
      ChatInput.tsx         — textarea + Send (Shift+Enter pour newline)
    ui/
      Markdown.tsx          — wrapper react-markdown
  hooks/
    use-chat.ts             — sendMessage(), gère streaming flag
  stores/
    chat-store.ts           — Zustand persist : sessions[], currentSessionId
  types/
    chat.ts                 — Message, Session, MCPTool, ToolCall
```

---

## Backend (V1+)

Pour brancher le vrai backend `code-explorer-mcp` :

```bash
# Dans le repo code-explorer
code-explorer serve --http 8080
```

Puis :

```bash
# Dans code-explorer-chat
echo "VITE_MCP_URL=http://localhost:8080" > .env.local
npm run dev
```

`mcp-client.ts` parlera à `code-explorer-mcp` via JSON-RPC 2.0 sur HTTP transport.
Tools exposés : 27 (list_repos, query, search_code, impact, hotspots, etc. — voir
`code-explorer/crates/code-explorer-mcp/src/backend/local.rs`).

---

## Roadmap

### V0 — Squelette (livré 2026-05-04)

- [x] Scaffolding Vite + React + TS + Tailwind v4
- [x] Layout chat 3-zones (sidebar + messages + input)
- [x] Store sessions persisté (localStorage)
- [x] Mock chat (faux delay 800ms, message canned)
- [x] Markdown rendering (GFM)

### V1 — Backend réel

- [ ] Brancher `mcp-client.ts` au vrai `code-explorer-mcp` HTTP (JSON-RPC 2.0)
- [ ] Streaming SSE réel (events progress/tool_call/result)
- [ ] Multi-provider config UI (OpenAI / Anthropic / Gemini / Ollama)
- [ ] Affichage tool_calls inline avec status (pending/running/done/error)
- [ ] Citations sources (file paths cliquables)

### V2 — Polish UX (inspirations OWUI sans copier)

- [ ] Palette prompts (Cmd+K) avec templates par mode (qa / deep_research / etc.)
- [ ] Markdown fancy : Shiki (40+ langues) + Mermaid lazy load
- [ ] Fork de session (clone messages jusqu'à un point X)
- [ ] Pin de message (sidebar Pinned filter)
- [ ] Slot modèle visible en permanence (changement à la volée)

### V3 — Déploiement client

- [ ] Dockerfile + `docker-compose.yml` (code-explorer-chat + code-explorer-mcp ensemble)
- [ ] Auth basique (single-user d'abord, multi-tenant plus tard)
- [ ] Config env-driven : `MCP_URL`, `OPENAI_API_KEY`, etc.
- [ ] Push GitHub `phuetz/code-explorer-chat` (publique sous MIT)

---

## Pourquoi pas Open WebUI ?

Voir le rapport
[`claude-et-patrice/propositions/CHAT-OPENWEBUI-2026-05-04.md`](https://github.com/phuetz/claude-et-patrice/blob/master/propositions/CHAT-OPENWEBUI-2026-05-04.md).
TL;DR : la licence BSD-3 modifiée d'OWUI ("fair-source", branding obligatoire
sauf <50 users ou enterprise license payante) est bloquante pour la
commercialisation via agile-up.com.

---

## License

MIT — voir [LICENSE](./LICENSE).
