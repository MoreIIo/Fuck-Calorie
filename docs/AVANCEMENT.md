# Fuck Calorie — Avancement

> Suivi de ce qui est construit par rapport au [cahier des charges](CAHIER_DES_CHARGES.md).
> Dernière mise à jour : 2026-11-15.

Légende : ✅ fait et testé · 🟡 partiel · ⬜ à faire.

## Vue d'ensemble

| Brique | État | Détail |
|---|---|---|
| Socle du dépôt (workspace, licence, CI) | ✅ | CI : format, clippy, tests sous Linux et Windows |
| `fc-core` — cœur métier | ✅ | 72 tests |
| `fc-storage` — SQLite natif | ✅ | 17 tests ; version web (OPFS) à faire |
| `fc-food-db` — Ciqual + Open Food Facts | ⬜ | |
| `app/` — interface Leptos + Tauri 2 | ⬜ | |
| `fc-sync` — Google Drive | ⬜ | L'horloge HLC et les écritures idempotentes sont déjà en place |

## Par section du cahier des charges

| Section | État | Où |
|---|---|---|
| §2.1 Profil, historisation | ✅ | `core::profile`, `storage::profile` |
| §2.2 BMR Mifflin-St Jeor, TDEE initial | ✅ | `core::energy` |
| §2.3 Poids de tendance, algorithme adaptatif | ✅ | `core::trend`, `core::adaptive` |
| §2.3 Recalcul hebdomadaire planifié, notification, cible figée | ⬜ | relève de l'application |
| §2.4 Modes d'objectif, projection | ✅ | `core::goal` |
| §2.5 Macros et repères par défaut | ✅ | `core::macros` (préréglages V1 à faire) |
| §2.6 Activité : saisie manuelle, MET, calories Strava | ✅ | `core::activity` (connexion Strava : V1) |
| §2.7 Garde-fous santé | ✅ | `core::goal` |
| §2.8 Pesées | 🟡 | modèle et stockage faits ; écran à faire |
| §3.1–3.2 Ciqual embarqué, client et cache Open Food Facts | ⬜ | |
| §3.3 Recherche locale | 🟡 | insensible aux accents et à la casse ; recherche floue à faire |
| §3.4 Quantités et unités | ✅ | `core::food` (table des poids moyens par pièce à faire) |
| §3.5–3.6 Journal, ajout rapide, copie, repas enregistrés | 🟡 | modèle et stockage faits ; écrans à faire ; favoris et recettes à faire |
| §3.8 Modèle de données | ✅ | sauf `Recipe` (V1) |
| §4 Scan | ⬜ | |
| §5 Dashboards | ⬜ | le bilan du jour (`core::journal::day_summary`) est prêt |
| §6 Design system | ⬜ | |
| §7.2 Synchronisation | 🟡 | HLC, tombstones, écritures idempotentes ; journal d'événements et Drive à faire |
| §8.4 Transactions, migrations versionnées | ✅ | `fc-storage` |

## Écarts et précisions par rapport au cahier des charges

1. **Noms des crates.** Les paquets s'appellent `fc-core`, `fc-storage`… (dossiers
   `crates/core`, `crates/storage`). Un paquet nommé `core` entrerait en conflit avec la
   bibliothèque standard de Rust.
2. **Formule adaptative et jours manquants (§2.3).** La formule est appliquée « par jour » :
   apport moyen des jours journalisés − activités moyennes − variation quotidienne de la
   tendance × 7 700. C'est identique à la formule du cahier des charges quand les 21 jours
   sont journalisés ; quand il en manque, l'apport des jours manquants est supposé égal à la
   moyenne des autres (au lieu de zéro).
3. **Plancher calorique et activités (§2.7).** Le plancher s'applique à la cible de base,
   avant l'ajout des calories d'activité.
4. **Âge.** Le profil est refusé en dessous de 18 ans : la formule de Mifflin-St Jeor et les
   garde-fous du §2.7 ne sont pas valables pour les enfants et les adolescents. Le cahier des
   charges ne fixait pas d'âge minimum.
5. **Valeurs nutritionnelles facultatives.** Dans une fiche aliment, chaque nutriment peut
   être « inconnu » (fiches Open Food Facts incomplètes) ; dans le journal, un nutriment
   inconnu compte pour zéro.

## Point à trancher

**Biais de l'algorithme adaptatif pendant les premières semaines (§2.3).** La moyenne mobile
exponentielle (α = 0,1) part du premier poids pesé et met environ cinq semaines à « rattraper »
une perte régulière. Pendant ce temps, la perte mesurée est sous-estimée, donc la dépense
aussi. Simulation avec des données parfaites (vraie dépense 2 300 kcal, apport 1 750 kcal) :

| Semaine | 2 | 3 | 4 | 5 | 6 | 8 |
|---|---|---|---|---|---|---|
| Dépense calculée (kcal) | 2 027 | 2 104 | 2 206 | 2 255 | 2 279 | 2 295 |

L'application baisserait donc la cible d'environ 100 kcal en semaine 2, puis encore 100 en
semaine 3, avant de la remonter. Pistes : (a) ne lancer le premier recalcul qu'après 5
semaines ; (b) mesurer la pente par régression linéaire sur les pesées de la fenêtre, sans
retard ; (c) accepter ce comportement. Le code suit pour l'instant le cahier des charges tel
quel.

## Prochaines étapes proposées

1. **Squelette de l'application** (`app/`) : Leptos + Tauri 2, tokens du design system (§6),
   écran *Aujourd'hui* branché sur `fc-core` et `fc-storage`, build Windows.
2. **Parcours MVP** : profil initial, ajout rapide, pesée, journée terminée.
3. **`fc-food-db`** : import de la table Ciqual, client Open Food Facts avec cache, recherche.
4. **Android** : build APK, scan de code-barres.
5. **Écran *Progrès*** : graphiques poids et calories.
6. **`fc-sync`** : journal d'événements, fusion, Google Drive.
