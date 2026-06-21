# CLAUDE.md

Instructions pour les futures sessions Claude Code travaillant sur `code-explorer-chat`.

## Project Overview

Web app de chat IA = front-end React qui consomme un backend `code-explorer-mcp` (Rust)
via JSON-RPC 2.0 sur HTTP. Conçu pour être déployé chez des clients agile-up.com.
Alternative open-source à Open WebUI sans contrainte de licence fair-source.

**Statut courant : V0** — squelette UI mock, pas connecté au backend réel.

## Stack

- Vite 7 + React 19 + TypeScript strict (`"strict": true` + `erasableSyntaxOnly` + `noUnusedLocals/Parameters`)
- Tailwind CSS v4 (via `@tailwindcss/vite`, pas de `tailwind.config.js`)
- Zustand + persist (localStorage)
- react-markdown + remark-gfm (Shiki prévu V2)
- lucide-react (icônes)

## Build / Dev

```bash
npm run dev      # http://localhost:5174
npm run build    # tsc -b && vite build (vérifie types + bundle)
npm run lint
```

## Conventions

- **Pas de classes CSS custom** sauf cas particulier — tout en Tailwind.
- **Pas de `tailwind.config.js`** : Tailwind v4 lit `@theme` directives dans le CSS si
  besoin de customisation. Pour V0, défaults suffisent.
- **TypeScript strict** : `erasableSyntaxOnly` interdit les *parameter properties*
  (`constructor(private foo: string)`). Utiliser `readonly foo: string;` + assignment.
- **Pas de tests** pour V0 (squelette). À ajouter avec Vitest dès qu'il y a de la
  logique réelle.
- **Naming** : composants PascalCase, hooks `use-foo.ts`, stores `foo-store.ts`.
- **Imports** : pas d'alias `@/` configuré, paths relatifs.

## Architecture en 1 minute

- `src/App.tsx` → `ChatPanel` qui assemble `ChatSidebar` + `ChatMessages` + `ChatInput`
- `useChat()` hook = entrée unique pour `sendMessage(content)` — gère le flag
  `isStreaming` et appelle `mcpClient.chat()`.
- `useChatStore` = Zustand persist, source de vérité pour `sessions[]` +
  `currentSessionId` + `isStreaming`. Exposé via `useChatStore((s) => ...)`.
- `mcp-client.ts` = wrapper. **V0 : retourne du mock**. V1 : passera au JSON-RPC 2.0
  vers `code-explorer serve --http 8080`.

## Roadmap (extraite du README)

V1 : backend réel + streaming + multi-provider config UI + tool_calls inline.
V2 : palette prompts, Shiki+Mermaid, fork/pin sessions.
V3 : Docker, auth, push GitHub.

## Liens utiles

- Backend : `C:\Users\patri\CascadeProjects\code-explorer\crates\code-explorer-mcp\` (27 tools MCP)
- HTTP transport : `crates\code-explorer-mcp\src\transport\http.rs`
- Rapport de décision : `D:\CascadeProjects\claude-et-patrice\propositions\CHAT-OPENWEBUI-2026-05-04.md`
- Convention multi-IA : `D:\CascadeProjects\claude-et-patrice\COLAB.md`

## Gotchas connus

- **Tailwind v4** : on ne peut pas utiliser `@apply` sans déclarer un layer. Pour
  réutiliser des combinaisons de classes, préfère un composant React.
- **Zustand persist + StrictMode** : 2 renders au boot — vérifier que les actions
  sont idempotentes.
- **`erasableSyntaxOnly`** : pas de `enum`, pas de parameter properties, pas
  d'`namespace`. Utiliser `const obj = {...} as const` ou union types.
