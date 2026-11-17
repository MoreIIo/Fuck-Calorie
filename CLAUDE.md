# Fuck Calorie — repères pour Claude Code

Tracker de calories open source (Tauri 2 + Leptos, 100 % Rust). La référence fonctionnelle
est `docs/CAHIER_DES_CHARGES.md` ; l'état d'avancement est dans `docs/AVANCEMENT.md`
(à mettre à jour à la fin de chaque session de travail).

## Commandes

Lancer l'application : `cd app && cargo tauri dev` (prérequis dans le README).

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

Les trois doivent passer avant de considérer une tâche terminée.

Mettre à jour la table Ciqual embarquée (nouvelle édition de l'ANSES) : relancer l'import,
puis changer `VERSION` dans `crates/food-db/src/ciqual.rs` (c'est ce qui déclenche le
réimport dans les bases existantes).

```sh
cargo run -p fc-food-db --example import_ciqual -- <table Ciqual FR .csv> crates/food-db/data/ciqual.tsv
```

## Règles

- Suivre le cahier des charges ; citer la section mise en œuvre dans les commentaires
  (« §2.3 »). En cas d'écart nécessaire, le signaler à Maëlle au lieu de trancher seul.
- Identifiants en anglais ; commentaires, documentation et messages d'erreur en français.
- Les crates s'appellent `fc-*` (`fc-core`, `fc-storage`…) : le dossier est `crates/core`,
  mais un paquet nommé `core` entrerait en conflit avec la bibliothèque standard de Rust.
- `fc-core` : calcul pur, aucune entrée-sortie, pas d'horloge système (dates et horodatages
  en paramètre).
- Chaque calcul a des tests avec des valeurs vérifiables à la main.
- Ne jamais modifier une migration SQL existante (`crates/storage/migrations/`) : en ajouter
  une et l'inscrire dans `MIGRATIONS`.
- Toute écriture d'un objet synchronisé passe par les `upsert_*` de `fc-storage`, qui
  n'acceptent qu'une version plus récente (HLC) : c'est ce qui rend la sync idempotente.
- Les fiches de référence (Ciqual, Open Food Facts) ont un identifiant déterministe
  (`fc_food_db::reference_id`) : le même produit a le même identifiant sur tous les
  appareils. Ne jamais changer ce calcul.
- Open Food Facts : toute requête passe par `fc_food_db::client::OffClient` (User-Agent,
  limites de débit). Jamais de requête depuis l'interface.
- Les workflows GitHub (`.github/workflows/`) ne peuvent pas être écrits par Claude depuis
  cet environnement : leurs nouvelles versions sont déposées dans `scripts/workflows/`, et
  Maëlle les copie.
- Aucun secret dans le dépôt ; jetons et clés API uniquement dans le coffre sécurisé de la
  plateforme. Aucune télémétrie.
- Design : `border-radius: 0` partout, un seul accent (bleu Klein), cf. §6. Styles dans
  `app/styles/main.css`, tokens en variables CSS.
- L'interface (`app/`, `fc-app`) ne calcule rien : elle affiche ce que renvoient les
  commandes Tauri (`app/src-tauri/src/lib.rs`), qui appellent `fc-service`. Nouvelle
  fonction = méthode dans `fc-service` (avec tests) → commande Tauri → appel dans
  `app/src/api.rs` → composant.
- `fc-app` dépend de `fc-core` **sans** la fonctionnalité `rng` (pas d'aléa en WebAssembly).
