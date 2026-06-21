# COLAB.md — code-explorer-chat

> Version : 0.0.1
> Statut : démarrage V0
> Convention canonique : voir `D:\CascadeProjects\claude-et-patrice\COLAB.md`

## Règles cardinales (rappel)

1. Max 10 fichiers modifiés par itération
2. Chaque tâche testée avant de passer à la suivante
3. Aucun script automatique de correction sans validation préalable
4. Boucle de rétroaction obligatoire après chaque modif
5. Documenter chaque changement dans le journal

## Boucle de rétroaction (règle 4)

Après chaque itération, dans cet ordre :

```bash
npm run build       # tsc -b && vite build (vérifie types + bundle)
npm run lint
```

Pas de tests à V0. À ajouter (Vitest + React Testing Library) dès la V1.

## Statut convention

| Symbole | Signification |
|---------|---------------|
| `[ ]`   | À faire |
| `[~]`   | En cours (indiquer IA + date) |
| `[x]`   | Fait et validé |
| `[!]`   | Bloqué |
| `[-]`   | Abandonné (justification) |

## Journal de bord

Le journal vit dans `D:\CascadeProjects\claude-et-patrice\journal\<hostname>-code-explorer-chat.md`
selon la convention "fichier par source". JAMAIS écrire dans `journal.md` monolithique.

## Phases V0 → V3

Voir le `README.md` section Roadmap. À chaque phase complétée, mettre à jour
le README + une ligne dans `claude-et-patrice/etat_projets.md`.
