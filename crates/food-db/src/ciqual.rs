//! Table Ciqual (ANSES) embarquée (§3.1) : aliments génériques français, toujours
//! disponibles hors ligne (§3.2).
//!
//! Le fichier `data/ciqual.tsv` est produit par `examples/import_ciqual.rs` à partir de la
//! table officielle ; il est mis à jour à chaque nouvelle édition de la table.

use fc_core::food::{Food, FoodSource};
use fc_core::hlc::{Hlc, SyncMeta};
use fc_core::nutrients::NutrientFacts;

use crate::reference_id;

/// Édition de la table embarquée. Enregistrée dans la base locale : quand elle change, les
/// fiches Ciqual sont réimportées.
pub const VERSION: &str = "2025";

/// Mention obligatoire (Licence Ouverte Etalab, §3.1).
pub const ATTRIBUTION: &str = "Source : Table Ciqual, ANSES";

const DATA: &str = include_str!("../data/ciqual.tsv");

fn number(field: Option<&str>) -> Option<f64> {
    field
        .filter(|text| !text.is_empty())
        .and_then(|text| text.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
}

fn food_from_line(line: &str) -> Option<Food> {
    let mut fields = line.split('\t');
    let code = fields.next().filter(|c| !c.is_empty())?;
    let name = fields.next().filter(|n| !n.is_empty())?;
    let per_100 = NutrientFacts {
        kcal: number(fields.next()),
        protein_g: number(fields.next()),
        carbs_g: number(fields.next()),
        sugars_g: number(fields.next()),
        fat_g: number(fields.next()),
        sat_fat_g: number(fields.next()),
        fiber_g: number(fields.next()),
        salt_g: number(fields.next()),
    };
    Some(Food {
        // Fiches de référence, identiques sur tous les appareils : horodatage zéro.
        meta: SyncMeta::with_id(reference_id(FoodSource::Ciqual, code), Hlc::ZERO),
        source: FoodSource::Ciqual,
        source_ref: Some(code.to_owned()),
        barcode: None,
        name: name.to_owned(),
        brand: None,
        per_100,
        density: None,
        unit_weight_g: None,
        portions: Vec::new(),
        updated_at_ms: 0,
    })
}

/// Les aliments de la table Ciqual, sous forme de fiches.
#[must_use]
pub fn foods() -> Vec<Food> {
    DATA.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(food_from_line)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn table_is_complete_and_well_formed() {
        let foods = foods();
        // Aucune ligne du fichier n'est ignorée.
        let lines = DATA.lines().filter(|l| !l.starts_with('#')).count();
        assert_eq!(foods.len(), lines);
        assert!(foods.len() > 3_000, "{} aliments", foods.len());
        let ids: HashSet<_> = foods.iter().map(|f| f.meta.id).collect();
        assert_eq!(ids.len(), foods.len(), "identifiants en double");
        for food in &foods {
            assert_eq!(food.source, FoodSource::Ciqual);
            assert!(food.per_100.has_energy(), "{} sans énergie", food.name);
            let kcal = food.per_100.kcal.unwrap();
            assert!((0.0..=900.0).contains(&kcal), "{} : {kcal} kcal", food.name);
        }
    }

    #[test]
    fn known_values() {
        let foods = foods();
        let by_code = |code: &str| {
            foods
                .iter()
                .find(|f| f.source_ref.as_deref() == Some(code))
                .unwrap_or_else(|| panic!("code {code} absent"))
        };
        // Huile d'olive vierge extra : presque uniquement des lipides.
        let oil = by_code("17270");
        assert_eq!(oil.name, "Huile d'olive vierge extra");
        assert_eq!(oil.per_100.kcal, Some(899.0));
        assert_eq!(oil.per_100.fat_g, Some(99.9));
        assert_eq!(oil.per_100.carbs_g, Some(0.0));
        // Pastis : « traces » et valeurs sous le seuil de mesure comptent pour zéro.
        let pastis = by_code("1000");
        assert_eq!(pastis.per_100.kcal, Some(274.0));
        assert_eq!(pastis.per_100.protein_g, Some(0.0));
    }

    #[test]
    fn unknown_values_stay_unknown() {
        let food = food_from_line("1\tTest\t100\t\t12.5\t\t\t\t\t").unwrap();
        assert_eq!(food.per_100.kcal, Some(100.0));
        assert_eq!(food.per_100.protein_g, None);
        assert_eq!(food.per_100.carbs_g, Some(12.5));
        assert_eq!(food.per_100.salt_g, None);
        assert!(food_from_line("\tSans code\t1").is_none());
    }
}
