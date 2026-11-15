//! Répartition par défaut des macros et repères nutritionnels (§2.5).

use serde::{Deserialize, Serialize};

use crate::goal::Goal;

/// Protéines par défaut, en g par kg de poids actuel.
pub const PROTEIN_G_PER_KG: f64 = 1.8;
/// Protéines en recomposition, en g par kg.
pub const PROTEIN_G_PER_KG_RECOMP: f64 = 2.0;
/// Part des lipides dans les calories.
pub const FAT_ENERGY_RATIO: f64 = 0.30;
/// Minimum de lipides, en g par kg.
pub const FAT_MIN_G_PER_KG: f64 = 0.8;
/// Fibres : minimum quotidien, en g.
pub const FIBER_MIN_G: f64 = 30.0;
/// Sucres : maximum indicatif, en g.
pub const SUGARS_MAX_G: f64 = 50.0;
/// Graisses saturées : part maximale des calories.
pub const SAT_FAT_MAX_ENERGY_RATIO: f64 = 0.10;
/// Sel : maximum quotidien (OMS), en g.
pub const SALT_MAX_G: f64 = 5.0;

/// Cibles et repères d'une journée.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MacroTargets {
    /// Cible.
    pub protein_g: f64,
    /// Cible.
    pub fat_g: f64,
    /// Cible : le reste des calories.
    pub carbs_g: f64,
    /// Minimum.
    pub fiber_min_g: f64,
    /// Maximum indicatif.
    pub sugars_max_g: f64,
    /// Maximum.
    pub sat_fat_max_g: f64,
    /// Maximum.
    pub salt_max_g: f64,
}

/// Répartition par défaut (préréglage *Équilibré*) pour une cible de `kcal_target` kcal.
///
/// - Protéines : 1,8 g/kg de poids actuel (2,0 en recomposition).
/// - Lipides : 30 % des calories, avec un minimum de 0,8 g/kg.
/// - Glucides : le reste (jamais négatif).
#[must_use]
pub fn default_macro_targets(kcal_target: f64, weight_kg: f64, goal: Goal) -> MacroTargets {
    let protein_per_kg = if matches!(goal, Goal::Recomp) {
        PROTEIN_G_PER_KG_RECOMP
    } else {
        PROTEIN_G_PER_KG
    };
    let protein_g = protein_per_kg * weight_kg;
    let fat_g = (FAT_ENERGY_RATIO * kcal_target / 9.0).max(FAT_MIN_G_PER_KG * weight_kg);
    let carbs_g = ((kcal_target - 4.0 * protein_g - 9.0 * fat_g) / 4.0).max(0.0);
    MacroTargets {
        protein_g,
        fat_g,
        carbs_g,
        fiber_min_g: FIBER_MIN_G,
        sugars_max_g: SUGARS_MAX_G,
        sat_fat_max_g: SAT_FAT_MAX_ENERGY_RATIO * kcal_target / 9.0,
        salt_max_g: SALT_MAX_G,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced_defaults() {
        let t = default_macro_targets(1_800.0, 70.0, Goal::Lose { kg_per_week: 0.5 });
        assert!((t.protein_g - 126.0).abs() < 1e-9);
        assert!((t.fat_g - 60.0).abs() < 1e-9); // 30 % de 1 800 = 540 kcal
        assert!((t.carbs_g - 189.0).abs() < 1e-9); // (1 800 − 504 − 540) / 4
        assert!((t.sat_fat_max_g - 20.0).abs() < 1e-9);
        assert_eq!(
            (t.fiber_min_g, t.sugars_max_g, t.salt_max_g),
            (30.0, 50.0, 5.0)
        );
        // Les trois macros retombent sur la cible calorique.
        let kcal = 4.0 * t.protein_g + 4.0 * t.carbs_g + 9.0 * t.fat_g;
        assert!((kcal - 1_800.0).abs() < 1e-9);
    }

    #[test]
    fn recomp_has_more_protein() {
        let t = default_macro_targets(2_160.0, 70.0, Goal::Recomp);
        assert!((t.protein_g - 140.0).abs() < 1e-9);
    }

    #[test]
    fn fat_minimum_per_kg() {
        // 30 % de 1 200 kcal = 40 g, mais le minimum pour 90 kg est 72 g.
        let t = default_macro_targets(1_200.0, 90.0, Goal::Lose { kg_per_week: 0.5 });
        assert!((t.fat_g - 72.0).abs() < 1e-9);
        // Protéines 648 kcal + lipides 648 kcal > 1 200 : glucides à zéro, pas négatifs.
        assert_eq!(t.carbs_g, 0.0);
    }
}
