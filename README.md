# Fuck Calorie

Tracker de calories **gratuit et open source** — Android, iOS, PC et web.

- Sans publicité, sans abonnement, sans fonctionnalité payante.
- **Vos données restent chez vous** : sur vos appareils et, si vous activez la synchronisation,
  dans votre propre Google Drive. Aucun serveur, aucun compte, aucune télémétrie.
- Une cible calorique qui **s'adapte à votre dépense réelle**, calculée à partir de ce que
  vous mangez et de l'évolution de votre poids.

> **État du projet : en cours de construction.** Première version de l'application de bureau :
> profil, écran *Aujourd'hui*, recherche d'aliments (Ciqual et Open Food Facts), ajout rapide,
> pesées. Voir [`docs/AVANCEMENT.md`](docs/AVANCEMENT.md).

Le fonctionnement prévu est décrit dans le
[cahier des charges](docs/CAHIER_DES_CHARGES.md).

## Organisation du dépôt

```
crates/
├─ core/      fc-core     Modèles et calculs (BMR, TDEE, macros, algorithme adaptatif…),
│                         sans interface ni entrées-sorties
├─ food-db/   fc-food-db  Table Ciqual embarquée, client Open Food Facts
├─ storage/   fc-storage  Base locale SQLite : schéma, migrations, journal
└─ service/   fc-service  Opérations de l'application (profil, journal, aliments, pesées)
app/          fc-app      Interface (Leptos → WebAssembly), design system
└─ src-tauri/ fc-desktop  Application de bureau (Tauri 2) : fenêtre, base, commandes
docs/                     Cahier des charges, avancement, références visuelles
```

À venir : `crates/sync` (Google Drive).

## Lancer l'application (Windows)

Prérequis : [Rust](https://rustup.rs) 1.90 ou plus récent, et les *Build Tools* de Visual
Studio (déjà nécessaires à Rust). WebView2 est inclus dans Windows 10 et 11.

Le plus simple : double-cliquer sur **`lancer.cmd`** à la racine du dépôt. Il installe ce
qui manque la première fois, puis ouvre l'application. `lancer installer` construit
l'installateur Windows.

À la main : une seule fois, installer la cible WebAssembly et les deux outils de build :

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
cargo install tauri-cli --version "^2" --locked
```

Puis, à chaque fois :

```sh
cd app
cargo tauri dev
```

La première compilation prend plusieurs minutes. Ensuite, chaque modification du code
recharge l'application. Pour produire un installateur Windows (`.msi` et `.exe` dans
`target/release/bundle/`) : `cargo tauri build`, toujours depuis `app`.

Les données sont dans `%APPDATA%\io.github.moreiio.fuckcalorie\fuck-calorie.db`.

## Tester sur Android

À chaque envoi sur `main`, GitHub construit un APK de test (workflow
[`android.yml`](.github/workflows/android.yml)). Depuis le téléphone, ouvrir la page
**Releases** du dépôt, pré-version « APK de test », télécharger `fuck-calorie-test.apk` et
l'installer (Android demande d'autoriser l'installation depuis le navigateur).

Sans clé de signature enregistrée dans les secrets du dépôt (`ANDROID_KEYSTORE`,
`ANDROID_KEYSTORE_PASSWORD`), chaque APK est signé avec une clé différente : il faut
désinstaller l'ancien avant d'installer le nouveau, ce qui efface les données du téléphone.

## Tester sur iPhone

À chaque envoi sur `main`, GitHub construit aussi une IPA de test **non signée** (workflow
[`ios.yml`](.github/workflows/ios.yml)), publiée dans la pré-version « IPA de test » de la
page **Releases**. Une IPA non signée ne s'installe pas directement : on la signe avec son
identifiant Apple gratuit, depuis un PC Windows.

1. Sur le PC : installer [iTunes](https://www.apple.com/fr/itunes/) (version du site d'Apple,
   pas celle du Microsoft Store) et [Sideloadly](https://sideloadly.io).
2. Télécharger `fuck-calorie-test.ipa` depuis la page Releases.
3. Brancher l'iPhone en USB, accepter « Faire confiance à cet ordinateur ».
4. Dans Sideloadly : glisser l'IPA, choisir l'iPhone, saisir l'identifiant Apple, **Start**.
5. Sur l'iPhone : *Réglages → Général → VPN et gestion de l'appareil* → faire confiance au
   profil de son identifiant Apple ; puis *Réglages → Confidentialité et sécurité → Mode
   développeur* → activer (l'iPhone redémarre).

Avec un identifiant Apple gratuit, l'application **expire au bout de 7 jours** : refaire
l'étape 4 (les données de l'application sont conservées tant qu'on ne la désinstalle pas).

## Développer

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
