# Code Explorer Chat

Client React de Code Explorer : chat, navigation dans les sources et le graphe,
configuration LLM et export des conversations. Il utilise le backend HTTP réel ;
il faut indexer un projet et démarrer ce backend avant d'interroger le code.

Prérequis : Node.js 22.12+ et un binaire `code-explorer` sur le PATH.

Depuis le projet à explorer, dans un premier terminal :

```bash
code-explorer analyze .
code-explorer serve --port 3010
```

Depuis ce dossier `chat-ui`, dans un second terminal :

```bash
npm ci
npm run dev -- --host 127.0.0.1 --port 5176 --strictPort
```

Ouvrir `http://127.0.0.1:5176`. Vite relaie `/api`, `/health` et `/mcp` vers
`http://127.0.0.1:3010`. Pour un autre backend, définir `VITE_MCP_URL` dans
`.env.local` avant de démarrer Vite. Une configuration LLM est nécessaire pour
les réponses du chat ; voir [le guide d'installation](../INSTALLATION_UBUNTU.md).

```bash
npm run build
npm run preview
npm run lint
npm run test
```

Le client utilise Vite 8, React 19, TypeScript et Tailwind CSS 4.
Sa licence figure dans [LICENSE](LICENSE) ; celle du backend figure dans
[la licence du dépôt](../LICENSE).
