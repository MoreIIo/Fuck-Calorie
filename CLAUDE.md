# Fuck Calorie — repères pour Claude Code

Tracker de calories open source (Tauri 2 + Leptos, 100 % Rust). La référence fonctionnelle
est `docs/CAHIER_DES_CHARGES.md` ; l'état d'avancement est dans `docs/AVANCEMENT.md`
(à mettre à jour à la fin de chaque session de travail).

## Commandes

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

Les trois doivent passer avant de considérer une tâche terminée.

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
- Aucun secret dans le dépôt ; jetons et clés API uniquement dans le coffre sécurisé de la
  plateforme. Aucune télémétrie.
- Design : `border-radius: 0` partout, un seul accent (bleu Klein), cf. §6.
