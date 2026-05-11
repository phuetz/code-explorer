# Installation de GitNexus

Derniere mise a jour: 2026-05-07

Ce guide donne le chemin le plus simple pour installer, configurer et lancer GitNexus en local. Les exemples Windows utilisent PowerShell depuis la racine du depot.

Pour une installation sur une machine Ubuntu neuve, voir aussi [INSTALLATION_UBUNTU.md](INSTALLATION_UBUNTU.md).

## 1. Prerequis

| Outil | Version conseillee | Usage |
| --- | --- | --- |
| Git | 2.x+ | Cloner les depots et calculer les statistiques Git |
| Rust stable | 1.75+ | Compiler le backend Rust et la CLI |
| Visual Studio Build Tools C++ | Recent | Compiler les grammaires tree-sitter sous Windows |
| Node.js | 18+ | Lancer les interfaces React |
| npm | Version fournie avec Node | Installer les dependances frontend |
| CMake | 3.15+ optionnel | Backend KuzuDB optionnel |

Sous Windows, installez le workload "Desktop development with C++" via Visual Studio Build Tools. Apres installation, ouvrez un nouveau terminal pour que `cargo`, `node` et `npm` soient bien dans le `PATH`.

## 2. Recuperer le projet

```powershell
git clone https://github.com/phuetz/gitnexus-rs.git
cd gitnexus-rs
```

Si vous travaillez dans le workspace actuel de Patrice:

```powershell
cd D:\CascadeProjects\gitnexus-rs-from-c
```

## 3. Diagnostic initial

Lancez d'abord le diagnostic. Il verifie la configuration, les ports courants, le fichier ChatGPT OAuth et le fichier `.env.local` du chat React.

```powershell
.\gitnexus.cmd doctor
```

Equivalent:

```powershell
.\doctor-gitnexus.cmd
```

Si cette commande signale un port deja occupe, gardez l'information: vous pourrez choisir d'autres ports au lancement du chat.

## 4. Configurer ChatGPT OAuth

Pour utiliser votre abonnement ChatGPT avec le flux OAuth type Codex:

```powershell
.\config-chatgpt.cmd
.\login-chatgpt.cmd
.\test-chatgpt.cmd
```

Effet attendu:

- `config-chatgpt.cmd` ecrit `C:\Users\<vous>\.gitnexus\chat-config.json`.
- `login-chatgpt.cmd` ouvre le navigateur, demande l'authentification ChatGPT et sauvegarde les tokens dans `C:\Users\<vous>\.gitnexus\auth\openai.json`.
- `test-chatgpt.cmd` envoie une petite requete de verification au modele configure.

Le fichier `openai.json` contient des tokens personnels. Ne le copiez jamais dans le depot et ne le partagez pas.

Configuration attendue pour ChatGPT:

```json
{
  "provider": "chatgpt",
  "api_key": "",
  "base_url": "https://chatgpt.com/backend-api/codex",
  "model": "gpt-5.5",
  "max_tokens": 8192,
  "reasoning_effort": "high"
}
```

Le niveau de reflexion se regle avec `reasoning_effort`: `low`, `medium`, `high` ou `xhigh` selon le modele et le fournisseur. Pour le travail d'audit ou d'architecture, `high` est le bon defaut.

## 5. Indexer un depot

GitNexus doit analyser un projet avant que le chat, les recherches ou les graphes puissent repondre correctement.

```powershell
.\gitnexus.cmd analyze -Repo D:\chemin\vers\mon-projet
```

Exemple avec Alise:

```powershell
.\gitnexus.cmd analyze -Repo D:\CascadeProjects\Alise_v2
```

Verifier les depots indexes:

```powershell
.\gitnexus.cmd list
```

## 6. Lancer le chat React

Commande standard:

```powershell
.\gitnexus.cmd chat
```

Par defaut, le script lance:

- backend HTTP GitNexus: `http://127.0.0.1:3010`
- client React chat: `http://127.0.0.1:5176`

Si vous avez deja un onglet ou une habitude sur `5174`:

```powershell
.\gitnexus.cmd chat -ChatPort 5174
```

Si un autre service utilise le port backend ou le port React:

```powershell
.\gitnexus.cmd chat -BackendPort 3011 -ChatPort 5177
```

Pour forcer un redemarrage propre apres modification:

```powershell
.\gitnexus.cmd chat -RestartBackend -RestartChat
```

Pour lancer seulement le client React si le backend tourne deja:

```powershell
.\gitnexus.cmd chat -NoBackend
```

Le script `start-chat-react.cmd` est un raccourci equivalent au mode chat standard:

```powershell
.\start-chat-react.cmd
```

### Explorer les sources et le graphe depuis le chat

Dans le client React, le bouton `Explorer` ouvre un panneau lateral:

- `Sources`: filtrer l'arborescence, ouvrir un fichier, lire un extrait avec numeros de ligne;
- `Graphe`: chercher une classe, methode, action MVC ou service, puis afficher son voisinage;
- `Source`: depuis un noeud du graphe, revenir au fichier associe;
- bouton message: envoyer le fichier ou le noeud selectionne dans le brouillon du chat.

Raccourci utile: `Ctrl+K` ouvre la recherche rapide. Elle cherche dans les fichiers et les symboles du projet actif; un fichier ouvre l'onglet Sources, un symbole ouvre directement son voisinage Graphe.

Cette navigation utilise uniquement les depots indexes par GitNexus. Si le panneau est vide, lancez d'abord:

```powershell
.\gitnexus.cmd analyze -Repo D:\chemin\vers\mon-projet
.\gitnexus.cmd list
```

## 7. Lancer l'application desktop

```powershell
.\start-desktop.cmd
```

Ce lanceur demarre l'UI Vite desktop sur `http://localhost:1421`, puis lance l'application Tauri. Le desktop contient deja une navigation graphe/fichiers plus complete que le chat web autonome.

## 8. Generer le site de documentation HTML

Generation simple:

```powershell
.\gitnexus.cmd docs -Repo D:\chemin\vers\mon-projet
```

Generation enrichie par LLM:

```powershell
.\gitnexus.cmd docs -Repo D:\chemin\vers\mon-projet -Enrich
```

Produire sans ouvrir le navigateur:

```powershell
.\gitnexus.cmd docs -Repo D:\chemin\vers\mon-projet -NoBrowser
```

Le site genere contient la navigation, la recherche, une page **Carte du Code** facon DeepWiki (modules, dependances, points d'entree, fichiers pivots), les diagrammes Mermaid, les extraits de code, le chat de documentation et les exports Markdown/PDF des conversations.

### Pilotage DeepWiki

GitNexus sait lire un manifeste compatible DeepWiki dans le repo analyse:

```text
.devin/wiki.json
```

Si ce fichier existe, la generation ajoute une section **Wiki guide** avec le plan demande, les notes de depot, les pages ciblees et des fichiers sources probables pour chaque page. Les champs supportes sont `repo_notes`, `pages`, `title`, `purpose`, `parent` et `page_notes`.

### Exports PDF

Deux exports PDF existent:

- dans le chat React, le bouton imprimante ouvre une version imprimable de la conversation avec metadata projet/LLM/date, code lisible, tables, citations source et diagrammes Mermaid deja rendus;
- dans la CLI, `gitnexus generate pdf --input <markdown>` produit un PDF A4 via Chromium/Playwright et attend les diagrammes Mermaid, les polices et les images avant capture.

Les exports nettoient les caracteres invisibles qui peuvent perturber les moteurs PDF. Si un diagramme Mermaid ne peut pas etre rendu, GitNexus conserve la source du diagramme dans le document au lieu de produire un bloc vide.

## 9. Compiler depuis les sources

Compiler la CLI en debug:

```powershell
cargo build -p gitnexus-cli
```

Compiler la CLI en release:

```powershell
cargo build --release -p gitnexus-cli
```

Compiler le chat React autonome:

```powershell
cd chat-ui
npm install
npm run build
cd ..
```

Compiler l'UI desktop:

```powershell
cd crates\gitnexus-desktop\ui
npm install
npm run build
cd ..\..\..
```

Compiler l'application desktop Rust/Tauri:

```powershell
cargo build -p gitnexus-desktop
```

## 10. Tout verifier

Validation principale:

```powershell
.\check-gitnexus.cmd
```

Equivalent:

```powershell
.\gitnexus.cmd check
```

Cette commande lance les validations principales du chat web, de l'UI desktop et des crates Rust. Pour un changement de documentation seul, un `git diff --check` suffit souvent avant commit.

## 11. Commandes utiles au quotidien

```powershell
.\gitnexus.cmd doctor
.\gitnexus.cmd list
.\gitnexus.cmd analyze -Repo D:\chemin\vers\mon-projet
.\gitnexus.cmd ask -Question "Resume l'architecture du projet en 5 points"
.\gitnexus.cmd docs -Repo D:\chemin\vers\mon-projet
.\gitnexus.cmd chat -RestartBackend -RestartChat
```

## 12. Depannage rapide

### Page noire ou erreur React "Maximum update depth exceeded"

Reprenez avec un redemarrage complet:

```powershell
.\gitnexus.cmd chat -RestartBackend -RestartChat
```

Puis rechargez l'onglet. Si le probleme persiste, ouvrez la console navigateur et notez le composant cite dans l'erreur.

### `list_repos failed: 502`

Le client React parle au backend, mais le backend ne repond pas correctement ou n'est pas celui attendu.

```powershell
.\gitnexus.cmd doctor
.\gitnexus.cmd chat -RestartBackend -RestartChat
```

Verifiez aussi que `chat-ui\.env.local` pointe vers le bon `VITE_MCP_URL`.

### Conflit de port

Utilisez des ports explicites:

```powershell
.\gitnexus.cmd chat -BackendPort 3011 -ChatPort 5177
```

Evitez de supposer que le port `3000` est libre: beaucoup d'applications React ou Node l'utilisent deja.

### Boucle OAuth ChatGPT ou double demande d'authentification

Fermez les anciens onglets d'authentification, puis relancez:

```powershell
.\login-chatgpt.cmd
.\test-chatgpt.cmd
```

Si la session reste incoherente, supprimez le fichier token local apres avoir verifie que vous ne le partagez pas:

```powershell
Remove-Item "$env:USERPROFILE\.gitnexus\auth\openai.json"
.\login-chatgpt.cmd
```

### Le chat ne connait pas mon projet

Indexez ou reindexez le depot:

```powershell
.\gitnexus.cmd analyze -Repo D:\chemin\vers\mon-projet
.\gitnexus.cmd list
```

Ensuite selectionnez le projet dans le menu du chat React.

## 13. Ordre conseille pour une premiere utilisation

```powershell
cd D:\CascadeProjects\gitnexus-rs-from-c
.\gitnexus.cmd doctor
.\config-chatgpt.cmd
.\login-chatgpt.cmd
.\test-chatgpt.cmd
.\gitnexus.cmd analyze -Repo D:\CascadeProjects\Alise_v2
.\gitnexus.cmd chat
```

Ensuite ouvrez le chat, choisissez le projet indexe, puis posez une question avec un diagramme attendu, par exemple:

```text
Trace le flux d'execution complet d'une creation de courrier en masse avec un diagramme Mermaid flowchart TD, puis liste les sources.
```
