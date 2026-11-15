//! Estimation initiale de la dépense énergétique (§2.2).

use crate::profile::{ActivityLevel, Profile, Sex};
use chrono::NaiveDate;

/// Énergie d'un kilogramme de masse corporelle, en kcal (§2.3).
pub const KCAL_PER_KG: f64 = 7_700.0;

/// Métabolisme de base (BMR) par la formule de Mifflin-St Jeor, en kcal/jour.
///
/// `10 × poids + 6,25 × taille − 5 × âge + constante`, la constante valant +5 (homme),
/// −161 (femme) ou −78 (non précisé, moyenne des deux).
#[must_use]
pub fn bmr(sex: Sex, weight_kg: f64, height_cm: f64, age_years: u32) -> f64 {
    let constant = match sex {
        Sex::Male => 5.0,
        Sex::Female => -161.0,
        Sex::Unspecified => -78.0,
    };
    10.0 * weight_kg + 6.25 * height_cm - 5.0 * f64::from(age_years) + constant
}

/// Dépense quotidienne initiale (TDEE) : BMR × multiplicateur d'activité.
///
/// Ne sert que pendant les premiers jours, tant que l'algorithme adaptatif
/// ([`crate::adaptive`]) n'a pas assez de données.
#[must_use]
pub fn initial_tdee(bmr: f64, level: ActivityLevel) -> f64 {
    bmr * level.multiplier()
}

/// TDEE initial d'un profil à la date `on`, avec le poids `weight_kg` (dernière pesée ou
/// poids du profil).
#[must_use]
pub fn initial_tdee_for(profile: &Profile, weight_kg: f64, on: NaiveDate) -> f64 {
    let base = bmr(
        profile.sex,
        weight_kg,
        profile.height_cm,
        profile.age_on(on),
    );
    initial_tdee(base, profile.activity_level)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal::Goal;
    use chrono::Weekday;

    #[test]
    fn mifflin_st_jeor() {
        // 10×70 + 6,25×170 − 5×35 = 1 587,5
        assert_eq!(bmr(Sex::Male, 70.0, 170.0, 35), 1_592.5);
        assert_eq!(bmr(Sex::Female, 70.0, 170.0, 35), 1_426.5);
        assert_eq!(bmr(Sex::Unspecified, 70.0, 170.0, 35), 1_509.5);
        // « Non précisé » est bien la moyenne des deux.
        let mean = (bmr(Sex::Male, 82.3, 181.0, 41) + bmr(Sex::Female, 82.3, 181.0, 41)) / 2.0;
        assert!((bmr(Sex::Unspecified, 82.3, 181.0, 41) - mean).abs() < 1e-9);
    }

    #[test]
    fn initial_tdee_uses_multiplier() {
        assert!((initial_tdee(1_500.0, ActivityLevel::Sedentary) - 1_800.0).abs() < 1e-9);
        assert!((initial_tdee(1_500.0, ActivityLevel::ModeratelyActive) - 2_325.0).abs() < 1e-9);
    }

    #[test]
    fn initial_tdee_for_profile() {
        let profile = Profile {
            sex: Sex::Female,
            birth_date: NaiveDate::from_ymd_opt(1990, 6, 15).unwrap(),
            height_cm: 170.0,
            weight_kg: 70.0,
            goal_weight_kg: 62.0,
            goal: Goal::Maintain,
            activity_level: ActivityLevel::LightlyActive,
            week_start: Weekday::Mon,
        };
        // Âge 36 le 15/11/2026 → BMR = 700 + 1 062,5 − 180 − 161 = 1 421,5
        let on = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        let tdee = initial_tdee_for(&profile, 70.0, on);
        assert!((tdee - 1_421.5 * 1.375).abs() < 1e-9);
    }
}
