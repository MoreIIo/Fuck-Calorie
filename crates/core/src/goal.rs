//! Modes d'objectif, cible calorique et garde-fous santé (§2.4, §2.7).

use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::energy::KCAL_PER_KG;
use crate::profile::Sex;

/// Rythmes de perte proposés, en kg par semaine (§2.4).
pub const LOSS_RATES_KG_PER_WEEK: [f64; 4] = [0.25, 0.5, 0.75, 1.0];
/// Rythmes de prise de masse proposés, en kg par semaine (§2.4).
pub const GAIN_RATES_KG_PER_WEEK: [f64; 2] = [0.25, 0.5];
/// Réduction appliquée au TDEE en recomposition (§2.4).
pub const RECOMP_DEFICIT_RATIO: f64 = 0.10;
/// Déficit maximum : 1 % du poids corporel par semaine (§2.7).
pub const MAX_WEEKLY_LOSS_RATIO: f64 = 0.01;
/// Seuil d'IMC sous lequel un poids objectif déclenche un avertissement (§2.7).
pub const UNDERWEIGHT_BMI: f64 = 18.5;
/// Tolérance autour du poids cible en maintien, en kg (§2.4).
pub const MAINTENANCE_TOLERANCE_KG: f64 = 1.0;

/// Mode d'objectif (§2.4).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Goal {
    /// Perte de poids : TDEE − déficit.
    Lose { kg_per_week: f64 },
    /// Maintien : TDEE.
    Maintain,
    /// Prise de masse : TDEE + surplus.
    Gain { kg_per_week: f64 },
    /// Recomposition : TDEE − 10 %, protéines élevées.
    Recomp,
}

impl Goal {
    /// Le rythme est-il un nombre positif raisonnable (au plus 1 kg/semaine) ?
    #[must_use]
    pub fn is_valid(&self) -> bool {
        match *self {
            Self::Lose { kg_per_week } | Self::Gain { kg_per_week } => {
                kg_per_week.is_finite() && kg_per_week > 0.0 && kg_per_week <= 1.0
            }
            Self::Maintain | Self::Recomp => true,
        }
    }

    /// Écart quotidien **demandé** par rapport au TDEE, en kcal (négatif = déficit),
    /// avant garde-fous. 0,5 kg/semaine → −550 kcal/jour.
    #[must_use]
    pub fn daily_delta_kcal(&self, tdee: f64) -> f64 {
        match *self {
            Self::Lose { kg_per_week } => -kg_per_week * KCAL_PER_KG / 7.0,
            Self::Maintain => 0.0,
            Self::Gain { kg_per_week } => kg_per_week * KCAL_PER_KG / 7.0,
            Self::Recomp => -RECOMP_DEFICIT_RATIO * tdee,
        }
    }

    /// Variation de poids visée, en kg par semaine (négatif = perte). `None` si le mode ne
    /// vise pas un rythme (maintien, recomposition).
    #[must_use]
    pub fn target_kg_per_week(&self) -> Option<f64> {
        match *self {
            Self::Lose { kg_per_week } => Some(-kg_per_week),
            Self::Gain { kg_per_week } => Some(kg_per_week),
            Self::Maintain | Self::Recomp => None,
        }
    }
}

/// Plancher calorique par défaut : 1 200 kcal (femme), 1 500 kcal (homme),
/// 1 350 kcal (non précisé) (§2.7).
#[must_use]
pub fn calorie_floor(sex: Sex) -> f64 {
    match sex {
        Sex::Female => 1_200.0,
        Sex::Male => 1_500.0,
        Sex::Unspecified => 1_350.0,
    }
}

/// Déficit quotidien maximum, en kcal : 1 % du poids corporel par semaine (§2.7).
/// Pour 70 kg : 0,7 kg/semaine, soit 770 kcal/jour.
#[must_use]
pub fn max_daily_deficit_kcal(weight_kg: f64) -> f64 {
    MAX_WEEKLY_LOSS_RATIO * weight_kg * KCAL_PER_KG / 7.0
}

/// Cible calorique de base d'une journée (hors activités), avec le détail des garde-fous
/// appliqués pour que l'interface puisse les expliquer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CalorieTarget {
    /// Cible de base retenue, en kcal (avant ajout des activités du jour).
    pub kcal: f64,
    /// Dépense estimée utilisée pour le calcul.
    pub tdee: f64,
    /// Écart demandé par l'objectif (négatif = déficit).
    pub requested_delta: f64,
    /// Écart réellement appliqué après garde-fous.
    pub applied_delta: f64,
    /// Le déficit a été réduit pour ne pas dépasser 1 % du poids par semaine.
    pub deficit_capped: bool,
    /// La cible a été remontée au plancher calorique.
    pub floor_applied: bool,
}

/// Calcule la cible de base : `TDEE + écart de l'objectif`, puis applique les garde-fous
/// du §2.7 (déficit maximum, puis plancher calorique).
///
/// `floor_override` remplace le plancher par défaut (paramètres avancés, avec avertissement).
#[must_use]
pub fn base_target(
    tdee: f64,
    goal: Goal,
    sex: Sex,
    weight_kg: f64,
    floor_override: Option<f64>,
) -> CalorieTarget {
    let requested_delta = goal.daily_delta_kcal(tdee);
    let max_deficit = max_daily_deficit_kcal(weight_kg);
    let deficit_capped = requested_delta < -max_deficit;
    let capped_delta = if deficit_capped {
        -max_deficit
    } else {
        requested_delta
    };

    let floor = floor_override.unwrap_or_else(|| calorie_floor(sex));
    let unfloored = tdee + capped_delta;
    let floor_applied = unfloored < floor;
    let kcal = if floor_applied { floor } else { unfloored };

    CalorieTarget {
        kcal,
        tdee,
        requested_delta,
        applied_delta: kcal - tdee,
        deficit_capped,
        floor_applied,
    }
}

/// Facteur de récupération des calories d'activité, borné à 50–100 % (§2.6).
#[must_use]
pub fn clamp_recovery_factor(factor: f64) -> f64 {
    if factor.is_nan() {
        1.0
    } else {
        factor.clamp(0.5, 1.0)
    }
}

/// Cible du jour (§2.3, point 5) : cible de base + activités enregistrées du jour ×
/// facteur de récupération.
///
/// Le plancher calorique s'applique à la cible de base, **avant** l'ajout des activités :
/// une grosse sortie ne « consomme » jamais le plancher.
#[must_use]
pub fn day_target_kcal(base: &CalorieTarget, activity_kcal: f64, recovery_factor: f64) -> f64 {
    base.kcal + activity_kcal.max(0.0) * clamp_recovery_factor(recovery_factor)
}

/// Indice de masse corporelle : poids (kg) / taille (m)².
#[must_use]
pub fn bmi(weight_kg: f64, height_cm: f64) -> f64 {
    let m = height_cm / 100.0;
    weight_kg / (m * m)
}

/// Le poids objectif correspond-il à un IMC inférieur à 18,5 ? (avertissement du §2.7)
#[must_use]
pub fn goal_is_underweight(goal_weight_kg: f64, height_cm: f64) -> bool {
    bmi(goal_weight_kg, height_cm) < UNDERWEIGHT_BMI
}

/// En maintien, le poids est-il dans la plage de tolérance (± 1 kg) autour du poids cible ?
#[must_use]
pub fn within_maintenance_range(weight_kg: f64, goal_weight_kg: f64) -> bool {
    (weight_kg - goal_weight_kg).abs() <= MAINTENANCE_TOLERANCE_KG
}

/// L'objectif de poids est-il atteint ? (déclenche la proposition de passer en maintien, §2.4)
#[must_use]
pub fn goal_reached(goal: Goal, trend_kg: f64, goal_weight_kg: f64) -> bool {
    match goal {
        Goal::Lose { .. } => trend_kg <= goal_weight_kg,
        Goal::Gain { .. } => trend_kg >= goal_weight_kg,
        Goal::Maintain | Goal::Recomp => false,
    }
}

/// Date estimée d'atteinte du poids objectif (§2.4).
///
/// `kg_per_week` est la variation **réelle** du poids de tendance (négative en perte), ou à
/// défaut le rythme choisi. Renvoie `None` si le poids ne va pas dans la bonne direction, ou
/// si l'échéance dépasse dix ans. Si l'objectif est déjà atteint, renvoie `from`.
#[must_use]
pub fn projected_goal_date(
    from: NaiveDate,
    trend_kg: f64,
    goal_weight_kg: f64,
    kg_per_week: f64,
) -> Option<NaiveDate> {
    let remaining = goal_weight_kg - trend_kg;
    if remaining.abs() < 0.05 {
        return Some(from);
    }
    if !kg_per_week.is_finite() || kg_per_week == 0.0 || remaining.signum() != kg_per_week.signum()
    {
        return None;
    }
    let days = (remaining / kg_per_week * 7.0).ceil();
    if days > 3_650.0 {
        return None;
    }
    // `days` est positif et borné : la conversion est sans perte.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    from.checked_add_days(Days::new(days as u64))
}

/// La perte réelle dépasse-t-elle le rythme choisi depuis 3 semaines de suite ? (alerte §2.7)
///
/// `weekly_trend_deltas_kg` contient la variation du poids de tendance de chaque semaine, de
/// la plus ancienne à la plus récente (négatif = perte).
#[must_use]
pub fn losing_too_fast(weekly_trend_deltas_kg: &[f64], goal: Goal) -> bool {
    let Goal::Lose { kg_per_week } = goal else {
        return false;
    };
    let n = weekly_trend_deltas_kg.len();
    n >= 3
        && weekly_trend_deltas_kg[n - 3..]
            .iter()
            .all(|delta| -delta > kg_per_week + 1e-9)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn deltas_match_spec_table() {
        // §2.4 : 0,25 / 0,5 / 0,75 / 1 kg/sem → −275 / −550 / −825 / −1 100 kcal/j
        let expected = [-275.0, -550.0, -825.0, -1_100.0];
        for (rate, kcal) in LOSS_RATES_KG_PER_WEEK.iter().zip(expected) {
            let delta = Goal::Lose { kg_per_week: *rate }.daily_delta_kcal(2_000.0);
            assert!((delta - kcal).abs() < 1e-9, "{rate} kg/sem");
        }
        // +0,25 / +0,5 kg/sem → +275 / +550 kcal/j
        for (rate, kcal) in GAIN_RATES_KG_PER_WEEK.iter().zip([275.0, 550.0]) {
            let delta = Goal::Gain { kg_per_week: *rate }.daily_delta_kcal(2_000.0);
            assert!((delta - kcal).abs() < 1e-9);
        }
        assert_eq!(Goal::Maintain.daily_delta_kcal(2_000.0), 0.0);
        assert!((Goal::Recomp.daily_delta_kcal(2_000.0) + 200.0).abs() < 1e-9);
    }

    #[test]
    fn goal_validity_and_rate() {
        assert!(Goal::Lose { kg_per_week: 0.5 }.is_valid());
        assert!(!Goal::Lose { kg_per_week: 0.0 }.is_valid());
        assert!(!Goal::Gain { kg_per_week: 1.5 }.is_valid());
        assert!(
            !Goal::Lose {
                kg_per_week: f64::NAN
            }
            .is_valid()
        );
        assert!(Goal::Maintain.is_valid() && Goal::Recomp.is_valid());
        assert_eq!(
            Goal::Lose { kg_per_week: 0.5 }.target_kg_per_week(),
            Some(-0.5)
        );
        assert_eq!(
            Goal::Gain { kg_per_week: 0.25 }.target_kg_per_week(),
            Some(0.25)
        );
        assert_eq!(Goal::Recomp.target_kg_per_week(), None);
    }

    #[test]
    fn floors_match_spec() {
        assert_eq!(calorie_floor(Sex::Female), 1_200.0);
        assert_eq!(calorie_floor(Sex::Male), 1_500.0);
        assert_eq!(calorie_floor(Sex::Unspecified), 1_350.0);
    }

    #[test]
    fn plain_deficit() {
        let t = base_target(
            2_300.0,
            Goal::Lose { kg_per_week: 0.5 },
            Sex::Female,
            70.0,
            None,
        );
        assert!((t.kcal - 1_750.0).abs() < 1e-9);
        assert!(!t.deficit_capped && !t.floor_applied);
        assert!((t.applied_delta + 550.0).abs() < 1e-9);
    }

    #[test]
    fn deficit_capped_at_one_percent_body_weight() {
        // 70 kg → 0,7 kg/semaine au maximum → 770 kcal/jour.
        assert!((max_daily_deficit_kcal(70.0) - 770.0).abs() < 1e-9);
        let t = base_target(
            2_600.0,
            Goal::Lose { kg_per_week: 1.0 },
            Sex::Male,
            70.0,
            None,
        );
        assert!(t.deficit_capped && !t.floor_applied);
        assert!((t.requested_delta + 1_100.0).abs() < 1e-9);
        assert!((t.kcal - 1_830.0).abs() < 1e-9);
        // À 110 kg, 1 kg/semaine reste sous le plafond (1,1 kg/semaine).
        let t = base_target(
            3_000.0,
            Goal::Lose { kg_per_week: 1.0 },
            Sex::Male,
            110.0,
            None,
        );
        assert!(!t.deficit_capped);
        assert!((t.kcal - 1_900.0).abs() < 1e-9);
    }

    #[test]
    fn floor_applies_and_can_be_overridden() {
        let goal = Goal::Lose { kg_per_week: 0.75 };
        let t = base_target(1_700.0, goal, Sex::Female, 60.0, None);
        // Plafond de déficit : 660 kcal → 1 040, remonté au plancher de 1 200.
        assert!(t.deficit_capped && t.floor_applied);
        assert_eq!(t.kcal, 1_200.0);
        assert!((t.applied_delta + 500.0).abs() < 1e-9);
        let t = base_target(1_700.0, goal, Sex::Female, 60.0, Some(1_000.0));
        assert!(!t.floor_applied);
        assert!((t.kcal - 1_040.0).abs() < 1e-9);
        // Le plancher vaut aussi en maintien si la dépense estimée est très basse.
        let t = base_target(1_100.0, Goal::Maintain, Sex::Female, 45.0, None);
        assert!(t.floor_applied && t.kcal == 1_200.0);
    }

    #[test]
    fn gain_and_recomp_targets() {
        let t = base_target(
            2_400.0,
            Goal::Gain { kg_per_week: 0.25 },
            Sex::Male,
            70.0,
            None,
        );
        assert!((t.kcal - 2_675.0).abs() < 1e-9);
        let t = base_target(2_400.0, Goal::Recomp, Sex::Male, 70.0, None);
        assert!((t.kcal - 2_160.0).abs() < 1e-9);
    }

    #[test]
    fn day_target_adds_activity() {
        let base = base_target(
            2_300.0,
            Goal::Lose { kg_per_week: 0.5 },
            Sex::Female,
            70.0,
            None,
        );
        assert!((day_target_kcal(&base, 0.0, 1.0) - 1_750.0).abs() < 1e-9);
        // Sortie vélo de 900 kcal, récupérée à 100 % puis à 50 %.
        assert!((day_target_kcal(&base, 900.0, 1.0) - 2_650.0).abs() < 1e-9);
        assert!((day_target_kcal(&base, 900.0, 0.5) - 2_200.0).abs() < 1e-9);
        // Facteur hors bornes ramené dans 50–100 % ; activité négative ignorée.
        assert!((day_target_kcal(&base, 900.0, 3.0) - 2_650.0).abs() < 1e-9);
        assert!((day_target_kcal(&base, 900.0, 0.1) - 2_200.0).abs() < 1e-9);
        assert!((day_target_kcal(&base, -50.0, 1.0) - 1_750.0).abs() < 1e-9);
        assert_eq!(clamp_recovery_factor(f64::NAN), 1.0);
    }

    #[test]
    fn bmi_and_underweight_warning() {
        assert!((bmi(70.0, 170.0) - 24.221_453).abs() < 1e-5);
        assert!(goal_is_underweight(53.0, 170.0)); // IMC 18,3
        assert!(!goal_is_underweight(54.0, 170.0)); // IMC 18,7
    }

    #[test]
    fn maintenance_and_goal_reached() {
        assert!(within_maintenance_range(62.8, 62.0));
        assert!(!within_maintenance_range(63.2, 62.0));
        let lose = Goal::Lose { kg_per_week: 0.5 };
        assert!(goal_reached(lose, 61.9, 62.0));
        assert!(!goal_reached(lose, 62.4, 62.0));
        assert!(goal_reached(Goal::Gain { kg_per_week: 0.25 }, 75.0, 74.0));
        assert!(!goal_reached(Goal::Maintain, 62.0, 62.0));
    }

    #[test]
    fn projection() {
        let from = date(2026, 11, 15);
        // 8 kg à perdre à 0,5 kg/semaine → 16 semaines.
        assert_eq!(
            projected_goal_date(from, 70.0, 62.0, -0.5),
            Some(date(2027, 3, 7))
        );
        // Prise de masse.
        assert_eq!(
            projected_goal_date(from, 70.0, 72.0, 0.25),
            Some(date(2027, 1, 10))
        );
        // Déjà atteint.
        assert_eq!(projected_goal_date(from, 62.0, 62.0, -0.5), Some(from));
        // Mauvaise direction, poids stable, échéance absurde.
        assert_eq!(projected_goal_date(from, 70.0, 62.0, 0.2), None);
        assert_eq!(projected_goal_date(from, 70.0, 62.0, 0.0), None);
        assert_eq!(projected_goal_date(from, 70.0, 62.0, -0.001), None);
        assert_eq!(projected_goal_date(from, 70.0, 62.0, f64::NAN), None);
    }

    #[test]
    fn too_fast_alert() {
        let goal = Goal::Lose { kg_per_week: 0.5 };
        assert!(losing_too_fast(&[-0.3, -0.8, -0.7, -0.9], goal));
        assert!(!losing_too_fast(&[-0.8, -0.7, -0.4], goal));
        assert!(!losing_too_fast(&[-0.8, -0.7], goal));
        assert!(!losing_too_fast(&[-0.5, -0.5, -0.5], goal)); // pile au rythme choisi
        assert!(!losing_too_fast(&[-0.8, -0.7, -0.9], Goal::Maintain));
    }

    #[test]
    fn serde_shape() {
        let json = serde_json::to_string(&Goal::Lose { kg_per_week: 0.5 }).unwrap();
        assert_eq!(json, r#"{"mode":"lose","kg_per_week":0.5}"#);
        assert_eq!(
            serde_json::to_string(&Goal::Maintain).unwrap(),
            r#"{"mode":"maintain"}"#
        );
        assert_eq!(
            serde_json::from_str::<Goal>(&json).unwrap(),
            Goal::Lose { kg_per_week: 0.5 }
        );
    }
}
