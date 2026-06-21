# Code Explorer launcher scripts

Scripts Windows pour lancer Code Explorer sans retenir les longues commandes Cargo/npm.

Voir aussi le guide complet: [INSTALLATION.md](../INSTALLATION.md).

## Lancer le client chat React

Depuis la racine du projet :

```powershell
.\start-chat-react.cmd
```

Ce lanceur :

- demarre le backend HTTP Code Explorer sur `http://127.0.0.1:3010`,
- configure `chat-ui/.env.local` avec `VITE_MCP_URL=http://127.0.0.1:3010`,
- demarre le client React sur `http://127.0.0.1:5176`,
- affiche un resume avec les URLs, le diagnostic et le LLM actif,
- ouvre le navigateur.

Le client chat indique le LLM actif, horodate les questions/reponses, rend le
Markdown, le code colore et les diagrammes Mermaid, puis exporte les conversations
en Markdown ou PDF imprimable. L'export PDF conserve les diagrammes rendus,
les blocs de code, les tables et les citations source; si Mermaid echoue, la
source du diagramme reste visible dans le document. Le bouton `Explorer` et le
raccourci `Ctrl+K` ouvrent la navigation read-only dans les
sources indexees et dans le voisinage graphe d'un symbole. En cas de `502` sur la liste des projets, le panneau d'erreur
propose un diagnostic copiable avec les commandes de reprise.

Si le backend ou le client React repond deja sur le port demande, le script le
reutilise au lieu d'ouvrir une deuxieme instance. Si le port est occupe par un
service qui ne repond pas comme Code Explorer, le script s'arrete avec un message
clair indiquant le processus qui occupe le port, au lieu de basculer
silencieusement vers un autre port.
Le lanceur nettoie aussi les anciens processus Vite Code Explorer du meme `chat-ui`
et du meme port lorsqu'ils ecoutent sur un autre host, afin d'eviter les
conflits `localhost` / `127.0.0.1`.

Variante sans backend si le serveur tourne deja :

```powershell
.\code-explorer.cmd chat -NoBackend
```

Changer les ports :

```powershell
.\code-explorer.cmd chat -BackendPort 3001 -ChatPort 5175
```

Compatibilite avec l'ancien onglet `localhost:5174` :

```powershell
.\code-explorer.cmd chat -ChatPort 5174
```

Redemarrer explicitement apres une modification backend ou UI :

```powershell
.\code-explorer.cmd chat -RestartBackend
.\code-explorer.cmd chat -RestartChat
.\code-explorer.cmd chat -RestartBackend -RestartChat
```

`-RestartBackend` n'arrete que le processus `code-explorer` qui ecoute sur le port
backend choisi. Si le port est occupe par une autre application, le script
refuse de l'arreter.

## Lancer l'application desktop

```powershell
.\start-desktop.cmd
```

Ce lanceur demarre l'UI Vite desktop sur `http://localhost:1421`, puis lance Tauri.

## ChatGPT OAuth

Configurer Code Explorer pour utiliser l'abonnement ChatGPT avec `gpt-5.5` :

```powershell
.\config-chatgpt.cmd
```

Se connecter a ChatGPT :

```powershell
.\login-chatgpt.cmd
```

Tester la connexion :

```powershell
.\test-chatgpt.cmd
```

Diagnostiquer la configuration, les ports et le login sans afficher de secret :

```powershell
.\doctor-code-explorer.cmd
```

Equivalent :

```powershell
.\code-explorer.cmd doctor
```

Le diagnostic verifie aussi que `chat-ui/.env.local` pointe vers le backend
attendu pour le port choisi et que les endpoints Explorer (`files` / `graph`)
sont disponibles. C'est utile apres des essais avec `-BackendPort` ou
`-ChatPort`, quand le client React peut demarrer correctement mais interroger
un ancien serveur.

## CLI rapide

```powershell
.\code-explorer.cmd ask -Question "Resume ce projet en 5 lignes"
.\code-explorer.cmd analyze -Repo D:\CascadeProjects\code-explorer-from-c
.\code-explorer.cmd docs -Repo D:\CascadeProjects\code-explorer-from-c
.\code-explorer.cmd docs -Repo D:\CascadeProjects\code-explorer-from-c -Enrich
.\code-explorer.cmd doctor
```

La commande `docs` ouvre automatiquement le site HTML genere. Ajoutez
`-NoBrowser` pour produire les fichiers sans ouvrir de fenetre.
Le chat integre du site HTML genere peut copier ou telecharger le transcript
Markdown, et ouvrir une version imprimable pour produire un PDF.
Pour un PDF de documentation complet depuis Markdown, utilisez aussi:

```powershell
.\code-explorer.cmd generate pdf --input D:\chemin\vers\docs
```

## Tout verifier

```powershell
.\check-code-explorer.cmd
```

Equivalent :

```powershell
.\code-explorer.cmd check
```

Cette commande relance les validations principales : `chat-ui` lint/tests/build,
UI desktop lint/build, puis `cargo fmt --check` et les tests Rust CLI/MCP/Desktop.
