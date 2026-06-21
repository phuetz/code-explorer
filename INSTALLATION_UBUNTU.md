# Installation de Code Explorer sur Ubuntu

Derniere mise a jour: 2026-05-11

Ce guide decrit l'installation de Code Explorer sur une machine Ubuntu neuve. Il
couvre la CLI Rust, le serveur HTTP, le chat React et l'application desktop
Tauri.

Les exemples supposent Ubuntu LTS recent et un shell Bash.

## 1. Prevoir l'espace disque

Code Explorer compile plusieurs crates Rust et plusieurs interfaces Node. Prevoir au
minimum 20 Go libres pour un poste de developpement confortable.

Si la partition systeme est petite, placer le depot et les repertoires de build
sur un disque plus grand:

```bash
mkdir -p /mnt/data/CascadeProjects
cd /mnt/data/CascadeProjects
```

Option utile si `target/` doit aussi partir sur ce disque:

```bash
export CARGO_TARGET_DIR=/mnt/data/cargo-target/code-explorer
```

Pour rendre cette variable permanente, l'ajouter a `~/.bashrc`.

## 2. Installer les paquets systeme

```bash
sudo apt update
sudo apt install -y \
  git \
  curl \
  wget \
  ca-certificates \
  gnupg \
  build-essential \
  pkg-config \
  cmake \
  file \
  libssl-dev \
  libwebkit2gtk-4.1-dev \
  libxdo-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

Notes:

- `build-essential` compile les grammaires tree-sitter.
- `cmake` est utile pour le backend KuzuDB optionnel.
- Les paquets `libwebkit2gtk-4.1-dev`, `libxdo-dev`,
  `libayatana-appindicator3-dev` et `librsvg2-dev` sont requis pour Tauri v2.

## 3. Installer Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup default stable
rustc --version
cargo --version
```

Le workspace Code Explorer declare `rust-version = "1.75"`. Utiliser Rust stable est
le chemin le plus simple.

## 4. Installer Node.js

Les lockfiles actuels utilisent Vite 8 pour `chat-ui` et l'UI desktop. Installer
Node.js 22 LTS ou plus recent; Node 18 n'est plus suffisant pour tout compiler.

Installation conseillee via `nvm`:

```bash
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
source "$HOME/.nvm/nvm.sh"
nvm install 22
nvm alias default 22
nvm use 22
node --version
npm --version
```

Verifier que `node --version` affiche au minimum `v22.12.0`.

## 5. Recuperer le depot

Installation depuis GitHub:

```bash
mkdir -p "$HOME/CascadeProjects"
cd "$HOME/CascadeProjects"
git clone https://github.com/phuetz/code-explorer.git
cd code-explorer
```

Si le depot est copie depuis une autre machine, eviter de transferer les gros
dossiers generes:

```text
target/
target-codex/
node_modules/
.codeexplorer/
```

Puis, depuis la racine du depot:

```bash
git status --short
```

## 6. Installer les dependances frontend

Utiliser `npm ci` pour respecter les `package-lock.json`.

```bash
npm ci --prefix chat-ui
npm ci --prefix crates/code-explorer-desktop/ui
```

Optionnel, si l'ancienne interface `nexus-brain` doit aussi etre lancee:

```bash
npm ci --prefix nexus-brain
```

## 7. Compiler la CLI

```bash
cargo build --release -p code-explorer-cli
CLI_BIN="${CARGO_TARGET_DIR:-$PWD/target}/release/code-explorer"
"$CLI_BIN" --help
```

Installer un raccourci utilisateur:

```bash
CLI_BIN="${CARGO_TARGET_DIR:-$PWD/target}/release/code-explorer"
mkdir -p "$HOME/.local/bin"
ln -sf "$CLI_BIN" "$HOME/.local/bin/code-explorer"
```

Verifier que `~/.local/bin` est dans le `PATH`, puis ouvrir un nouveau terminal
si necessaire:

```bash
code-explorer --help
```

## 8. Configurer le LLM

Code Explorer lit sa configuration dans `~/.codeexplorer/chat-config.json`.

Exemple avec ChatGPT OAuth:

```bash
mkdir -p "$HOME/.codeexplorer"
cat > "$HOME/.codeexplorer/chat-config.json" <<'JSON'
{
  "provider": "chatgpt",
  "api_key": "",
  "base_url": "https://chatgpt.com/backend-api/codex",
  "model": "gpt-5.5",
  "max_tokens": 8192,
  "reasoning_effort": "high"
}
JSON

code-explorer login
code-explorer config test
```

Pour un serveur sans navigateur graphique, preferer un fournisseur avec cle API
compatible OpenAI et renseigner `api_key` dans ce meme fichier.

Ne jamais copier `~/.codeexplorer/auth/openai.json` dans le depot: il contient des
tokens personnels.

## 9. Indexer un projet

```bash
code-explorer analyze /chemin/vers/mon-projet
code-explorer list
```

Forcer une reindexation complete:

```bash
code-explorer analyze /chemin/vers/mon-projet --force
```

## 10. Lancer le serveur HTTP

Terminal 1:

```bash
code-explorer serve --port 3010
```

Tester:

```bash
curl http://127.0.0.1:3010/health
```

Le serveur ecoute par defaut sur `127.0.0.1`. Si vous le liez a une interface
reseau, definir d'abord un token:

```bash
export CODE_EXPLORER_HTTP_TOKEN='changer-cette-valeur'
code-explorer serve --host 0.0.0.0 --port 3010
```

## 11. Lancer le chat React

Terminal 2:

```bash
printf 'VITE_MCP_URL=http://127.0.0.1:3010\n' > chat-ui/.env.local
npm --prefix chat-ui run dev -- --host 127.0.0.1 --port 5176 --strictPort
```

Ouvrir ensuite:

```text
http://127.0.0.1:5176
```

Si le port est deja pris, choisir un autre port:

```bash
npm --prefix chat-ui run dev -- --host 127.0.0.1 --port 5177 --strictPort
```

## 12. Generer la documentation HTML

```bash
code-explorer generate html --path /chemin/vers/mon-projet
```

Avec enrichissement LLM:

```bash
code-explorer generate html --path /chemin/vers/mon-projet --enrich
```

Le site est genere par defaut dans:

```text
/chemin/vers/mon-projet/.codeexplorer/docs/index.html
```

## 13. Compiler et lancer l'application desktop

Installer la CLI Tauri:

```bash
cargo install tauri-cli --locked
```

Mode developpement:

```bash
cd crates/code-explorer-desktop
cargo tauri dev
```

Build release:

```bash
cd ../..
chmod +x build-release.sh
./build-release.sh desktop
```

Les paquets generes se trouvent dans:

```text
target/release/bundle/
```

Pour construire uniquement la CLI avec le script fourni:

```bash
./build-release.sh cli
```

## 14. Verifier l'installation

Verification rapide:

```bash
cargo fmt --check
cargo test -p code-explorer-cli -p code-explorer-mcp -p code-explorer-desktop
npm --prefix chat-ui run build
npm --prefix crates/code-explorer-desktop/ui run build
```

Verification plus complete:

```bash
cargo test --workspace
npm --prefix chat-ui run lint
npm --prefix chat-ui run test
npm --prefix chat-ui run build
npm --prefix crates/code-explorer-desktop/ui run lint
npm --prefix crates/code-explorer-desktop/ui run build
```

## 15. Depannage rapide

### `node` trop ancien

Symptome typique: Vite refuse de demarrer ou signale une contrainte `engines`.

```bash
node --version
nvm install 22
nvm use 22
```

Puis reinstaller les dependances:

```bash
rm -rf chat-ui/node_modules crates/code-explorer-desktop/ui/node_modules
npm ci --prefix chat-ui
npm ci --prefix crates/code-explorer-desktop/ui
```

### Erreur Tauri sur WebKitGTK

Sur Ubuntu recent, installer `libwebkit2gtk-4.1-dev`, pas l'ancien paquet
`libwebkit2gtk-4.0-dev`.

```bash
sudo apt install -y libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev
```

### Erreur OpenSSL ou `pkg-config`

```bash
sudo apt install -y pkg-config libssl-dev
```

### Port deja utilise

```bash
ss -ltnp | grep ':3010'
ss -ltnp | grep ':5176'
```

Changer ensuite le port du serveur ou du chat:

```bash
code-explorer serve --port 3011
printf 'VITE_MCP_URL=http://127.0.0.1:3011\n' > chat-ui/.env.local
npm --prefix chat-ui run dev -- --host 127.0.0.1 --port 5177 --strictPort
```

### Manque d'espace disque pendant `cargo build`

Nettoyer les anciens artefacts:

```bash
cargo clean
rm -rf target-codex
```

Ou deplacer les artefacts sur un disque plus grand:

```bash
export CARGO_TARGET_DIR=/mnt/data/cargo-target/code-explorer
cargo build --release -p code-explorer-cli
```

## 16. Ordre conseille pour une premiere utilisation

```bash
sudo apt update
sudo apt install -y git curl wget ca-certificates gnupg build-essential pkg-config cmake file libssl-dev libwebkit2gtk-4.1-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
source "$HOME/.nvm/nvm.sh"
nvm install 22
nvm alias default 22

git clone https://github.com/phuetz/code-explorer.git
cd code-explorer
npm ci --prefix chat-ui
npm ci --prefix crates/code-explorer-desktop/ui
cargo build --release -p code-explorer-cli

mkdir -p "$HOME/.local/bin"
CLI_BIN="${CARGO_TARGET_DIR:-$PWD/target}/release/code-explorer"
ln -sf "$CLI_BIN" "$HOME/.local/bin/code-explorer"

code-explorer analyze /chemin/vers/mon-projet
code-explorer serve --port 3010
```

Dans un second terminal:

```bash
cd "$HOME/CascadeProjects/code-explorer"
printf 'VITE_MCP_URL=http://127.0.0.1:3010\n' > chat-ui/.env.local
npm --prefix chat-ui run dev -- --host 127.0.0.1 --port 5176 --strictPort
```

## References

- Rust: https://www.rust-lang.org/tools/install
- Tauri v2 Linux prerequisites: https://v2.tauri.app/start/prerequisites/
- Node.js: https://nodejs.org/
