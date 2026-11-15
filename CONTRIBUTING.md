# Contribuer à Fuck Calorie

Merci de votre intérêt ! Le projet est jeune : avant un gros changement, ouvrez une issue
pour en discuter.

## Avant d'ouvrir une pull request

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Les trois commandes doivent passer ; l'intégration continue les relance.

## Conventions

- **Le cahier des charges fait foi** (`docs/CAHIER_DES_CHARGES.md`). Le code cite la section
  qu'il met en œuvre (« §2.3 »). Si le code doit s'en écarter, on modifie d'abord le cahier
  des charges.
- **Identifiants en anglais, commentaires et documentation en français.**
- **`fc-core` ne fait aucune entrée-sortie** : pas de base de données, de réseau ni d'horloge
  système. Les dates et les horodatages sont passés en paramètre, ce qui rend tout testable.
- **Tout calcul a ses tests**, avec des valeurs vérifiables à la main. Couverture visée pour
  `fc-core` : 80 % au moins.
- **Une migration SQL publiée ne se modifie jamais** : on ajoute un fichier
  `crates/storage/migrations/000N_description.sql` et on l'inscrit dans `MIGRATIONS`.
- **Aucun secret dans le dépôt**, et aucun jeton ni clé API dans SQLite, les logs ou la
  synchronisation : ils vont dans le coffre sécurisé de la plateforme.
- **Aucune télémétrie, aucun traceur.** Tout nouvel appel réseau doit être listé dans
  l'écran *Confidentialité*.
- Commits [conventionnels](https://www.conventionalcommits.org/fr/) (`feat:`, `fix:`,
  `docs:`…), versions [SemVer](https://semver.org/lang/fr/), changements notés dans
  `CHANGELOG.md`.

## Licence

En contribuant, vous acceptez que votre contribution soit publiée sous licence
[MPL-2.0](LICENSE).
