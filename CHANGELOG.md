# Journal des modifications

Format inspiré de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/) ; le projet suit
[SemVer](https://semver.org/lang/fr/).

## [Non publié]

### Ajouté
- Workspace Cargo, licence MPL-2.0, intégration continue (format, clippy, tests sous Linux et
  Windows).
- `fc-core` : nutriments, profil historisé, BMR Mifflin-St Jeor et TDEE initial, modes
  d'objectif, cible calorique avec garde-fous santé, macros par défaut, poids de tendance,
  algorithme adaptatif, activités (MET, Strava), aliments et conversion des quantités,
  journal et bilan du jour, horloge logique hybride.
- `fc-storage` : base SQLite, migrations versionnées, écritures idempotentes, recherche
  d'aliments insensible aux accents, journal, pesées, activités, profil, réglages.
