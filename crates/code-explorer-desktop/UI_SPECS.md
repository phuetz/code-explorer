# Code Explorer Desktop — UI/UX Specification v2.0

> **Objectif** : Créer la meilleure interface de code intelligence & graph visualization du marché, en s'inspirant des forces de Sourcegraph, Neo4j Bloom, Linear, VS Code, Warp et Raycast, tout en corrigeant les faiblesses identifiées chez les concurrents.

---

## 1. Audit concurrentiel — Synthèse

### 1.1 Sourcegraph
| Force | Faiblesse |
|-------|-----------|
| Search-first UX : barre pleine largeur, filtres dynamiques | UI dense qui peut intimider les nouveaux utilisateurs |
| Code intelligence inline (hover docs, go-to-def) | Pas de visualisation graphe native |
| Design system "Wildcard" cohérent | Peu de personnalisation visuelle |
| MCP server pour AI agents | Interface très orientée texte, peu de datavis |

### 1.2 Neo4j Bloom
| Force | Faiblesse |
|-------|-----------|
| Exploration graphe naturelle (near-NLP search) | Pas orienté code (graph DB générique) |
| GPU-powered rendering, performances élevées | UI manque de modernité (look 2020) |
| Perspectives partagées (collaboration) | Pas de code view intégrée |
| Interaction directe drag-to-move | Apprentissage des Perspectives complexe |

### 1.3 CodeScene
| Force | Faiblesse |
|-------|-----------|
| Hotspot visualization (heatmaps) | Visualisations statiques, pas d'exploration interactive |
| Code Health scoring intégré | Dashboard-only, pas d'IDE feel |
| Change coupling maps | Pas de graph knowledge navigable |
| Link code quality → business impact | UX can overwhelm on large codebases |

### 1.4 SciTools Understand
| Force | Faiblesse |
|-------|-----------|
| Multiple graph types (call tree, UML, data flow) | UI datée, look "enterprise 2015" |
| Node previews avec AI summaries (v7.1) | Steep learning curve |
| Comprehensive static analysis | Pas web-native |
| Customizable layouts per graph type | Trop de features exposées sans hiérarchie |

### 1.5 GitHub Code Navigation
| Force | Faiblesse |
|-------|-----------|
| Zero friction (built-in) | Limité au scope du repository |
| Language-agnostic via SCIP | Pas de visualisation graphe |
| Integration seamless dans le workflow | Cross-repo navigation limitée |

### 1.6 Outils de référence UX (Linear, Warp, Raycast, Arc)
| Outil | Pattern clé à adopter |
|-------|----------------------|
| **Linear** | "Calmer interface" — surfaces empilées avec contraste minimal, whitespace généreux, motion purposeful |
| **Warp** | Blocks pour grouper input/output, theming extensible, command suggestions |
| **Raycast** | Keyboard-first, command palette minimal, fuzzy search |
| **Arc** | Vertical tabs, spaces/folders pour organisation, customizable accents |
| **VS Code** | Multi-panel flexible, view containers, sidebar consolidation |

---

## 2. Analyse de l'interface actuelle — Problèmes identifiés

### 2.1 Design System Audit

| Catégorie | Score | Problèmes |
|-----------|-------|-----------|
| **Token Coverage** | 7/10 | Bon système de variables CSS, mais certains composants utilisent encore des valeurs hardcoded (ex: `#292e42` dans GraphExplorer) |
| **Naming Consistency** | 6/10 | Mix d'alias legacy (`--border`, `--bg-primary`) et de noms v2 (`--bg-0`, `--surface`) |
| **Component States** | 5/10 | Hover states OK, mais manque : focus-visible cohérent, loading skeletons, disabled states, error states structurés |
| **Typography** | 6/10 | 3 fonts chargées (Outfit, DM Sans, JetBrains Mono) — bien, mais l'échelle typo n'est pas assez stricte (tailles arbitraires) |
| **Spacing** | 5/10 | Padding/margin inconsistants entre composants (px-3, px-4, px-6, px-8, px-10…) |
| **Motion** | 4/10 | Quelques animations basiques (fadeIn, shimmer) mais pas de système cohérent |
| **Accessibility** | 3/10 | Focus ring défini globalement mais pas testé ; pas d'ARIA roles, pas de skip-nav |

### 2.2 Problèmes UX critiques

1. **Graph Explorer — Nœuds invisibles** : Avec le mock data, les nœuds se regroupent en haut à gauche au zoom "package". Le layout `grid` place 1 seul nœud quand il n'y a qu'un package.
2. **Detail Panel — Pas de progressive disclosure** : Tout est affiché d'un coup (callers, callees, imports, community). Devrait être des sections collapsibles.
3. **Command Bar / Search — Sous-exploité** : La barre de commande est juste un breadcrumb + trigger search. Devrait être un vrai command center (à la Raycast/VS Code).
4. **Sidebar — Pas de contextual awareness** : La sidebar ne s'adapte pas au contenu actif (ex: quand on est dans le graphe, elle pourrait montrer un mini-legend ou des filtres).
5. **File Explorer — Pas de code preview** : Cliquer un fichier ne fait rien d'utile sans panel de code.
6. **Repo Manager — Pas d'onboarding** : L'état vide est minimal. Devrait guider l'utilisateur avec des étapes claires.
7. **Status Bar — Sous-utilisée** : Juste "repo + zoom + version". Pourrait montrer des métriques live.
8. **Aucune vue Impact Analysis** : Le tab existe dans la sidebar mais la vue n'est pas implémentée.
9. **Aucune vue Documentation** : Idem.
10. **Pas de minimap** : Les outils modernes de graph viz offrent une minimap pour naviguer les grands graphes.

---

## 3. Design Principles — Code Explorer

Inspirés des meilleures pratiques identifiées :

### P1: **Clarity over density**
> Chaque pixel a un purpose. L'information est progressive : overview → detail on demand.

### P2: **Graph-first, code-connected**
> Le graphe est le cœur de l'expérience. Chaque vue ramène au graphe. Le code est toujours à un clic.

### P3: **Keyboard-first, mouse-friendly**
> Toute action clé est accessible via raccourci. Le command palette est l'outil central.

### P4: **Calm authority**
> L'interface respire la confiance technique sans intimider. Inspiration Linear : calme, précise, élégante.

### P5: **Contextual intelligence**
> L'UI s'adapte au contexte : le panneau latéral change selon la vue, les actions proposées dépendent de la sélection.

---

## 4. Design Tokens — v2.1 (Évolution)

### 4.1 Couleurs — Palette "Obsidian Observatory"

```
Background Layers (5 niveaux)
──────────────────────────────
--bg-0: #0a0c12    (app background, deepest)
--bg-1: #0e1119    (sidebar, panels)
--bg-2: #141821    (cards, elevated surfaces)
--bg-3: #1b2030    (interactive surfaces, inputs)
--bg-4: #242a3a    (hover states, dividers)

Surfaces
──────────────────────────────
--surface:             #12161f
--surface-hover:       #1a1f2e
--surface-active:      #212740
--surface-border:      rgba(148, 163, 194, 0.08)
--surface-border-hover:rgba(148, 163, 194, 0.16)
--surface-elevated:    #161b28   (NEW — popovers, dropdowns, modals)

Text Hierarchy (5 niveaux)
──────────────────────────────
--text-0: #eaeff7   (headings, primary content)
--text-1: #c8d1e0   (body text)
--text-2: #8e99b0   (secondary labels)
--text-3: #5c677d   (placeholders, disabled)
--text-4: #3d4558   (decorative, dividers text)

Accent (Blue — brand color)
──────────────────────────────
--accent:        #6aa1f8
--accent-hover:  #83b3fa
--accent-muted:  #5088d4      (NEW — less prominent accent uses)
--accent-subtle: rgba(106, 161, 248, 0.10)
--accent-border: rgba(106, 161, 248, 0.25)
--accent-glow:   rgba(106, 161, 248, 0.06)

Semantic Colors
──────────────────────────────
--green:   #4ade80    (success, exported, active)
--amber:   #fbbf24    (warning, classes, enums)
--rose:    #fb7185    (error, danger, imports)
--purple:  #a78bfa    (info, structs, community)
--cyan:    #67e8f9    (functions, methods, code)
--teal:    #2dd4bf    (special, traits)
--orange:  #fb923c    (NEW — structs, routes)

Chaque couleur sémantique a 3 variantes :
  --{color}:         couleur pleine
  --{color}-subtle:  rgba(..., 0.10)  (backgrounds)
  --{color}-border:  rgba(..., 0.25)  (borders)
```

### 4.2 Typography Scale (stricte)

```
Font Families
──────────────────────────────
--font-display: 'Outfit', system-ui, sans-serif     (headings, nav labels)
--font-body:    'DM Sans', system-ui, sans-serif     (body, descriptions)
--font-mono:    'JetBrains Mono', monospace           (code, paths, IDs)

Size Scale (rem-based, 13px base)
──────────────────────────────
--text-2xs:  10px   (badges, micro-labels)
--text-xs:   11px   (metadata, file paths)
--text-sm:   12px   (secondary text, table cells)
--text-base: 13px   (body default)
--text-md:   14px   (emphasized body, nav items)
--text-lg:   16px   (section titles)
--text-xl:   18px   (page titles)
--text-2xl:  22px   (hero headings)

Weight Scale
──────────────────────────────
--weight-normal:   400
--weight-medium:   500   (labels, nav items)
--weight-semibold: 600   (headings, buttons)
--weight-bold:     700   (hero headings, brand)

Line Heights
──────────────────────────────
--leading-tight:   1.3   (headings)
--leading-normal:  1.55  (body text)
--leading-relaxed: 1.7   (code blocks, long reads)
```

### 4.3 Spacing Scale

```
--space-0:   0px
--space-1:   4px    (tight gaps, icon-text)
--space-2:   8px    (item spacing, small gaps)
--space-3:   12px   (section padding, card padding)
--space-4:   16px   (panel padding)
--space-5:   20px   (section gaps)
--space-6:   24px   (large section gaps)
--space-8:   32px   (page padding)
--space-10:  40px   (hero spacing)
--space-12:  48px   (major layout gaps)
```

### 4.4 Radii

```
--radius-xs:  4px   (badges, tags)
--radius-sm:  6px   (buttons, inputs)
--radius-md:  8px   (cards, panels)
--radius-lg:  12px  (modals, large cards)
--radius-xl:  16px  (hero elements)
--radius-full: 9999px (pills, avatars)
```

### 4.5 Shadows (3 niveaux)

```
--shadow-sm:   0 1px 2px rgba(0,0,0,0.25)
--shadow-md:   0 4px 12px rgba(0,0,0,0.25), 0 1px 3px rgba(0,0,0,0.15)
--shadow-lg:   0 8px 32px rgba(0,0,0,0.35), 0 2px 8px rgba(0,0,0,0.2)
--shadow-glow: 0 0 24px var(--accent-glow)
--shadow-xl:   0 16px 48px rgba(0,0,0,0.4), 0 4px 16px rgba(0,0,0,0.2)  (NEW)
```

### 4.6 Motion

```
Durations
──────────────────────────────
--duration-instant:  80ms    (micro-feedback: color changes)
--duration-fast:     120ms   (button presses, toggles)
--duration-base:     180ms   (panel transitions, hovers)
--duration-slow:     280ms   (page transitions, expansions)
--duration-slower:   400ms   (modal entry, complex animations)

Easings
──────────────────────────────
--ease-out:    cubic-bezier(0.16, 1, 0.3, 1)     (natural deceleration)
--ease-in-out: cubic-bezier(0.45, 0, 0.55, 1)    (symmetric)
--ease-spring: cubic-bezier(0.34, 1.56, 0.64, 1) (bouncy, for attention)
```

---

## 5. Layout Architecture

### 5.1 Shell Layout

```
┌─────────────────────────────────────────────────────────────────┐
│  CommandBar (46px) — breadcrumbs + command palette trigger       │
├──────┬──────────────────────────────────────────┬───────────────┤
│      │                                          │               │
│  S   │           Main Content Area              │   Detail      │
│  i   │     (GraphExplorer / FileExplorer /      │   Panel       │
│  d   │      RepoManager / ImpactAnalysis /      │   (resizable  │
│  e   │      Documentation)                      │    30-50%)    │
│  b   │                                          │               │
│  a   │                                          │               │
│  r   │                                          │               │
│      │                                          │               │
│(52-  │                                          │               │
│ 220  │                                          │               │
│  px) │                                          │               │
│      │                                          │               │
├──────┴──────────────────────────────────────────┴───────────────┤
│  StatusBar (28px) — repo info + metrics + version               │
└─────────────────────────────────────────────────────────────────┘
```

### 5.2 Responsive Breakpoints (Desktop app)

| Breakpoint | Window Width | Behavior |
|-----------|-------------|----------|
| Compact   | < 900px     | Sidebar auto-collapse, Detail Panel en overlay |
| Normal    | 900-1400px  | Layout standard |
| Wide      | > 1400px    | Detail Panel peut montrer Code + Context côte à côte |

---

## 6. Component Specifications

### 6.1 CommandBar (header)

**Role** : Navigation hub + Command palette trigger. C'est le "cerveau" de l'app.

```
┌──────────────────────────────────────────────────────────────────┐
│ [breadcrumb: repo > view > node]          [⌘K Search symbols…]  │
└──────────────────────────────────────────────────────────────────┘
```

**Specs** :
- Hauteur fixe : 46px
- Background : `--bg-1` avec border-bottom `--surface-border`
- Breadcrumb :
  - Repo name : chip avec dot de statut vert (inline, `--green`, 6px circle)
  - Séparateur : `›` en `--text-4`
  - View name : chip accent `--accent-subtle` background
  - Node name (si sélectionné) : chip `--purple-subtle` background
  - Chaque chip : `padding: 2px 10px`, `border-radius: var(--radius-full)`, font `--text-sm`, `--weight-medium`
- Search trigger :
  - Right-aligned, `min-width: 240px`
  - Style : input ghost avec `--bg-3` background, `--text-3` placeholder
  - Icône Search (14px) + texte "Search symbols…" + badge `⌘K`
  - Hover : `--surface-border-hover` border apparaît
  - Click : ouvre SearchModal (plein écran centré)

**States** :
| State | Comportement |
|-------|-------------|
| No repo | Breadcrumb = "Code Explorer", search disabled (grayed out) |
| Repo loaded | Breadcrumb complet, search active |
| Node selected | 3 niveaux de breadcrumb visibles |

**Interactions** :
- Click sur un segment du breadcrumb → navigue vers ce niveau
- `⌘K` / `Ctrl+K` → ouvre le command palette
- `⌘P` / `Ctrl+P` → ouvre le file finder

---

### 6.2 Sidebar

**Role** : Navigation principale entre les vues. Collapsible.

**Specs** :
- Largeur expanded : 220px, collapsed : 52px
- Background : `--bg-1`
- Border-right : `1px solid var(--surface-border)`
- Transition : `width var(--duration-slow) var(--ease-out)`

**Structure** :
```
┌────────────────────────┐
│  [G] Code Explorer    [◀]   │  ← Logo + collapse toggle
│────────────────────────│  ← Separator (1px, --surface-border)
│  WORKSPACE             │  ← Section label (10px, uppercase, --text-3)
│  ▸ Repositories        │
│  ▸ File Explorer       │
│────────────────────────│  ← Separator
│  EXPLORE               │  ← Renamed from "TOOLS"
│  ▸ Graph Explorer      │
│  ▸ Impact Analysis     │
│  ▸ Documentation       │
│                        │
│  (spacer)              │
│────────────────────────│  ← Separator
│  ▸ Settings            │
└────────────────────────┘
```

**NavItem** :
- Padding : `7px 10px` (expanded), `8px` (collapsed)
- Border-radius : `var(--radius-md)`
- Icon : 16px, `--text-2` (inactive), `--accent` (active)
- Label : `13px`, `--weight-medium`, `--text-2` (inactive), `--accent` (active)
- Active state :
  - Background : `--accent-subtle`
  - Left indicator : 2.5px wide, gradient `transparent → --accent → transparent`, avec glow
  - Subtle radial gradient glow depuis la gauche
- Hover state (inactive) :
  - Background : `--surface-hover`
  - Text color : `--text-1`
- Transition : `all var(--duration-fast) var(--ease-out)`

**Collapsed mode** :
- Seul l'icône est visible, centré
- Tooltip au hover montrant le label
- Section labels remplacées par un spacer de 12px

---

### 6.3 Graph Explorer (vue principale)

**Role** : Cœur de l'application. Visualisation interactive du knowledge graph.

**Sub-components** :
1. **GraphToolbar** (top)
2. **GraphCanvas** (Cytoscape)
3. **GraphMinimap** (NEW — bottom-right overlay)
4. **GraphLegend** (NEW — bottom-left overlay)

#### 6.3.1 GraphToolbar

```
┌──────────────────────────────────────────────────────────────┐
│ [Packages] [Modules] [Symbols]  │  Force ▾  │ ⊕ │  8n 8e   │
└──────────────────────────────────────────────────────────────┘
```

**Specs** :
- Hauteur : 40px
- Background : `--bg-1`
- Border-bottom : `1px solid var(--surface-border)`
- Padding : `0 var(--space-4)`

**Zoom Level Pills** :
- Groupe de 3 boutons en pill toggle
- Container : `--bg-3` background, `border-radius: var(--radius-full)`, `padding: 2px`
- Item actif : `--accent` background, `white` text, `border-radius: var(--radius-full)`
- Item inactif : transparent, `--text-2`
- Taille : `padding: 4px 12px`, font `11px`, `--weight-medium`

**Layout Selector** :
- Dropdown button avec chevron
- Options : Force, Grid, Circle, Hierarchical
- Chaque option avec icône descriptive

**Fit button** :
- Icon-only button (Maximize2, 16px)
- `--bg-3` background, `--text-2` color
- Hover : `--surface-border-hover` border

**Stats** :
- `{n} nodes  {e} edges` en badges arrondis
- Font mono, `10px`, `--text-3`

#### 6.3.2 GraphCanvas

**Cytoscape Styles** :
```
Nodes :
  - Size : 36px (package), 28px (module), 20px (symbol)
  - Shape : roundrectangle (packages), ellipse (others)
  - Border : 2px solid, même couleur que fill mais 20% plus sombre
  - Selected : border 3px --accent, size +25%, shadow glow
  - Label : font-size 11px, --text-1, text-margin-y 8px
  - Colors par type (voir Node Type Colors ci-dessous)

Edges :
  - Width : 1px (normal), 2px (selected/hover)
  - Color : --bg-4 (normal), --accent (selected)
  - Arrow : triangle, scale 0.5
  - Curve : bezier
  - Hover : color --text-3, width 1.5px

Background :
  - Radial gradient center : accent-glow 2% → transparent
  - Dot grid : --bg-4, 0.5px dots, 20px spacing
```

**Node Type Colors** :
```
Function    → #7aa2f7  (blue)
Class       → #bb9af7  (purple)
Method      → #7dcfff  (cyan)
Interface   → #e0af68  (amber)
Struct      → #ff9e64  (orange)
Trait       → #9ece6a  (green)
Enum        → #f7768e  (rose)
Module      → #565f89  (gray)
Package     → #414868  (dark gray)
File        → #565f89  (gray)
Variable    → #73daca  (teal)
Type        → #c0caf5  (light)
Import      → #414868  (dark)
Constructor → #7dcfff  (cyan)
Property    → #73daca  (teal)
Constant    → #bb9af7  (purple)
```

**Interactions** :
| Action | Comportement |
|--------|-------------|
| Click node | Select → detail panel shows context |
| Double-click node | Zoom to next level (package→module→symbol) |
| Click background | Deselect all |
| Hover node | Tooltip glass avec name + type + filepath |
| Drag node | Move node |
| Scroll | Zoom in/out |
| Ctrl+scroll | Pan horizontal |
| Right-click node | Context menu (Go to file, Show callers, Expand, Hide) |
| Drag selection | Box select multiple nodes |

#### 6.3.3 GraphMinimap (NEW)

**Specs** :
- Position : bottom-right, 16px margin
- Size : 160×120px
- Background : `--bg-2` avec border `--surface-border`
- Border-radius : `--radius-md`
- Opacity : 0.8 (idle), 1.0 (hover)
- Shows : simplified view of full graph with viewport rectangle
- Viewport rect : `--accent-border` stroke, `--accent-glow` fill

#### 6.3.4 GraphLegend (NEW)

**Specs** :
- Position : bottom-left, 16px margin
- Collapsible (par défaut fermé, icône "Legend")
- Quand ouvert : liste des types de nœuds avec dot coloré + label
- Max height : 200px, scrollable
- Background : glass effect (`--surface-elevated`, backdrop-blur)
- Close button en top-right

---

### 6.4 Detail Panel

**Role** : Panneau contextuel montrant les détails du nœud sélectionné.

**Specs** :
- Default width : 38% du content area (min 24%, max 50%)
- Background : `--bg-0`
- Border-left : `1px solid var(--surface-border)`
- Transition panel resize : smooth via react-resizable-panels

**Structure** :
```
┌─────────────────────────────────┐
│  [Context] [Code] [Properties]  │  ← Tab bar (pill-style buttons)
├─────────────────────────────────┤
│                                 │
│  ┌─ Node Header Card ─────────┐│
│  │ [Function] [exported]       ││  ← Type badge + status badges
│  │ run_pipeline                ││  ← Symbol name (--text-0, 16px, semibold)
│  │ src/pipeline.rs:42-120      ││  ← File path (--font-mono, 11px, --text-3)
│  └─────────────────────────────┘│
│                                 │
│  ▾ CALLERS (1)                  │  ← Collapsible section (NEW)
│  ┌──────────────────────────┐   │
│  │ Module  ingest            │   │
│  │         src/ingest/mod.rs │   │
│  └──────────────────────────┘   │
│                                 │
│  ▾ CALLEES (3)                  │  ← Collapsible section
│  ┌──────────────────────────┐   │
│  │ Function  parse_file      │   │
│  │           src/parser.rs   │   │
│  ├──────────────────────────┤   │
│  │ Function  resolve_imports │   │
│  │           src/imports.rs  │   │
│  ├──────────────────────────┤   │
│  │ Function  detect_comms    │   │
│  │           src/community.rs│   │
│  └──────────────────────────┘   │
│                                 │
│  ▸ COMMUNITY                    │  ← Collapsed by default
│                                 │
└─────────────────────────────────┘
```

**Tab Bar** :
- Background : `--bg-1`
- Padding : `var(--space-3) var(--space-4)`
- Border-bottom : `1px solid var(--surface-border)`
- Tabs : pill buttons
  - Active : `--accent` bg, white text
  - Inactive : `--bg-3` bg, `--text-2`
  - Size : `padding: 6px 14px`, `border-radius: var(--radius-full)`, font `12px`, `--weight-medium`

**Node Header Card** :
- Background : `--bg-1`
- Border-left : `4px solid {node-type-color}`
- Border-radius : `--radius-md`
- Padding : `var(--space-3)`
- Badges : `border-radius: var(--radius-xs)`, `padding: 2px 8px`, `font-size: 11px`
- Name : font-display, `--text-lg`, `--weight-semibold`
- Path : font-mono, `--text-xs`, `--text-3`

**Relation Sections** (NEW: collapsible) :
- Header : clickable, `cursor-pointer`
  - Chevron icon (▾/▸) rotating on toggle
  - Title : uppercase, `--text-xs`, `--weight-semibold`, `--text-2`
  - Count badge : `--text-3`
- Items : cards with hover effect
  - Background : `--bg-1`
  - Border : `1px solid var(--surface-border)`
  - Border-radius : `--radius-md`
  - Padding : `var(--space-2) var(--space-3)`
  - Type badge : `--bg-3` bg, `--text-2`, 10px
  - Name : `--text-0`, 12px, `--weight-medium`
  - Path : `--text-3`, 10px, font-mono
  - Hover : `--surface-hover` bg, `--surface-border-hover` border
  - Click : navigate to node in graph + select

**Empty State** :
- Centered content
- Icon : circle outline (○) dans un carré arrondi `--bg-2`
- Title : "No node selected" (`--text-1`, 14px, medium)
- Description : "Click a node in the graph to see its details" (`--text-3`, 12px)

---

### 6.5 Repo Manager

**Role** : Dashboard des repositories indexés. Premier écran de l'app.

**Empty State** (0 repos) :
```
┌─────────────────────────────────────────┐
│                                         │
│        ┌──────────────────┐             │
│        │   📊  Database   │             │
│        └──────────────────┘             │
│        No repositories indexed          │
│   Select a project folder to build      │
│   its knowledge graph                   │
│                                         │
│        [✨ Analyze Project]             │
│                                         │
│   ┌─────────────────────────────────┐   │
│   │  Step 1: Select a project folder│   │  ← NEW: Onboarding steps
│   │  Step 2: Wait for analysis      │   │
│   │  Step 3: Explore the graph!     │   │
│   └─────────────────────────────────┘   │
│                                         │
└─────────────────────────────────────────┘
```

**Loaded State** :
- Container : `max-width: 780px`, centered, `padding: var(--space-10)`
- Header : title "Repositories" + count + action buttons
- Cards : voir RepoCard spec
- Stagger animation sur les cards (50ms delay entre chaque)

**RepoCard** :
- Background : `--surface`
- Border : `1px solid var(--surface-border)`
- Border-radius : `--radius-xl`
- Hover : border `--surface-border-hover`, shadow `--shadow-md`
- Padding : `var(--space-5) var(--space-6)`
- Layout : avatar(40px) + content + menu button
- Avatar : gradient based on repo name hash, font-display, bold, white
- Name : font-display, `--text-sm`, `--weight-semibold`
- Path : font-mono, `11px`, `--text-3`
- Timestamp : Clock icon + relative time, `10px`, `--text-4`
- Stats badges : colored backgrounds with semantic colors
  - Files: accent
  - Nodes: purple
  - Edges: cyan
  - Communities: green

---

### 6.6 File Explorer

**Role** : Navigation dans l'arborescence des fichiers du repo.

**Specs** :
- Header : Folder icon + "Files" + file count badge
- Tree indentation : 20px per level
- Vertical guide lines : dotted, `--bg-4`, 1px, left-positioned at each depth level
- File icons colored by extension (language colors)
- Folder icons : standard yellow/amber

**File Item** :
- Padding : `4px 8px`
- Hover : `--surface-hover` background
- Selected : `--accent-subtle` background, `--accent` text
- Font : `--text-sm`, `--text-1`
- Icon : 14px, colored by language

**Language Colors** :
```
.js/.jsx    → var(--amber)     (JavaScript yellow)
.ts/.tsx    → #3b82f6          (TypeScript blue)
.py         → #3572A5          (Python blue)
.rs         → var(--rose)      (Rust orange-red)
.go         → #00ADD8          (Go cyan)
.java       → var(--rose)      (Java red)
.c/.cpp/.h  → #555555          (C gray)
.cs         → #178600          (C# green)
.rb         → #CC342D          (Ruby red)
.php        → #4F5D95          (PHP purple)
.kt         → var(--purple)    (Kotlin purple)
.swift      → var(--orange)    (Swift orange)
.md         → var(--text-2)    (Markdown gray)
.json       → var(--amber)     (JSON yellow)
.toml       → var(--teal)      (TOML teal)
.yaml/.yml  → var(--rose)      (YAML pink)
.css/.scss  → #563d7c          (CSS purple)
.html       → #e34c26          (HTML orange)
```

**Click Behavior** :
- Click file → show source code in Detail Panel (Code tab)
- Click folder → toggle expand/collapse
- Double-click file → open in main area as full code view

---

### 6.7 Search Modal (Command Palette)

**Role** : Recherche universelle + command palette. Cœur de la navigation rapide.

**Specs** :
- Overlay : backdrop blur 8px + `rgba(0,0,0,0.6)`
- Modal : centered, `max-width: 640px`, `max-height: 480px`
- Background : `--surface-elevated`
- Border : `1px solid var(--surface-border-hover)`
- Border-radius : `--radius-lg`
- Shadow : `--shadow-xl`
- Animation : scaleIn 200ms

**Structure** :
```
┌──────────────────────────────────────────┐
│  🔍  Search symbols, files, commands…   │  ← Input (16px, --text-0)
├──────────────────────────────────────────┤
│  SYMBOLS                                 │  ← Category header
│  ┌─ ƒ  run_pipeline  Function  ─────┐   │
│  │     src/pipeline.rs:42            │   │
│  ├─ ■  Parser         Class    ─────┤   │
│  │     src/parser.rs:1               │   │
│  ├─ ƒ  parse_file     Function ─────┤   │
│  │     src/parser.rs:15              │   │
│  └───────────────────────────────────┘   │
│                                          │
│  FILES                                   │  ← Category header
│  ┌─ 📄 pipeline.rs ────────────────┐     │
│  │    src/ingest/pipeline.rs        │     │
│  └──────────────────────────────────┘     │
│                                          │
│  ↑↓ Navigate   ↵ Open   ⎋ Close         │  ← Footer hints
└──────────────────────────────────────────┘
```

**Input** :
- Full-width, no border, transparent background
- Padding : `var(--space-4)`
- Font : `--text-md`, `--text-0`
- Placeholder : `--text-3`
- Auto-focus on open

**Results** :
- Grouped by category (Symbols, Files, Commands)
- Category headers : `10px`, uppercase, `--text-3`, `--weight-semibold`
- Result items :
  - Padding : `8px 16px`
  - Icon + name + type badge + file path
  - Highlight matched characters in `--accent`
  - Keyboard selection : up/down arrows, `--accent-subtle` background
  - Enter : navigate to result

**Keyboard** :
- `Escape` : close
- `↑/↓` : navigate results
- `Enter` : open selected result
- `⌘Enter` : open in new tab (future)
- Type to filter (debounced 150ms)

---

### 6.8 Status Bar

**Role** : Information bar en bas de l'app.

**Specs** :
- Hauteur : 28px
- Background : `--bg-1`
- Border-top : `1px solid var(--surface-border)`
- Padding : `0 var(--space-4)`
- Font : `--text-2xs` (10px), `--text-3`
- Layout : flex between left items and right items

**Left items** :
- Green dot (6px, pulsing if loading) + repo name
- Separator (`|` en `--text-4`)
- Zoom level (ex: "Zoom: package")
- Separator
- Active connections count (ex: "Active: 1")

**Right items** :
- Graph stats (if loaded) : "3.8k nodes · 12.7k edges"
- Separator
- "Code Explorer v0.1.0"

---

### 6.9 Impact Analysis (NEW — à implémenter)

**Role** : Visualiser l'impact d'un changement sur le codebase.

**Structure** :
```
┌──────────────────────────────────────────┐
│  Impact Analysis                          │
│  Select a symbol to analyze its impact    │
├──────────────────────────────────────────┤
│  🔍  Search symbol to analyze…           │  ← Search input
├──────────────────────────────────────────┤
│                                          │
│  ┌─ Direct Impact ─────────────────────┐ │
│  │ 3 callers, 5 callees               │ │
│  │ [graph visualization of 1-hop]      │ │
│  └─────────────────────────────────────┘ │
│                                          │
│  ┌─ Transitive Impact ─────────────────┐ │
│  │ 12 symbols affected (2+ hops)       │ │
│  │ [expandable list with file groups]  │ │
│  └─────────────────────────────────────┘ │
│                                          │
│  ┌─ Risk Assessment ──────────────────┐  │
│  │ ⚠ HIGH — 3 exported functions      │  │
│  │   affected, 2 communities crossed  │  │
│  └────────────────────────────────────┘  │
│                                          │
└──────────────────────────────────────────┘
```

---

### 6.10 Documentation View (NEW — à implémenter)

**Role** : Wiki auto-générée à partir du knowledge graph.

**Structure** :
- Left sidebar : Table of contents (tree structure by module)
- Main area : Markdown-rendered documentation
  - Auto-generated API docs per module
  - Mermaid diagrams for architecture
  - Cross-linked symbols (click to navigate to graph)
- Supports Shiki syntax highlighting for code blocks

---

## 7. Interaction Patterns

### 7.1 Navigation Flow

```
App Launch
  └→ RepoManager (if no active repo)
      └→ Click repo card
          └→ Graph Explorer (default view)
              ├→ Click node → Detail Panel shows context
              ├→ Double-click → Zoom deeper
              ├→ ⌘K → Search Modal
              └→ Sidebar tabs → switch views
```

### 7.2 Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `⌘K` / `Ctrl+K` | Open command palette |
| `⌘P` / `Ctrl+P` | Quick file open |
| `⌘B` / `Ctrl+B` | Toggle sidebar |
| `⌘\` / `Ctrl+\` | Toggle detail panel |
| `⌘1-5` | Switch sidebar tabs |
| `Escape` | Close modal / deselect |
| `F` | Fit graph to screen |
| `1/2/3` | Switch zoom level (package/module/symbol) |
| `L` | Cycle graph layouts |

### 7.3 Context Menus

**Node context menu** (right-click on graph node) :
```
┌─────────────────────────┐
│  Go to Definition       │
│  Find All References    │
│  ───────────────────── │
│  Expand Neighbors       │
│  Hide Node              │
│  ───────────────────── │
│  Copy Name              │
│  Copy File Path         │
└─────────────────────────┘
```

---

## 8. Animations & Micro-interactions

### 8.1 Page Transitions
- View switches (sidebar tab changes) : `fadeIn 250ms ease-out`
- Content appears with stagger : 50ms delay between items

### 8.2 Graph Interactions
- Node hover : scale 1.1 (150ms), glow shadow appears
- Node select : scale 1.25 (200ms), border color transition, ripple effect
- Edge hover : width 1→1.5px, color transition (120ms)
- Layout change : animate node positions (500ms spring easing)

### 8.3 Panel Interactions
- Detail panel sections collapse/expand : height animation (200ms ease-out)
- Tab switch : content crossfade (150ms)
- Tooltip : fadeIn (120ms) with 4px translateY

### 8.4 Loading States
- Graph loading : shimmer background + spinning square
- List loading : skeleton rows (3-5) with shimmer effect
- Button loading : spinner icon replaces content, width maintained

---

## 9. Accessibility Checklist

| Requirement | Status | Notes |
|------------|--------|-------|
| WCAG 2.1 AA contrast ratios | ⚠️ To verify | Text on backgrounds needs audit |
| Keyboard navigation all components | ❌ To implement | Tab order, arrow keys in lists |
| Focus-visible on all interactive | ✅ Partial | Global rule exists, needs per-component testing |
| Screen reader labels | ❌ To implement | ARIA labels on graph, toolbar buttons |
| Skip navigation link | ❌ To implement | "Skip to main content" |
| Reduced motion support | ❌ To implement | `prefers-reduced-motion` media query |
| Color not sole indicator | ⚠️ Partial | Node types use color + label, but graph relies on color |

---

## 10. Implementation Priority

### Phase 1 — Foundation (Sprint actuel)
1. ✅ Refonte CSS tokens (fait)
2. ✅ Refonte Sidebar (fait)
3. ✅ Refonte CommandBar (fait)
4. ✅ Refonte Graph Explorer styling (fait)
5. ✅ Refonte Detail Panel (fait)
6. ✅ Refonte File Explorer (fait)
7. ✅ Refonte Repo Manager (fait)
8. ✅ Collapsible sections dans Detail Panel (fait)
9. ✅ Minimap overlay (fait — canvas-based, click-to-pan, toggle visibility)
10. ✅ Graph Legend overlay (fait)

### Phase 2 — Intelligence
11. ✅ Command Palette amélioré (fuzzy search, commands) (fait)
12. ✅ Impact Analysis view (fait)
13. ✅ Context menu sur les nœuds (fait — 6 actions: Go to Definition, Find All References, Expand Neighbors, Hide Node, Copy Name, Copy File Path)
14. ✅ Code preview dans File Explorer (fait — Shiki syntax highlighting, 21 langages, navigation par fichier)

### Phase 3 — Polish
15. ✅ Documentation view (fait)
16. ✅ Keyboard shortcuts complets (fait — 14+ raccourcis: Ctrl+1-5, Ctrl+\, F, 1/2/3, L, Ctrl+K, Escape)
17. ✅ Accessibility audit & fixes (fait — skip-nav, aria-labels, focus-visible, sr-only)
18. ✅ Reduced motion support (fait — @media prefers-reduced-motion)
19. ✅ Responsive breakpoints (fait — useResponsive hook, auto-collapse sidebar < 900px)
20. ✅ Onboarding flow amélioré (fait — 3-step guide dans RepoManager empty state)

---

*Document généré le 26 mars 2026 — Code Explorer Desktop UI Specifications v2.0*
*Basé sur l'audit de : Sourcegraph, Neo4j Bloom, CodeScene, SciTools Understand, GitHub, Linear, Warp, Raycast, Arc Browser, VS Code*
