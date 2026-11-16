//! # fc-food-db — base de produits de Fuck Calorie
//!
//! | Module | Contenu | Cahier des charges |
//! |---|---|---|
//! | [`ciqual`] | Table Ciqual (ANSES) embarquée : ~3 300 aliments génériques français | §3.1 |
//! | [`off`] | Lecture des réponses d'Open Food Facts, adresses de l'API | §3.1, §3.2 |
//! | [`rate`] | Limiteur de débit (respect des limites d'Open Food Facts) | §3.1 |
//! | [`client`] | Client HTTP Open Food Facts (fonctionnalité `http`) | §3.1 |
//!
//! Sans la fonctionnalité `http`, la crate ne fait aucune entrée-sortie : elle transforme des
//! données (table embarquée, JSON reçu) en fiches [`fc_core::Food`].
//!
//! Licences des données : Ciqual — Licence Ouverte Etalab (« Source : Table Ciqual, ANSES ») ;
//! Open Food Facts — ODbL (« Données Open Food Facts ») (§8.10).

pub mod ciqual;
#[cfg(feature = "http")]
pub mod client;
pub mod off;
pub mod rate;

use fc_core::food::FoodSource;
use uuid::Uuid;

/// Espace de noms des identifiants déterministes de Fuck Calorie (UUID v5).
const NAMESPACE: Uuid = Uuid::from_u128(0x6675_636b_2d63_616c_6f72_6965_2d66_6462);

/// Identifiant **déterministe** d'une fiche de référence (Ciqual, Open Food Facts), calculé
/// à partir de sa source et de son identifiant dans cette source.
///
/// Le cache de produits n'est pas synchronisé (§3.2), mais les entrées du journal le sont et
/// désignent leur aliment par identifiant : avec un identifiant déterministe, le même
/// produit a le même identifiant sur tous les appareils.
#[must_use]
pub fn reference_id(source: FoodSource, source_ref: &str) -> Uuid {
    let prefix = match source {
        FoodSource::Ciqual => "ciqual",
        FoodSource::Off => "off",
        FoodSource::Custom => "custom",
        FoodSource::Recipe => "recipe",
    };
    Uuid::new_v5(&NAMESPACE, format!("{prefix}:{source_ref}").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_ids_are_stable_and_distinct() {
        let a = reference_id(FoodSource::Off, "3017620422003");
        assert_eq!(a, reference_id(FoodSource::Off, "3017620422003"));
        assert_ne!(a, reference_id(FoodSource::Off, "3017620422004"));
        assert_ne!(a, reference_id(FoodSource::Ciqual, "3017620422003"));
        // Valeur figée : la changer casserait le lien entre le journal synchronisé et les
        // fiches des autres appareils.
        assert_eq!(a.get_version_num(), 5);
        assert_eq!(
            reference_id(FoodSource::Ciqual, "13050").to_string(),
            CIQUAL_13050
        );
    }

    const CIQUAL_13050: &str = "f6ac681c-01cb-5b1d-9b63-ae508f66eaaf";
}
