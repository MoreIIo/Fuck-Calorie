//! Profil de l'utilisatrice (§2.1).

use chrono::{Datelike, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

use crate::goal::Goal;
use crate::hlc::SyncMeta;

/// Sexe **pour le calcul** du métabolisme de base (§2.1). « Non précisé » utilise la moyenne
/// des deux constantes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sex {
    Male,
    Female,
    Unspecified,
}

/// Niveau d'activité de base, utilisé pour l'estimation initiale de la dépense (§2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityLevel {
    Sedentary,
    LightlyActive,
    ModeratelyActive,
    VeryActive,
    ExtremelyActive,
}

impl ActivityLevel {
    pub const ALL: [Self; 5] = [
        Self::Sedentary,
        Self::LightlyActive,
        Self::ModeratelyActive,
        Self::VeryActive,
        Self::ExtremelyActive,
    ];

    /// Multiplicateur appliqué au métabolisme de base (§2.2).
    #[must_use]
    pub fn multiplier(self) -> f64 {
        match self {
            Self::Sedentary => 1.2,
            Self::LightlyActive => 1.375,
            Self::ModeratelyActive => 1.55,
            Self::VeryActive => 1.725,
            Self::ExtremelyActive => 1.9,
        }
    }
}

/// Données du profil nécessaires aux calculs (champs MVP du §2.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub sex: Sex,
    pub birth_date: NaiveDate,
    pub height_cm: f64,
    /// Poids au moment de la saisie du profil (kg). Le suivi au quotidien passe par les pesées.
    pub weight_kg: f64,
    pub goal_weight_kg: f64,
    pub goal: Goal,
    pub activity_level: ActivityLevel,
    /// Premier jour de la semaine pour les bilans hebdomadaires (lundi par défaut).
    pub week_start: Weekday,
}

/// Raison pour laquelle un profil est refusé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProfileError {
    #[error("la taille doit être comprise entre 100 et 250 cm")]
    Height,
    #[error("le poids doit être compris entre 30 et 400 kg")]
    Weight,
    #[error("le poids objectif doit être compris entre 30 et 400 kg")]
    GoalWeight,
    #[error("la date de naissance doit correspondre à un âge de 18 à 120 ans")]
    BirthDate,
    #[error("le rythme de l'objectif n'est pas valide")]
    Goal,
}

impl Profile {
    /// Âge en années révolues à la date `on`.
    #[must_use]
    pub fn age_on(&self, on: NaiveDate) -> u32 {
        age_on(self.birth_date, on)
    }

    /// Vérifie que les valeurs sont plausibles. `today` sert à calculer l'âge.
    ///
    /// L'application s'adresse aux adultes : les formules et les garde-fous du cahier des
    /// charges (Mifflin-St Jeor, planchers caloriques, seuils d'IMC) ne sont pas valables
    /// pour les enfants et les adolescents.
    pub fn validate(&self, today: NaiveDate) -> Result<(), ProfileError> {
        if !(100.0..=250.0).contains(&self.height_cm) {
            return Err(ProfileError::Height);
        }
        if !(30.0..=400.0).contains(&self.weight_kg) {
            return Err(ProfileError::Weight);
        }
        if !(30.0..=400.0).contains(&self.goal_weight_kg) {
            return Err(ProfileError::GoalWeight);
        }
        if self.birth_date > today || !(18..=120).contains(&self.age_on(today)) {
            return Err(ProfileError::BirthDate);
        }
        if !self.goal.is_valid() {
            return Err(ProfileError::Goal);
        }
        Ok(())
    }
}

/// Âge en années révolues à la date `on` (0 si `on` précède la naissance).
#[must_use]
pub fn age_on(birth_date: NaiveDate, on: NaiveDate) -> u32 {
    if on < birth_date {
        return 0;
    }
    let mut years = on.year() - birth_date.year();
    if (on.month(), on.day()) < (birth_date.month(), birth_date.day()) {
        years -= 1;
    }
    u32::try_from(years).unwrap_or(0)
}

/// Version datée du profil. Toute modification crée une nouvelle révision, pour que les
/// anciens jours gardent leur cible d'origine (§2.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileRevision {
    #[serde(flatten)]
    pub meta: SyncMeta,
    /// Premier jour auquel cette version du profil s'applique.
    pub effective_from: NaiveDate,
    pub profile: Profile,
}

/// Profil en vigueur à la date `on` : la révision la plus récente dont la date d'effet est
/// passée. Si plusieurs révisions ont la même date d'effet, la dernière modifiée gagne.
#[must_use]
pub fn profile_on(revisions: &[ProfileRevision], on: NaiveDate) -> Option<&Profile> {
    revisions
        .iter()
        .filter(|r| !r.meta.deleted && r.effective_from <= on)
        .max_by_key(|r| (r.effective_from, r.meta.hlc))
        .map(|r| &r.profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hlc::{Hlc, HlcClock};
    use uuid::Uuid;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    pub(crate) fn sample() -> Profile {
        Profile {
            sex: Sex::Female,
            birth_date: date(1990, 6, 15),
            height_cm: 170.0,
            weight_kg: 70.0,
            goal_weight_kg: 62.0,
            goal: Goal::Lose { kg_per_week: 0.5 },
            activity_level: ActivityLevel::Sedentary,
            week_start: Weekday::Mon,
        }
    }

    #[test]
    fn age_counts_completed_years() {
        let p = sample();
        assert_eq!(p.age_on(date(2026, 6, 14)), 35);
        assert_eq!(p.age_on(date(2026, 6, 15)), 36);
        assert_eq!(p.age_on(date(1980, 1, 1)), 0);
        // Né un 29 février.
        assert_eq!(age_on(date(2000, 2, 29), date(2025, 2, 28)), 24);
        assert_eq!(age_on(date(2000, 2, 29), date(2025, 3, 1)), 25);
    }

    #[test]
    fn multipliers_match_spec() {
        let m: Vec<f64> = ActivityLevel::ALL.iter().map(|l| l.multiplier()).collect();
        assert_eq!(m, vec![1.2, 1.375, 1.55, 1.725, 1.9]);
    }

    #[test]
    fn validation() {
        let today = date(2026, 11, 15);
        assert_eq!(sample().validate(today), Ok(()));
        assert_eq!(
            Profile {
                height_cm: 17.0,
                ..sample()
            }
            .validate(today),
            Err(ProfileError::Height)
        );
        assert_eq!(
            Profile {
                weight_kg: 700.0,
                ..sample()
            }
            .validate(today),
            Err(ProfileError::Weight)
        );
        assert_eq!(
            Profile {
                goal_weight_kg: 6.2,
                ..sample()
            }
            .validate(today),
            Err(ProfileError::GoalWeight)
        );
        assert_eq!(
            Profile {
                birth_date: date(2030, 1, 1),
                ..sample()
            }
            .validate(today),
            Err(ProfileError::BirthDate)
        );
        assert_eq!(
            Profile {
                birth_date: date(2015, 1, 1),
                ..sample()
            }
            .validate(today),
            Err(ProfileError::BirthDate)
        );
        assert_eq!(
            Profile {
                goal: Goal::Lose { kg_per_week: -1.0 },
                ..sample()
            }
            .validate(today),
            Err(ProfileError::Goal)
        );
    }

    #[test]
    fn revisions_keep_history() {
        let mut clock = HlcClock::new(Uuid::from_u128(1));
        let rev = |from, weight, hlc: Hlc| ProfileRevision {
            meta: SyncMeta::new(hlc),
            effective_from: from,
            profile: Profile {
                weight_kg: weight,
                ..sample()
            },
        };
        let mut revisions = vec![
            rev(date(2026, 1, 1), 75.0, clock.tick(1)),
            rev(date(2026, 3, 1), 72.0, clock.tick(2)),
            rev(date(2026, 3, 1), 71.5, clock.tick(3)), // correction le même jour
        ];
        assert!(profile_on(&revisions, date(2025, 12, 31)).is_none());
        assert_eq!(
            profile_on(&revisions, date(2026, 2, 10)).unwrap().weight_kg,
            75.0
        );
        assert_eq!(
            profile_on(&revisions, date(2026, 3, 1)).unwrap().weight_kg,
            71.5
        );
        // Une révision supprimée est ignorée.
        revisions[2].meta.delete(clock.tick(4));
        assert_eq!(
            profile_on(&revisions, date(2026, 6, 1)).unwrap().weight_kg,
            72.0
        );
    }

    #[test]
    fn serde_round_trip() {
        let mut clock = HlcClock::new(Uuid::from_u128(1));
        let rev = ProfileRevision {
            meta: SyncMeta::new(clock.tick(1)),
            effective_from: date(2026, 1, 1),
            profile: sample(),
        };
        let json = serde_json::to_string(&rev).unwrap();
        assert!(json.contains("\"sex\":\"female\""));
        assert_eq!(serde_json::from_str::<ProfileRevision>(&json).unwrap(), rev);
    }
}
