# Exports sans ressources externes

HTML, PDF et DOCX utilisent Mermaid **11.14.0**, embarqué localement. Les
bibliothèques de l'export de graphe du bureau sont également embarquées ; leurs
versions, empreintes et licences figurent dans `THIRD-PARTY-NOTICES.md`.
Les exports n'utilisent aucun CDN ni police distante. Les liens de documentation
restent cliquables. Les images Markdown distantes sont affichées sous forme de
texte ; les captures PNG incorporées aux documents de travail restent disponibles.

## Moteur local pour PDF et DOCX

Installer Node.js, Playwright et Chromium **avant** l'utilisation sur un réseau
fermé, par exemple avec :

```sh
npm install -g playwright@1.58.2
playwright install chromium
```

Une installation locale de Playwright est aussi possible en définissant
`NODE_PATH` sur son répertoire `node_modules`. Le lanceur respecte cette valeur.
Aucune installation et aucun téléchargement ne sont déclenchés pendant l'export.

Les diagrammes DOCX sont capturés en PNG avec un facteur de résolution de 3.
Sans moteur local, le DOCX conserve le source Mermaid en bloc de code et affiche
un avertissement dans le document et sur stderr. Le PDF nécessite Chromium et
signale son indisponibilité. `CODE_EXPLORER_MERMAID_PLACEHOLDER=1` force le repli
texte du DOCX.

## Kroki auto-hébergé, uniquement sur choix explicite

Par défaut, aucun source Mermaid n'est envoyé à Kroki. Pour sélectionner une
instance administrée par votre organisation, fournir l'adresse complète du
point de rendu PNG dans `CODE_EXPLORER_KROKI_URL`, par exemple :

```sh
export CODE_EXPLORER_KROKI_URL=http://127.0.0.1:8000/mermaid/png
```

Cette configuration transmet les diagrammes à l'adresse choisie. Une variable
absente ou vide sélectionne le moteur local. Aucun service public n'est choisi
par défaut ; les redirections HTTP du point configuré sont désactivées.

## Test d'intégration hors réseau

```sh
cargo test -p code-explorer-cli -j 8 --test offline_exports -- --ignored --nocapture
```

Le test nécessite Node.js, Playwright/Chromium déjà installés et Python 3. Il est
ignoré par défaut pour permettre les tests Rust sur les machines sans navigateur.
Il génère la documentation d'un petit dépôt TypeScript, puis ses exports HTML,
PDF et DOCX. Un proxy refuse les appels HTTP(S) du CLI ; Playwright intercepte et
compte les demandes HTTP(S) des pages. Les lanceurs de production bloquent aussi
ces demandes avant tout envoi.

Le test vérifie le rendu SVG HTML, les PNG incorporés au DOCX, les images du PDF,
le repli DOCX sans Node.js et l'absence de demandes externes. Il vérifie ensuite,
séparément, l'option Kroki avec un serveur factice sur l'interface de boucle locale.
Les fichiers de diagnostic restent dans un répertoire temporaire. La variable
`CODE_EXPLORER_OFFLINE_ARTIFACT_DIR` permet de choisir un nouveau répertoire de
livraison. Les échecs de rendu et les délais d'attente ne sont pas masqués.

Les API du chat et de l'enrichissement restent utilisables lorsqu'un utilisateur
les configure et demande ces fonctions ; elles ne sont pas appelées par le rendu
des exports. Le site HTML exporté utilise uniquement les routes API de son origine
si l'utilisateur ouvre et utilise son chat intégré.
