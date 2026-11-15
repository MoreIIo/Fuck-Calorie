# Fuck Calorie

Tracker de calories **gratuit et open source** — Android, iOS, PC et web.

- Sans publicité, sans abonnement, sans fonctionnalité payante.
- **Vos données restent chez vous** : sur vos appareils et, si vous activez la synchronisation,
  dans votre propre Google Drive. Aucun serveur, aucun compte, aucune télémétrie.
- Une cible calorique qui **s'adapte à votre dépense réelle**, calculée à partir de ce que
  vous mangez et de l'évolution de votre poids.

> **État du projet : en cours de construction.** Le cœur métier et le stockage local sont
> écrits et testés ; l'interface n'existe pas encore. Voir [`docs/AVANCEMENT.md`](docs/AVANCEMENT.md).

Le fonctionnement prévu est décrit dans le
[cahier des charges](docs/CAHIER_DES_CHARGES.md).

## Organisation du dépôt

```
crates/
├─ core/      fc-core     Modèles et calculs (BMR, TDEE, macros, algorithme adaptatif…),
│                         sans interface ni entrées-sorties
└─ storage/   fc-storage  Base locale SQLite : schéma, migrations, journal
docs/                     Cahier des charges, avancement, références visuelles
```

À venir : `crates/food-db` (Open Food Facts, Ciqual), `crates/sync` (Google Drive) et `app/`
(interface Leptos + Tauri 2).

## Développer

Prérequis : [Rust](https://rustup.rs) 1.89 ou plus récent, et un compilateur C (sous Windows,
les *Build Tools* de Visual Studio, déjà nécessaires à Rust).

```sh
cargo test --workspace                                  # lancer les tests
cargo clippy --workspace --all-targets -- -D warnings   # vérifier le code
cargo fmt --all                                         # mettre en forme
```

Voir [`CONTRIBUTING.md`](CONTRIBUTING.md) pour les conventions.

### Envoyer sur GitHub (Windows)

```bat
deploy "feat: description du changement"
```

Le script [`scripts/deploy.ps1`](scripts/deploy.ps1) initialise git au premier lancement,
relance les vérifications ci-dessus, montre les fichiers modifiés, demande confirmation, puis
crée le commit et le pousse. Il ne force jamais un envoi.

## Avertissement

Fuck Calorie n'est pas un dispositif médical. Demandez conseil à un professionnel de santé
avant un changement important d'alimentation.

## Licences

- Code : [MPL-2.0](LICENSE).
- Données Open Food Facts : [ODbL](https://opendatacommons.org/licenses/odbl/1-0/).
- Source : Table Ciqual, ANSES — Licence Ouverte Etalab.
- Police Urbanist : SIL OFL 1.1. Icônes Phosphor : MIT.
