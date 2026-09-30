# Code Explorer Desktop UI

Frontend React de l'application Tauri Code Explorer. Prérequis : Node.js 22.12+.
Les commandes suivantes se lancent depuis ce dossier :

```bash
npm ci
npm run build
npm run lint
npm run test:config
npx playwright install chromium
npm run test:e2e
```

`test:config` exécute le hook de compilation déclaré dans `tauri.conf.json` et
vérifie l'icône requise par AppImage. Les tests E2E utilisent des réponses Tauri
simulées ; ils ne remplacent pas une vérification de l'application native.

Pour tester également le HTML généré, passer le chemin du fichier :

```bash
CODE_EXPLORER_HTML_PATH=/chemin/vers/projet/.codeexplorer/docs/index.html npm run test:e2e
```

Pour démarrer l'application native, depuis `crates/code-explorer-desktop` :

```bash
cargo tauri dev
```

Le hook Tauri démarre Vite sur le port 1421. `npm run dev` seul utilise le port
1420 et présente uniquement le frontend, sans les commandes natives Tauri.
Voir [l'installation Ubuntu](../../../INSTALLATION_UBUNTU.md) pour les dépendances
système et la [construction des installateurs](../../../build-release.sh).
