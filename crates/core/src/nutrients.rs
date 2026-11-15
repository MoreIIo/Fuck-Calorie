//! Les 8 nutriments suivis (§2.5) : ceux de l'étiquette nutritionnelle européenne.

use std::iter::Sum;
use std::ops::{Add, AddAssign};

use serde::{Deserialize, Serialize};

/// Quantités de nutriments **réellement consommées** (ou cumulées).
///
/// Sert aux totaux et à l'instantané (`nutrients_snapshot`) des entrées du journal :
/// l'historique ne change pas si la fiche de l'aliment est corrigée plus tard.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Nutrients {
    pub kcal: f64,
    pub protein_g: f64,
    pub carbs_g: f64,
    /// Sous-catégorie des glucides.
    pub sugars_g: f64,
    pub fat_g: f64,
    /// Sous-catégorie des lipides.
    pub sat_fat_g: f64,
    pub fiber_g: f64,
    pub salt_g: f64,
}

impl Nutrients {
    pub const ZERO: Self = Self {
        kcal: 0.0,
        protein_g: 0.0,
        carbs_g: 0.0,
        sugars_g: 0.0,
        fat_g: 0.0,
        sat_fat_g: 0.0,
        fiber_g: 0.0,
        salt_g: 0.0,
    };

    /// Multiplie toutes les valeurs par `factor` (ex. 1,25 pour 125 g d'un aliment
    /// décrit pour 100 g).
    #[must_use]
    pub fn scaled(self, factor: f64) -> Self {
        Self {
            kcal: self.kcal * factor,
            protein_g: self.protein_g * factor,
            carbs_g: self.carbs_g * factor,
            sugars_g: self.sugars_g * factor,
            fat_g: self.fat_g * factor,
            sat_fat_g: self.sat_fat_g * factor,
            fiber_g: self.fiber_g * factor,
            salt_g: self.salt_g * factor,
        }
    }

    /// Énergie recalculée à partir des macros : `4×P + 4×G + 9×L + 2×Fibres` (§4.3).
    #[must_use]
    pub fn kcal_from_macros(&self) -> f64 {
        4.0 * self.protein_g + 4.0 * self.carbs_g + 9.0 * self.fat_g + 2.0 * self.fiber_g
    }

    /// Contrôles de cohérence d'une étiquette (§4.3) :
    /// - l'énergie déclarée correspond aux macros à ± `tolerance` (0,15 dans le cahier des
    ///   charges) ;
    /// - les sous-catégories ne dépassent pas leur parent (sucres ≤ glucides, saturés ≤ lipides) ;
    /// - aucune valeur négative.
    #[must_use]
    pub fn is_consistent(&self, tolerance: f64) -> bool {
        let values = [
            self.kcal,
            self.protein_g,
            self.carbs_g,
            self.sugars_g,
            self.fat_g,
            self.sat_fat_g,
            self.fiber_g,
            self.salt_g,
        ];
        if values.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return false;
        }
        if self.sugars_g > self.carbs_g + 1e-9 || self.sat_fat_g > self.fat_g + 1e-9 {
            return false;
        }
        let expected = self.kcal_from_macros();
        // Pour les aliments quasi sans énergie (eau, thé…), l'écart relatif n'a pas de sens.
        if expected < 5.0 && self.kcal < 5.0 {
            return true;
        }
        (self.kcal - expected).abs() <= tolerance * expected.max(self.kcal)
    }
}

impl Add for Nutrients {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self {
            kcal: self.kcal + rhs.kcal,
            protein_g: self.protein_g + rhs.protein_g,
            carbs_g: self.carbs_g + rhs.carbs_g,
            sugars_g: self.sugars_g + rhs.sugars_g,
            fat_g: self.fat_g + rhs.fat_g,
            sat_fat_g: self.sat_fat_g + rhs.sat_fat_g,
            fiber_g: self.fiber_g + rhs.fiber_g,
            salt_g: self.salt_g + rhs.salt_g,
        }
    }
}

impl AddAssign for Nutrients {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sum for Nutrients {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, Add::add)
    }
}

/// Valeurs nutritionnelles d'une **fiche aliment**, pour 100 g (ou 100 ml).
///
/// Chaque valeur est optionnelle : les fiches Open Food Facts sont souvent incomplètes, et
/// « inconnu » n'est pas « zéro » (§3.3 : filtre des produits incomplets ; §4.2 : proposer de
/// scanner l'étiquette pour compléter).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct NutrientFacts {
    pub kcal: Option<f64>,
    pub protein_g: Option<f64>,
    pub carbs_g: Option<f64>,
    pub sugars_g: Option<f64>,
    pub fat_g: Option<f64>,
    pub sat_fat_g: Option<f64>,
    pub fiber_g: Option<f64>,
    pub salt_g: Option<f64>,
}

impl NutrientFacts {
    /// La fiche indique-t-elle au moins les calories ? Sinon elle est inutilisable pour le suivi.
    #[must_use]
    pub fn has_energy(&self) -> bool {
        self.kcal.is_some()
    }

    /// Les 8 nutriments sont-ils tous renseignés ?
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.kcal.is_some()
            && self.protein_g.is_some()
            && self.carbs_g.is_some()
            && self.sugars_g.is_some()
            && self.fat_g.is_some()
            && self.sat_fat_g.is_some()
            && self.fiber_g.is_some()
            && self.salt_g.is_some()
    }

    /// Valeurs pour 100 g, les inconnues comptant pour zéro.
    #[must_use]
    pub fn per_100(&self) -> Nutrients {
        Nutrients {
            kcal: self.kcal.unwrap_or(0.0),
            protein_g: self.protein_g.unwrap_or(0.0),
            carbs_g: self.carbs_g.unwrap_or(0.0),
            sugars_g: self.sugars_g.unwrap_or(0.0),
            fat_g: self.fat_g.unwrap_or(0.0),
            sat_fat_g: self.sat_fat_g.unwrap_or(0.0),
            fiber_g: self.fiber_g.unwrap_or(0.0),
            salt_g: self.salt_g.unwrap_or(0.0),
        }
    }

    /// Nutriments apportés par `grams` grammes de l'aliment.
    #[must_use]
    pub fn for_grams(&self, grams: f64) -> Nutrients {
        self.per_100().scaled(grams / 100.0)
    }
}

impl From<Nutrients> for NutrientFacts {
    fn from(n: Nutrients) -> Self {
        Self {
            kcal: Some(n.kcal),
            protein_g: Some(n.protein_g),
            carbs_g: Some(n.carbs_g),
            sugars_g: Some(n.sugars_g),
            fat_g: Some(n.fat_g),
            sat_fat_g: Some(n.sat_fat_g),
            fiber_g: Some(n.fiber_g),
            salt_g: Some(n.salt_g),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaourt() -> Nutrients {
        // Yaourt nature, pour 100 g (ordre de grandeur d'une étiquette réelle).
        Nutrients {
            kcal: 61.0,
            protein_g: 3.8,
            carbs_g: 5.3,
            sugars_g: 5.3,
            fat_g: 2.7,
            sat_fat_g: 1.8,
            fiber_g: 0.0,
            salt_g: 0.13,
        }
    }

    #[test]
    fn scaling_and_sum() {
        let pot = yaourt().scaled(1.25);
        assert!((pot.kcal - 76.25).abs() < 1e-9);
        let total: Nutrients = [yaourt(), yaourt()].into_iter().sum();
        assert!((total.protein_g - 7.6).abs() < 1e-9);
        let mut acc = Nutrients::ZERO;
        acc += yaourt();
        assert_eq!(acc, yaourt());
    }

    #[test]
    fn energy_from_macros() {
        let n = Nutrients {
            protein_g: 10.0,
            carbs_g: 20.0,
            fat_g: 5.0,
            fiber_g: 3.0,
            ..Nutrients::ZERO
        };
        assert!((n.kcal_from_macros() - 171.0).abs() < 1e-9);
    }

    #[test]
    fn consistency_checks() {
        assert!(yaourt().is_consistent(0.15));
        // Énergie incohérente avec les macros (erreur d'OCR typique : kJ lus comme kcal).
        assert!(
            !Nutrients {
                kcal: 255.0,
                ..yaourt()
            }
            .is_consistent(0.15)
        );
        // Sucres > glucides.
        assert!(
            !Nutrients {
                sugars_g: 9.0,
                ..yaourt()
            }
            .is_consistent(0.15)
        );
        // Saturés > lipides.
        assert!(
            !Nutrients {
                sat_fat_g: 3.0,
                ..yaourt()
            }
            .is_consistent(0.15)
        );
        // Valeur négative ou non finie.
        assert!(
            !Nutrients {
                salt_g: -0.1,
                ..yaourt()
            }
            .is_consistent(0.15)
        );
        assert!(
            !Nutrients {
                kcal: f64::NAN,
                ..yaourt()
            }
            .is_consistent(0.15)
        );
        // Eau : tout à zéro, cohérent.
        assert!(Nutrients::ZERO.is_consistent(0.15));
    }

    #[test]
    fn facts_completeness() {
        let mut facts = NutrientFacts::from(yaourt());
        assert!(facts.is_complete() && facts.has_energy());
        facts.fiber_g = None;
        assert!(!facts.is_complete() && facts.has_energy());
        assert_eq!(facts.per_100().fiber_g, 0.0);
        assert!(!NutrientFacts::default().has_energy());
        assert!((facts.for_grams(125.0).kcal - 76.25).abs() < 1e-9);
    }
}
