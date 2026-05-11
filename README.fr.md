# GitNexus

Intelligence de code basée sur un graphe de connaissances pour agents IA. GitNexus construit un graphe à partir de votre code source et l'expose via [MCP](https://modelcontextprotocol.io/) (Model Context Protocol) pour l'analyse de code assistée par IA.

Écrit en Rust. Supporte 14 langages de programmation. Livré avec une application desktop et un générateur de documentation HTML.

[English version](README.md)
[Guide d'installation](INSTALLATION.md)
[Installation Ubuntu](INSTALLATION_UBUNTU.md)
[Feuille de route de modernisation](MODERNIZATION.md)
[Plan navigation React sources/graphe](REACT_SOURCE_GRAPH_NAVIGATION.md)

## Pourquoi GitNexus ? (vs un assistant IA seul)

Les assistants IA comme Claude Code, Cursor ou Copilot lisent les fichiers **un par un, à la demande**. Pour un gros projet (800+ fichiers), ils doivent lire des dizaines de fichiers pour comprendre une seule chaîne d'appels, repartent de zéro à chaque conversation, et remplissent leur fenêtre de contexte avec du code brut.

GitNexus résout ce problème en pré-indexant **l'intégralité** de votre codebase dans un graphe de connaissances.

| | Assistant IA seul | Assistant IA + GitNexus |
|---|---|---|
| **Relations** | Doit lire chaque fichier pour découvrir qui appelle quoi | Graphe pré-calculé : appelants, appelés, hiérarchie instantanés |
| **Échelle** | ~50 fichiers dans le contexte max | 800+ fichiers indexés, interrogeables en 1 commande |
| **Persistance** | Repart de zéro à chaque conversation | Le graphe persiste sur disque, toujours disponible |
| **Efficacité contexte** | Lire 50 fichiers = contexte plein, plus de place pour réfléchir | Retourne uniquement les relations pertinentes, le contexte reste libre |
| **Analyse d'impact** | Impossible sans lire tout le projet | `gitnexus impact handleRequest` → chaîne complète en 1 seconde |
| **Analytics Git** | Devrait parser `git log` à chaque fois | Hotspots, couplage, ownership pré-calculés |
| **Documentation** | Peut écrire 1-2 pages par conversation | Génère 40+ pages HTML avec diagrammes, navigation, recherche |
| **Frameworks legacy** | Ne comprend pas Telerik 2011, EDMX, jQuery→Controller | Parsers spécialisés ASP.NET MVC, EF6, Telerik, AJAX |
| **Multi-agents** | Limité à un seul outil | Serveur MCP → Claude, Cursor, VS Code, tout agent |
| **Hors ligne** | Besoin d'API | Le graphe fonctionne 100% local, sans internet |

**En résumé :** un assistant IA lit du code. GitNexus **comprend** la structure de tout le codebase. Ensemble, l'IA a un "cerveau" qui connaît déjà toutes les relations — au lieu de lire 50 fichiers pour trouver ce qui appelle `PaymentService`, une seule commande donne la réponse instantanément, sans consommer de contexte.

C'est la différence entre demander à quelqu'un de **lire un livre** et lui donner **l'index et le sommaire**.

## Fonctionnalités

- **Graphe de connaissances** — Parse le code source en un graphe riche de symboles (fonctions, classes, modules, imports, appels, héritage) avec 50+ types de nœuds et relations typées
- **14 Langages** — JavaScript, TypeScript, Python, Java, C, C++, C#, Go, Rust, Ruby, PHP, Kotlin, Swift, Razor via tree-sitter
- **Support avancé ASP.NET MVC 5** — Controllers, actions, vues Razor, Entity Framework 6 EDMX, grilles Telerik/Kendo, mapping jQuery/AJAX, détection couche service/repository (voir ci-dessous)
- **Générateur de documentation HTML** — Site "DeepWiki" professionnel avec recherche plein texte (Ctrl+K), icônes Lucide, sidebar dynamique, coloration syntaxique, boutons copier, estimation du temps de lecture et liens de références croisées automatiques entre les symboles.
- **UX Interactive** — Application mono-page (SPA) avec support de l'historique du navigateur, fil d'Ariane, navigation Précédent/Suivant, scroll spy TOC, design responsive et diagrammes Mermaid interactifs (zoom/plein écran).
- **Chat de documentation** — Les sites HTML générés incluent un assistant connecté au graphe, avec rendu Markdown, code colorisé, diagrammes Mermaid et exports de conversation en Markdown ou PDF imprimable.
- **Documentation de Processus Métier** — Génération automatique de rapports fonctionnels de haut niveau (B1-B5) pour les flux complexes (Cycle de paiement, Moteur de calcul, Génération de documents), incluant des diagrammes de séquence et de flux Mermaid détaillés.
- **Enrichissement LLM** — Mode `--enrich` optionnel qui augmente la documentation avec de la prose LLM grounded, des payloads JSON structurés, des citations avec provenance et une validation anti-hallucination.
- **Interroger le code** — Commande CLI `gitnexus ask "question"` pour du Q&A basé sur le graphe avec réponses en streaming.
- **Feedback par Page** — Widget de feedback intégré sur chaque page pour suivre la qualité et l'utilité du contenu.
- **Application Desktop** — Application Tauri v2 avec visualisation interactive du graphe, vue treemap, chat intelligent et palette de commandes (Ctrl+K)
- **Chat Intelligent** — Q&A de code assisté par IA avec réponses en streaming, analyse de complexité des requêtes, plans de recherche multi-étapes et mode recherche approfondie. Supporte Ollama, OpenAI, Anthropic, OpenRouter et Gemini (avec mode raisonnement)
- **Serveur MCP** — 27 outils accessibles à tout agent IA compatible MCP (Claude, Cursor, VS Code, etc.)
- **Skill Claude Code** — Skill `/gitnexus` intégré qui permet à Claude d'interroger le graphe de connaissances pendant votre conversation, avec invocation automatique sur les questions en langage naturel
- **Rapport de Santé du Code** — Commande `gitnexus report` combinant hotspots, couplage temporel, ownership et métriques du graphe en un score de santé (A-E)
- **Recherche Hybride** — Recherche lexicale BM25 + embeddings sémantiques ONNX optionnels, fusionnés par Reciprocal Rank Fusion. Reranker LLM optionnel pour réordonner les résultats en post-traitement, avec repli automatique si le modèle est indisponible.
- **Analyse d'Impact** — Trace les appelants amont, les appelés aval et l'impact transitif de tout symbole
- **Modes Interactifs** — Shell REPL, dashboard TUI, surveillance de fichiers avec réindexation automatique
- **Stockage Modulaire** — Backend en mémoire (par défaut) ou base de données graphe KuzuDB

## Support ASP.NET MVC 5 / Legacy .NET

GitNexus offre un support approfondi des projets ASP.NET MVC 5 legacy, idéal pour documenter et comprendre des applications d'entreprise complexes.

### Ce qu'il détecte

| Fonctionnalité | Détection |
|----------------|-----------|
| **Controllers & Actions** | Héritage de classes, `[HttpGet/Post]`, `[GridAction]`, templates de routes, signatures de paramètres |
| **Vues Razor** (.cshtml) | `@model`, `@layout`, `@Html.Partial`, `@Html.RenderAction`, `@Html.BeginForm` |
| **Entity Framework 6** | DbContext, DbSet, entités EDMX, associations, propriétés de navigation, héritage (TPH/TPT) |
| **Telerik / Kendo UI** | `Html.Telerik().Grid<T>()`, `Html.Kendo().Grid<T>()`, bindings DataSource (`.Select()`, `.Read()`), colonnes de grille, `ClientEvents`, `DatePickerFor`, `DropDownListFor` |
| **jQuery / AJAX** | `$.ajax()`, `$.getJSON()`, `$.post()`, `$.get()`, `$.load()`, `fetch()`, `@Url.Action()` — liés aux actions des controllers |
| **Couche Service** | Classes `*Service`, `*Repository`, `*Manager`, `*Provider`, `*UnitOfWork` avec détection d'interfaces |
| **Injection de Dépendances** | Autofac (`RegisterType<T>().As<I>()`), Unity, Ninject, MS DI |
| **Attributs Personnalisés** | `[AuthorizeADAttribute]`, `[VerifActionFilter]`, tout `[*Attribute]`, `[*Filter]`, `[*Action]` |
| **Services Externes** | Détection de clients WebAPI (`new CMCASClient(httpClient)`), références WCF, traçage d'appels HTTP |
| **Traçabilité StackLogger** | Analyse de couverture — identifie les méthodes instrumentées avec `BeginMethodScope()` |
| **Controllers de Base** | Héritage de controllers personnalisés (`RootController` → `Controller`) |
| **Web.config** | Détection des fichiers de configuration |

### Documentation générée

La commande `generate html` produit un site de documentation HTML style DeepWiki :

```bash
gitnexus analyze D:\chemin\vers\mon-projet-mvc5
gitnexus generate --path D:\chemin\vers\mon-projet-mvc5 html
# Ouvrir .gitnexus/docs/index.html dans le navigateur
```

Le site HTML inclut :
- **Vue d'ensemble** avec stack technique, structure des projets et métriques interactives
- **Diagramme d'architecture** (Mermaid) montrant les couches Présentation → Logique Métier → Accès aux Données
- **Processus Métier** (B1-B5) avec flux de haut niveau pour les Courriers, les Paiements et le Calcul des Barèmes
- **Pages par controller** avec signatures des actions, paramètres (liés au modèle de données), appelants et code source
- **Pages modèle de données** avec diagrammes de relations par entité et par domaine métier
- **Guide fonctionnel** avec descriptions métier en français, niveaux de criticité et diagrammes de flux Mermaid
- **Éléments Interactifs** : Zoom sur les diagrammes Mermaid, fichiers sources cliquables avec copie du chemin, et support de l'historique de navigation
- **Assistant intégré** : chat sur la documentation générée avec exports Markdown/PDF, horodatage et rendu des diagrammes Mermaid
- **Thème sombre/clair** avec recherche dans la sidebar, fil d'Ariane et navigation Précédent/Suivant

## Démarrage Rapide

### Prérequis

| Dépendance | Version | Nécessaire pour | Installation |
|-----------|---------|----------------|--------------|
| **Rust** | 1.75+ | Tout | [rustup.rs](https://rustup.rs/) |
| **Compilateur C/C++** | - | Grammaires tree-sitter | Windows: Visual Studio Build Tools. Linux: `apt install build-essential`. macOS: `xcode-select --install` |
| **Node.js** | 18+ | Frontend de l'app desktop | [nodejs.org](https://nodejs.org/) |
| **git** | 2.0+ | Analytics git (hotspots, couplage, ownership) | Déjà installé sur la plupart des systèmes |
| **CMake** | 3.15+ | Backend KuzuDB (optionnel) | Windows: `winget install cmake`. Linux: `apt install cmake` |

### Installation & Compilation

```bash
# 1. Cloner
git clone https://github.com/phuetz/gitnexus-rs.git
cd gitnexus-rs

# 2. Compiler la CLI (mode release, ~35 Mo)
cargo build --release -p gitnexus-cli

# Le binaire se trouve à :
# Windows : target\release\gitnexus.exe
# Linux/macOS : target/release/gitnexus

# 3. (Optionnel) Compiler l'Application Desktop
cd crates/gitnexus-desktop/ui && npm install && npm run build && cd ../../..
cargo build -p gitnexus-desktop --release
```

Des scripts de build sont aussi fournis :

```bash
# Windows
build-release.bat           # Compiler CLI + Desktop
build-release.bat cli       # CLI uniquement
build-release.bat desktop   # Desktop uniquement

# Linux/macOS
./build-release.sh          # Compiler CLI + Desktop
./build-release.sh cli      # CLI uniquement
```

Scripts Windows pour travailler en local rapidement :

```powershell
.\config-chatgpt.cmd        # Configure ChatGPT OAuth avec gpt-5.5
.\login-chatgpt.cmd         # Connexion OAuth ChatGPT
.\gitnexus.cmd chat         # Backend 3010 + chat React 5176
.\gitnexus.cmd chat -ChatPort 5174  # Compatibilite avec un ancien onglet
.\start-desktop.cmd         # UI desktop + Tauri
.\check-gitnexus.cmd        # Lint/tests/build principaux
```

Le chat React affiche le LLM/modèle/niveau de raisonnement actif, horodate les messages, rend Mermaid et le code colorisé, exporte les conversations en Markdown/PDF, et propose un diagnostic copiable quand le backend ou la liste des projets est inaccessible.

### Compilation avec fonctionnalités optionnelles

```bash
# Avec le backend KuzuDB (pour les très gros repos, nécessite CMake)
cargo build --release -p gitnexus-cli --features gitnexus-cli/kuzu-backend

# Avec la recherche sémantique ONNX (BM25 + embeddings hybrides)
cargo build --release -p gitnexus-cli --features gitnexus-search/embeddings

# Avec le reranker LLM (post-traitement via API OpenAI-compatible)
cargo build --release -p gitnexus-cli --features gitnexus-search/reranker-llm

# Avec tout (KuzuDB + embeddings + reranker)
cargo build --release -p gitnexus-cli --features gitnexus-cli/kuzu-backend,gitnexus-search/embeddings,gitnexus-search/reranker-llm
```

> **Note :** la build par défaut de `gitnexus-cli` active déjà `embeddings` et `reranker-llm`. Les commandes ci-dessus sont des activations explicites pour les crates qui consomment la lib à la carte.

### Configuration LLM (pour `ask` et `--enrich`)

Créer `~/.gitnexus/chat-config.json` :

```json
{
  "provider": "gemini",
  "api_key": "VOTRE_CLE_API",
  "base_url": "https://generativelanguage.googleapis.com/v1beta/openai",
  "model": "gemini-2.5-flash",
  "max_tokens": 8192,
  "reasoning_effort": "high",

  "big_context_model": "gemini-2.5-pro",
  "big_context_threshold_bytes": 40000,
  "big_context_max_tokens": 131072
}
```

Fournisseurs supportés : **Gemini**, **OpenAI**, **Anthropic**, **OpenRouter**, **Ollama** (local, pas de clé API nécessaire) et **ChatGPT** via le flux OAuth de type Codex pour `gitnexus ask` / le chat `gitnexus serve`. `--enrich` reste sur les fournisseurs API-key compatibles OpenAI.

Pour utiliser votre abonnement ChatGPT au lieu d'une clé API avec `gitnexus ask` et le chat web lancé par `gitnexus serve` :

```bash
gitnexus login
```

Puis configurer `~/.gitnexus/chat-config.json` avec le fournisseur ChatGPT. `api_key` reste volontairement vide : le jeton est chargé depuis la connexion OAuth créée ci-dessus.

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

Les trois champs optionnels `big_context_*` routent les pages volumineuses (≥ `big_context_threshold_bytes` de markdown brut, défaut 40 Ko) vers un modèle long-contexte pour échapper au plafond de 65K tokens en sortie de Gemini 2.5 Flash qui provoque les troncatures `finish_reason: length`. Tous les appels LLM pour cette page (mode sectionné, monolithique, fallback freeform, passe de revue) utilisent le modèle de substitution. Laisser ces champs vides conserve le comportement mono-modèle historique.

#### Tuner l'enrichissement sans recompiler

Les constantes qui imposaient auparavant un `cargo build --release` pour être ajustées sont désormais surchargeables via un bloc `enrichment` optionnel. Tous les champs sont optionnels — omettre un champ (ou le bloc entier) conserve le comportement historique :

```json
{
  "enrichment": {
    "sectionMaxTokens": 4096,
    "monolithicMaxTokensFloor": 65536,
    "sectionContentSnippetBytes": 3000,
    "profiles": {
      "fast":    { "maxEvidence": 10, "maxRetries": 8, "timeoutSecs": 60,  "minGapMs": 500, "useJsonSchema": false },
      "quality": { "maxEvidence": 20, "maxRetries": 1, "timeoutSecs": 180, "minGapMs": 0,   "useJsonSchema": true },
      "strict":  { "maxEvidence": 30, "maxRetries": 2, "timeoutSecs": 300, "minGapMs": 0,   "useJsonSchema": true }
    }
  }
}
```

Cas courants : baisser `sectionMaxTokens` pour économiser des tokens sur les petites pages ; augmenter `quality.timeoutSecs` si Gemini ralentit aux heures de pointe européennes ; augmenter `fast.maxEvidence` pour des brouillons plus riches. Les valeurs ci-dessus sont les défauts codés en dur.

Valider votre configuration :

```bash
gitnexus config test
```

### Lancer l'Application Desktop (dev mode avec rechargement à chaud)

Ou lancer en mode développement avec rechargement à chaud :

```bash
cd crates/gitnexus-desktop
cargo tauri dev
```

## Utilisation de la CLI

### Analyser un projet

```bash
# Indexer le répertoire courant
gitnexus analyze

# Indexer un chemin spécifique (ex: un projet ASP.NET MVC legacy)
gitnexus analyze D:\chemin\vers\projet

# Forcer la réindexation (réinitialise le graphe)
gitnexus analyze D:\chemin\vers\projet --force
```

Cela crée un répertoire `.gitnexus/` contenant le graphe de connaissances sérialisé.

### Générer la documentation

```bash
# Générer le site HTML (recommandé)
gitnexus generate --path D:\chemin\vers\projet html
# → Ouvrir .gitnexus/docs/index.html dans le navigateur

# Générer avec enrichissement LLM (nécessite un LLM configuré)
gitnexus generate --path D:\chemin\vers\projet html --enrich
gitnexus generate --path D:\chemin\vers\projet html --enrich --enrich-profile strict
gitnexus generate --path D:\chemin\vers\projet html --enrich --enrich-lang en

# Tout générer (AGENTS.md, wiki, skills, docs, DOCX, HTML)
gitnexus generate --path D:\chemin\vers\projet all

# Générer des formats spécifiques
gitnexus generate --path D:\chemin\vers\projet docs     # Pages Markdown
gitnexus generate --path D:\chemin\vers\projet docx     # Document Word (header + footer + brand)
gitnexus generate --path D:\chemin\vers\projet pdf      # PDF (basé Puppeteer, depuis l'HTML)
gitnexus generate --path D:\chemin\vers\projet context  # AGENTS.md uniquement
gitnexus generate --path D:\chemin\vers\projet wiki     # Pages wiki
gitnexus generate --path D:\chemin\vers\projet skills   # Fichiers skills
gitnexus generate --path D:\chemin\vers\projet inject   # Re-injecter les fragments LLM sans tout regénérer
```

### Word DOCX — personnalisation client

Les `.docx` générés embarquent désormais un en-tête configurable (nom client + titre du document), un pied de page (texte de marque + pagination automatique `Page X / Y`), et les métadonnées Word `Fichier > Propriétés`. Pour surcharger les défauts, créer `~/.gitnexus/brand.json` :

```json
{
  "client_name": "CCAS Alise",
  "company_name": "agile-up.com",
  "footer_text": "agile-up.com — Confidentiel — Ne pas diffuser",
  "document_title": "Documentation Technique et Fonctionnelle"
}
```

Un `brand.json` absent ou mal formé bascule silencieusement vers les défauts `agile-up.com` — le binaire reste utilisable sans configuration. Surcharger l'emplacement du fichier via `$GITNEXUS_BRAND_FILE`.

Les diagrammes Mermaid présents dans le markdown source sont rendus en PNG via [Kroki](https://kroki.io) et embarqués inline dans le `.docx`. Définir `GITNEXUS_MERMAID_PLACEHOLDER=1` pour conserver le fallback texte historique (sans appel réseau), ou `GITNEXUS_KROKI_URL=<url>` pour pointer sur une instance Kroki self-hostée.

### Validation pré-livraison (`validate-docs`)

Avant d'envoyer un document Word/HTML à un client, lancer le linter pour repérer ce qui pourrait nous embarrasser :

```bash
gitnexus validate-docs --repo D:\chemin\vers\projet
gitnexus validate-docs --repo D:\chemin\vers\projet --json   # rapport JSON
```

Cinq vérifications à deux niveaux de sévérité :

- **ROUGE — bloque la livraison** : `TODO` / `TBD` / `FIXME` / `XXX` résiduels, anchors `<!-- GNX:* -->` non remplies (signe que l'enrichissement LLM a échoué silencieusement), liens markdown vers fichiers inexistants.
- **JAUNE — à corriger** : sections H1/H2 contenant moins de 50 mots, pages Service / Controller sans en-tête `§4 Algorithmes` (méthodo Alise v1.1).

Le rapport est aussi écrit dans `<docs_dir>/_meta/validation.json` pour intégration CI / scripts. **Sortie avec code 2 si une issue ROUGE est trouvée** (sinon 0) — à câbler en gate fail-fast dans votre pipeline CI.

### Interroger le code

```bash
# Poser une question en utilisant le graphe + LLM (réponse en streaming)
gitnexus ask "comment fonctionne le calcul des barèmes ?"
gitnexus ask "quels controllers appellent le WebAPI Erable ?" --path D:\taf\Alise_v2
```

### Rechercher & Explorer

```bash
# Recherche en langage naturel (BM25 par défaut)
gitnexus query "middleware d'authentification"

# Recherche hybride : BM25 + embeddings sémantiques fusionnés via Reciprocal Rank Fusion
gitnexus query "middleware d'authentification" --hybrid

# Ajouter le reranker LLM par-dessus (BM25 ou hybride)
gitnexus query "middleware d'authentification" --hybrid --rerank

# Contexte 360° d'un symbole (appelants, appelés, imports, hiérarchie)
gitnexus context UserService

# Analyse de rayon d'impact
gitnexus impact handleRequest --direction both

# Requête Cypher brute
gitnexus cypher "MATCH (n:Function) RETURN n.name LIMIT 10"
```

### Workflow recherche sémantique

Pour activer `--hybrid`, il faut d'abord générer les embeddings du graphe indexé.
Le modèle par défaut est `Xenova/all-MiniLM-L6-v2` (384d, ~90 Mo), adapté à
l'anglais et à la plupart des contenus en alphabet latin. Pour les corpus
français ou multilingues, préférer BGE-M3 ou Qwen3-Embedding (option `--model`).

```bash
# 1. Indexer le code comme d'habitude
gitnexus analyze D:\chemin\vers\projet

# 2. Générer les embeddings (écrit .gitnexus/embeddings.bin + embeddings.meta.json)
gitnexus embed --repo D:\chemin\vers\projet --model ~/.gitnexus/models/all-MiniLM-L6-v2/model.onnx
gitnexus embed --repo D:\chemin\vers\projet --model ~/.gitnexus/models/bge-m3/model.onnx
gitnexus embed --repo D:\chemin\vers\projet --model ~/.gitnexus/models/all-MiniLM-L6-v2/model.onnx --batch 16

# 3. Rechercher en hybride ; --rerank ajoute le reranker LLM
gitnexus query "où est gérée l'annulation du chat ?" --hybrid --repo D:\chemin\vers\projet
gitnexus query "où est gérée l'annulation du chat ?" --hybrid --rerank --repo D:\chemin\vers\projet
```

Le reranker LLM réutilise `~/.gitnexus/chat-config.json` et bascule automatiquement
sur la liste de résultats non rerankée si le modèle ne répond pas (erreur réseau,
réponse tronquée, JSON malformé) — la recherche reste utilisable même quand
l'étape de reranking échoue.

### Modes interactifs

```bash
gitnexus shell         # REPL interactif avec auto-complétion
gitnexus dashboard     # Dashboard TUI avec navigation dans le graphe
gitnexus watch         # Surveillance & réindexation automatique
```

### Serveur MCP (pour agents IA)

```bash
# Transport stdio (pour Claude, Cursor, VS Code, etc.)
gitnexus mcp

# Configuration automatique MCP dans votre éditeur
gitnexus setup

# Serveur HTTP
gitnexus serve         # Port 3010 par défaut
```

`gitnexus serve` écoute par défaut sur `127.0.0.1`. Si vous le liez à une
interface réseau comme `0.0.0.0`, définissez d'abord `GITNEXUS_HTTP_TOKEN` ;
le serveur refuse les binds non-loopback sans ce jeton bearer.

### Autres commandes

```bash
gitnexus list          # Lister les dépôts indexés avec statistiques
gitnexus status        # Afficher le statut de l'index du dépôt courant
gitnexus clean         # Supprimer l'index
gitnexus clean --all   # Supprimer tous les dépôts indexés
gitnexus report        # Rapport combiné de santé du code (hotspots + couplage + ownership)
gitnexus report --json # Idem, en JSON
```

### Exemple complet (projet ASP.NET MVC)

```bash
# 1. Compiler la CLI
cargo build --release -p gitnexus-cli

# 2. Analyser le projet
.\target\release\gitnexus.exe analyze D:\taf\MonAppLegacy

# 3. Générer la documentation HTML
.\target\release\gitnexus.exe generate --path D:\taf\MonAppLegacy html

# 4. Ouvrir dans le navigateur
start D:\taf\MonAppLegacy\.gitnexus\docs\index.html

# 5. Ou lancer l'application desktop pour une exploration interactive
.\target\release\gitnexus-desktop.exe
```

## Application Desktop

L'application desktop GitNexus est une application Tauri v2 avec un frontend React 19. Elle fournit une interface visuelle pour explorer le graphe de connaissances de votre code et un système de chat intelligent pour l'analyse de code assistée par IA.

### Explorateur de Graphe

Visualisation interactive du graphe propulsée par Cytoscape.js avec trois niveaux de zoom (package, module, symbole), plusieurs algorithmes de disposition et navigation au clic. Sélectionnez n'importe quel nœud pour voir son contexte complet : appelants, appelés, imports, exports et chaîne d'héritage.

### Chat Intelligent

Le système de chat est la fonctionnalité principale de l'application desktop. Il va au-delà du simple Q&A en analysant la complexité des requêtes et en exécutant des plans de recherche multi-étapes quand nécessaire.

**Analyse de Complexité** — Chaque question est classifiée en Simple (recherche directe), Moyenne (2-3 opérations), ou Complexe (DAG multi-étapes). Le système détecte les intentions en français et en anglais.

**Plans de Recherche Multi-Étapes** — Pour les requêtes moyennes et complexes, le planificateur génère un DAG de recherche avec suivi de dépendances.

**Mode Recherche Approfondie** (Ctrl+Shift+D) — Force une analyse multi-étapes complète quelle que soit la complexité de la requête.

**Filtrage Style IDE** — Ciblez vos questions sur des parties spécifiques du code :
- **Sélecteur de Fichiers** (Ctrl+P) — Recherche floue de fichiers
- **Sélecteur de Symboles** (Ctrl+Shift+O) — Recherche de symboles par type
- **Sélecteur de Modules** — Sélection par communautés

### Raccourcis Clavier

| Raccourci | Action |
|-----------|--------|
| Ctrl+K | Palette de commandes |
| Ctrl+B | Afficher/masquer la sidebar |
| Ctrl+1-5 | Changer d'onglet (Dépôts, Fichiers, Graphe, Impact, Docs) |
| Ctrl+\\ | Fermer le panneau de détails |
| Ctrl+Shift+D | Basculer le mode recherche approfondie |
| F | Ajuster le graphe à l'écran |
| L | Changer d'algorithme de disposition |
| 1 / 2 / 3 | Changer de niveau de zoom (package / module / symbole) |
| Escape | Fermer les modales, désélectionner |

## Outils MCP

En mode serveur MCP, GitNexus expose ces outils :

| Outil | Description |
|-------|-------------|
| `list_repos` | Lister les dépôts indexés avec statistiques |
| `query` | Recherche en langage naturel dans le graphe |
| `context` | Vue 360° d'un symbole : appelants, appelés, imports, exports, hiérarchie |
| `impact` | Analyse de rayon d'impact — amont, aval ou les deux |
| `detect_changes` | Analyser les changements non committés et leur impact |
| `rename` | Trouver toutes les références à mettre à jour pour un renommage |
| `cypher` | Exécuter une requête Cypher en lecture seule |

## Intégration IA : Trois façons d'utiliser GitNexus avec l'IA

GitNexus propose trois approches distinctes pour l'intelligence de code assistée par IA, chacune avec ses avantages :

### 1. Skill Claude Code (`/gitnexus`) -- Recommandé

Un [skill Claude Code](https://docs.anthropic.com/en/docs/claude-code) intégré qui permet à Claude d'interroger directement le graphe de connaissances pendant votre conversation.

```bash
# Tapez simplement dans Claude Code :
/gitnexus query "middleware d'authentification"
/gitnexus impact UserService --direction upstream
/gitnexus report --path D:\taf\MonProjet

# Ou posez la question naturellement — Claude invoque le skill automatiquement :
"Qu'est-ce qui appelle le PaymentService ?"  # → Claude lance gitnexus impact PaymentService
```

Le skill est défini dans `.claude/skills/gitnexus/SKILL.md` et fonctionne directement pour quiconque clone le dépôt. Une version personnelle (globale) peut être installée dans `~/.claude/skills/gitnexus/SKILL.md` pour l'utiliser dans tous vos projets.

### 2. Serveur MCP (pour tout agent IA)

Un serveur [Model Context Protocol](https://modelcontextprotocol.io/) standard exposant 27 outils. Compatible avec Claude Desktop, Cursor, VS Code Copilot, et tout agent MCP.

```bash
gitnexus mcp          # transport stdio
gitnexus serve        # transport HTTP (port 3010)
gitnexus setup        # Configuration automatique dans votre éditeur
```

### 3. API LLM (`--enrich` et `ask`)

Appels LLM directs via API compatible OpenAI pour l'enrichissement de la documentation et le Q&A sur le code. Nécessite `~/.gitnexus/chat-config.json`.

```bash
gitnexus ask "comment fonctionne la validation des paiements ?" --path D:\taf\MonProjet
gitnexus generate html --path D:\taf\MonProjet --enrich
```

### Comparaison

| | Skill Claude Code | Serveur MCP | API LLM |
|---|---|---|---|
| **Fonctionnement** | Claude lit le graphe directement via la CLI | L'agent IA appelle des outils via JSON-RPC | GitNexus appelle un LLM externe |
| **Modèle IA** | Claude (votre session en cours) | Tout agent compatible MCP | Gemini, OpenAI, Anthropic, Ollama |
| **Configuration** | Zéro (le skill est dans le dépôt) | `gitnexus setup` | Fichier config + clé API |
| **Latence** | Faible (CLI locale) | Faible (serveur local) | Plus élevée (aller-retour API) |
| **Coût** | Inclus dans Claude Code | Inclus dans votre agent | Coût par token API |
| **Idéal pour** | Exploration interactive, workflow dev | Intégration IDE, multi-agents | Enrichissement de doc, Q&A en batch |
| **Contexte** | Conversation complète + graphe | Par requête (scope outil) | Contexte graphe uniquement |

**Quand utiliser quoi :**
- **Skill Claude Code** : Vous travaillez dans Claude Code et voulez explorer le code interactivement. Claude comprend l'historique de votre conversation ET le graphe — idéal pour les questions complexes.
- **Serveur MCP** : Vous utilisez Cursor, VS Code, ou un autre éditeur compatible MCP. Le graphe est toujours disponible comme outil.
- **API LLM** : Vous voulez enrichir la documentation en batch ou avez besoin d'une commande Q&A autonome sans agent IA.

## Langages Supportés

| Langage | Extensions |
|---------|------------|
| JavaScript | `.js` `.jsx` `.mjs` `.cjs` |
| TypeScript | `.ts` `.tsx` `.mts` `.cts` |
| Python | `.py` `.pyi` |
| Java | `.java` |
| C | `.c` `.h` |
| C++ | `.cpp` `.hpp` `.cc` `.hh` `.cxx` `.hxx` |
| C# | `.cs` `.cshtml` `.edmx` `.config` |
| Go | `.go` |
| Rust | `.rs` |
| Ruby | `.rb` |
| PHP | `.php` |
| Kotlin | `.kt` `.kts` |
| Swift | `.swift` |
| Razor | `.cshtml` `.razor` |

## Origine & Crédits

GitNexus-RS est une implémentation haute performance en Rust et une extension du projet original **[GitNexus](https://github.com/abhigyanpatwari/GitNexus)** créé par [Abhigyan Patwari](https://github.com/abhigyanpatwari).

Alors que l'implémentation originale est principalement en TypeScript, cette version Rust se concentre sur :
- **La Performance** : Indexation parallèle ultra-rapide de grands dépôts via Rayon et Tree-sitter.
- **L'expérience Desktop Native** : Une application Tauri v2 avec visualisation interactive du graphe intégrée.
- **L'Enrichissement Entreprise** : Parsers spécialisés pour les stacks legacy (ASP.NET MVC 5, EF6, Telerik).
- **Le stockage Graphe Embarqué** : Intégration étroite avec KuzuDB pour un stockage persistant à faible consommation mémoire.

Nous sommes profondément reconnaissants pour la vision et les fondations architecturales posées par le projet [GitNexus](https://github.com/abhigyanpatwari/GitNexus).

## Licence

PolyForm Noncommercial 1.0.0
