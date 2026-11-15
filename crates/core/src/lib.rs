//! # fc-core — cœur métier de Fuck Calorie
//!
//! Modèles de données et calculs nutritionnels, **sans interface ni entrées-sorties** :
//! tout ici est du calcul pur, testable sans base de données, sans réseau et sans horloge
//! système (les dates et les horodatages sont toujours passés en paramètre).
//!
//! Les numéros de section (« §2.3 ») renvoient au cahier des charges
//! (`docs/CAHIER_DES_CHARGES.md`).
//!
//! | Module | Contenu | Cahier des charges |
//! |---|---|---|
//! | [`nutrients`] | Les 8 nutriments suivis, additions, contrôle de cohérence | §2.5, §4.3 |
//! | [`profile`] | Profil, sexe pour le calcul, niveau d'activité, historisation | §2.1 |
//! | [`energy`] | BMR (Mifflin-St Jeor) et TDEE initial | §2.2 |
//! | [`goal`] | Modes d'objectif, cible calorique, garde-fous santé, projection | §2.4, §2.7 |
//! | [`macros`] | Répartition par défaut des macros et repères | §2.5 |
//! | [`trend`] | Poids de tendance (moyenne mobile exponentielle) | §2.3, §2.8 |
//! | [`adaptive`] | Algorithme adaptatif (TDEE réel) | §2.3 |
//! | [`activity`] | Activités physiques, estimation MET, calories Strava | §2.6 |
//! | [`food`] | Aliments, portions, unités et conversion en grammes | §3.4, §3.8 |
//! | [`journal`] | Repas, entrées du journal, pesées, bilan du jour | §3.5, §3.8, §5.2 |
//! | [`hlc`] | Horloge logique hybride et métadonnées de synchronisation | §7.2 |

pub mod activity;
pub mod adaptive;
pub mod energy;
pub mod food;
pub mod goal;
pub mod hlc;
pub mod journal;
pub mod macros;
pub mod nutrients;
pub mod profile;
pub mod trend;

pub use goal::{CalorieTarget, Goal};
pub use hlc::{Hlc, HlcClock, SyncMeta};

pub use activity::{Activity, ActivitySource};
pub use food::{Food, FoodSource, Portion, Unit};
pub use journal::{DayStatus, DaySummary, LogEntry, MealSlot, SavedMeal, WeighIn};
pub use macros::MacroTargets;
pub use nutrients::{NutrientFacts, Nutrients};
pub use profile::{ActivityLevel, Profile, ProfileRevision, Sex};
