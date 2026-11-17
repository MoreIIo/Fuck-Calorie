//! Open Food Facts (§3.1, §3.2) : adresses de l'API et lecture des réponses.
//!
//! Aucune entrée-sortie ici : ce module construit des adresses et transforme le JSON reçu en
//! fiches [`Food`]. L'envoi des requêtes est dans [`crate::client`].
//!
//! - Lecture d'un produit : API v2, `/api/v2/product/<code-barres>`.
//! - Recherche par texte : `/cgi/search.pl` (la recherche de l'API v2 ne sait pas chercher
//!   par texte), limitée aux produits vendus en France et triée par popularité (§3.3).
//! - Le champ `fields` réduit la réponse à ce dont l'application a besoin.

use std::fmt::Write as _;

use fc_core::food::{Food, FoodSource, Portion};
use fc_core::hlc::{Hlc, SyncMeta};
use fc_core::nutrients::NutrientFacts;
use serde_json::Value;

use crate::reference_id;

/// Serveur d'Open Food Facts.
pub const HOST: &str = "https://world.openfoodfacts.org";

/// Mention obligatoire (licence ODbL, §3.1).
pub const ATTRIBUTION: &str = "Données Open Food Facts";

/// Une fiche en cache est rafraîchie au bout de 30 jours (§3.2).
pub const CACHE_MAX_AGE_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Nombre de résultats demandés par recherche.
pub const SEARCH_PAGE_SIZE: u32 = 20;

/// Champs demandés à Open Food Facts.
const FIELDS: &str = "code,product_name,product_name_fr,generic_name_fr,generic_name,brands,\
                      product_quantity_unit,serving_size,serving_quantity,\
                      serving_quantity_unit,nutriments";

/// Énergie maximale plausible pour 100 g (l'huile pure fait 900 kcal) : au-delà, la valeur
/// est une erreur de saisie et la fiche est traitée comme « sans calories ».
const MAX_KCAL_100: f64 = 1_000.0;

/// Erreur d'un échange avec Open Food Facts. Les messages sont affichables tels quels.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum OffError {
    #[error("ce code-barres n'est pas valide")]
    InvalidBarcode,
    #[error("trop de requêtes vers Open Food Facts : réessaie dans {wait_s} s")]
    RateLimited { wait_s: i64 },
    #[error("Open Food Facts est injoignable (vérifie ta connexion)")]
    Network,
    #[error("Open Food Facts a répondu par une erreur ({0})")]
    Server(u16),
    #[error("la réponse d'Open Food Facts est illisible")]
    InvalidResponse,
}

/// En-tête `User-Agent` exigé par Open Food Facts : `FuckCalorie/<version> (<contact>)`.
#[must_use]
pub fn user_agent(version: &str) -> String {
    format!("FuckCalorie/{version} (https://github.com/MoreIIo/Fuck-Calorie)")
}

pub use fc_core::food::is_barcode;

/// Encode un texte pour une adresse (`café au lait` → `caf%C3%A9%20au%20lait`).
fn url_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Adresse de lecture d'un produit par code-barres.
pub fn product_url(barcode: &str) -> Result<String, OffError> {
    if !is_barcode(barcode) {
        return Err(OffError::InvalidBarcode);
    }
    Ok(format!("{HOST}/api/v2/product/{barcode}?fields={FIELDS}"))
}

/// Adresse de recherche par texte : produits vendus en France, les plus scannés d'abord.
#[must_use]
pub fn search_url(query: &str) -> String {
    format!(
        "{HOST}/cgi/search.pl?search_terms={}&search_simple=1&action=process&json=1\
         &tagtype_0=countries&tag_contains_0=contains&tag_0=france\
         &sort_by=unique_scans_n&page_size={SEARCH_PAGE_SIZE}&fields={FIELDS}",
        url_encode(query.trim())
    )
}

/// Nombre lu dans le JSON : Open Food Facts renvoie tantôt un nombre, tantôt du texte.
fn number(value: Option<&Value>) -> Option<f64> {
    let number = match value? {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => s.trim().replace(',', ".").parse::<f64>().ok()?,
        _ => return None,
    };
    (number.is_finite() && number >= 0.0).then_some(number)
}

fn text<'a>(product: &'a Value, key: &str) -> Option<&'a str> {
    product
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Les 8 nutriments pour 100 g (ou 100 ml), à partir du champ `nutriments`.
fn nutrient_facts(nutriments: Option<&Value>) -> NutrientFacts {
    let get = |key: &str| number(nutriments.and_then(|n| n.get(key)));
    // Énergie : en kcal si elle est indiquée, sinon convertie depuis les kJ.
    let kcal = get("energy-kcal_100g")
        .or_else(|| get("energy-kj_100g").map(|kj| kj / 4.184))
        .or_else(|| get("energy_100g").map(|kj| kj / 4.184))
        .filter(|kcal| *kcal <= MAX_KCAL_100)
        .map(|kcal| (kcal * 10.0).round() / 10.0);
    // Sel : à défaut, sodium × 2,5 (§4.3).
    let salt_g = get("salt_100g").or_else(|| get("sodium_100g").map(|sodium| sodium * 2.5));
    NutrientFacts {
        kcal,
        protein_g: get("proteins_100g"),
        carbs_g: get("carbohydrates_100g"),
        sugars_g: get("sugars_100g"),
        fat_g: get("fat_100g"),
        sat_fat_g: get("saturated-fat_100g"),
        fiber_g: get("fiber_100g"),
        salt_g,
    }
}

/// Transforme un produit Open Food Facts en fiche. `None` si le produit n'a ni code-barres
/// valide ni nom : il ne pourrait être ni retrouvé ni affiché.
#[must_use]
pub fn food_from_product(product: &Value, now_ms: i64) -> Option<Food> {
    let code = match product.get("code")? {
        Value::String(code) => code.trim().to_owned(),
        Value::Number(code) => code.to_string(),
        _ => return None,
    };
    if !is_barcode(&code) {
        return None;
    }
    let name = [
        "product_name_fr",
        "product_name",
        "generic_name_fr",
        "generic_name",
    ]
    .into_iter()
    .find_map(|key| text(product, key))?;
    // Plusieurs marques sont séparées par des virgules : la première suffit.
    let brand = text(product, "brands")
        .and_then(|brands| brands.split(',').map(str::trim).find(|b| !b.is_empty()));

    let serving_unit = text(product, "serving_quantity_unit").map(str::to_lowercase);
    let liquid = [
        text(product, "product_quantity_unit"),
        serving_unit.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|unit| unit.eq_ignore_ascii_case("ml"));
    // Portion : seulement si sa quantité est connue en grammes ou en millilitres.
    let portions = number(product.get("serving_quantity"))
        .filter(|quantity| *quantity > 0.0)
        .filter(|_| matches!(serving_unit.as_deref(), None | Some("g" | "ml")))
        .map(|grams| Portion {
            label: "1 portion".to_owned(),
            grams,
        })
        .into_iter()
        .collect();

    Some(Food {
        // L'horodatage est posé par l'application au moment de l'enregistrement en cache.
        meta: SyncMeta::with_id(reference_id(FoodSource::Off, &code), Hlc::ZERO),
        source: FoodSource::Off,
        source_ref: Some(code.clone()),
        barcode: Some(code),
        name: name.to_owned(),
        brand: brand.map(str::to_owned),
        per_100: nutrient_facts(product.get("nutriments")),
        // Liquide : les valeurs sont pour 100 ml, densité 1 par défaut (§3.4).
        density: liquid.then_some(1.0),
        unit_weight_g: None,
        portions,
        updated_at_ms: now_ms,
    })
}

/// Lit la réponse à une lecture de produit. `Ok(None)` : produit inconnu d'Open Food Facts.
pub fn parse_product_response(json: &str, now_ms: i64) -> Result<Option<Food>, OffError> {
    let value: Value = serde_json::from_str(json).map_err(|_| OffError::InvalidResponse)?;
    match value.get("product") {
        Some(product) if product.is_object() => Ok(food_from_product(product, now_ms)),
        // Produit inconnu : `status: 0`, sans champ `product`.
        _ if value.get("status").is_some() => Ok(None),
        _ => Err(OffError::InvalidResponse),
    }
}

/// Lit la réponse à une recherche. Les produits inutilisables (sans nom, sans code-barres)
/// sont ignorés.
pub fn parse_search_response(json: &str, now_ms: i64) -> Result<Vec<Food>, OffError> {
    let value: Value = serde_json::from_str(json).map_err(|_| OffError::InvalidResponse)?;
    let products = value
        .get("products")
        .and_then(Value::as_array)
        .ok_or(OffError::InvalidResponse)?;
    Ok(products
        .iter()
        .filter_map(|product| food_from_product(product, now_ms))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_core::food::Unit;

    /// Réponse de l'API v2 pour un produit, réduite aux champs demandés.
    const PRODUCT: &str = r#"{
        "code": "3033490004743",
        "product": {
            "code": "3033490004743",
            "product_name": "Plain yogurt",
            "product_name_fr": "Yaourt nature",
            "brands": "Marque, Sous-marque",
            "product_quantity_unit": "g",
            "serving_size": "125 g",
            "serving_quantity": 125,
            "serving_quantity_unit": "g",
            "nutriments": {
                "energy-kcal_100g": 61,
                "energy-kj_100g": 255,
                "energy_100g": 255,
                "proteins_100g": 3.8,
                "carbohydrates_100g": 5.3,
                "sugars_100g": 5.3,
                "fat_100g": 2.7,
                "saturated-fat_100g": 1.8,
                "salt_100g": 0.13,
                "sodium_100g": 0.052
            }
        },
        "status": 1,
        "status_verbose": "product found"
    }"#;

    #[test]
    fn product_becomes_a_food() {
        let food = parse_product_response(PRODUCT, 42).unwrap().unwrap();
        assert_eq!(food.source, FoodSource::Off);
        assert_eq!(food.barcode.as_deref(), Some("3033490004743"));
        assert_eq!(food.source_ref, food.barcode);
        assert_eq!(food.meta.id, reference_id(FoodSource::Off, "3033490004743"));
        // Le nom français passe avant le nom principal ; une seule marque.
        assert_eq!(food.name, "Yaourt nature");
        assert_eq!(food.brand.as_deref(), Some("Marque"));
        assert_eq!(food.per_100.kcal, Some(61.0));
        assert_eq!(food.per_100.protein_g, Some(3.8));
        assert_eq!(food.per_100.sat_fat_g, Some(1.8));
        assert_eq!(food.per_100.fiber_g, None); // absent de la fiche : inconnu, pas zéro
        assert_eq!(food.per_100.salt_g, Some(0.13));
        assert_eq!(food.density, None);
        assert_eq!(
            food.portions,
            [Portion {
                label: "1 portion".into(),
                grams: 125.0
            }]
        );
        assert_eq!(food.updated_at_ms, 42);
        // 1 portion = 125 g = 76,25 kcal.
        let eaten = food
            .nutrients_for(1.0, &Unit::Portion("1 portion".into()))
            .unwrap();
        assert!((eaten.kcal - 76.25).abs() < 1e-9);
    }

    #[test]
    fn unknown_product() {
        let json = r#"{"code":"0000000000000","status":0,"status_verbose":"product not found"}"#;
        assert_eq!(parse_product_response(json, 0), Ok(None));
        assert_eq!(
            parse_product_response("<html>", 0),
            Err(OffError::InvalidResponse)
        );
        assert_eq!(
            parse_product_response("{}", 0),
            Err(OffError::InvalidResponse)
        );
    }

    fn food(product: &str) -> Option<Food> {
        food_from_product(&serde_json::from_str(product).unwrap(), 0)
    }

    #[test]
    fn energy_and_salt_fallbacks() {
        // Énergie en kJ seulement : 1 000 kJ ÷ 4,184 = 239,0 kcal. Sodium seulement :
        // 0,4 g × 2,5 = 1 g de sel. Nombres reçus sous forme de texte.
        let f = food(
            r#"{"code":"12345678","product_name":"Biscuit",
                "nutriments":{"energy_100g":"1000","sodium_100g":"0.4","fat_100g":"12,5"}}"#,
        )
        .unwrap();
        assert_eq!(f.per_100.kcal, Some(239.0));
        assert_eq!(f.per_100.salt_g, Some(1.0));
        assert_eq!(f.per_100.fat_g, Some(12.5));
        assert_eq!(f.brand, None);
        assert!(f.portions.is_empty());
    }

    #[test]
    fn incomplete_or_absurd_values() {
        // Fiche sans nutriments : utilisable pour l'affichage, mais « sans calories ».
        let f = food(r#"{"code":"12345678","product_name":"Mystère"}"#).unwrap();
        assert!(!f.per_100.has_energy());
        // Énergie impossible (erreur de saisie) et valeur négative : ignorées.
        let f = food(
            r#"{"code":"12345678","product_name":"Erreur",
                "nutriments":{"energy-kcal_100g":4500,"proteins_100g":-3}}"#,
        )
        .unwrap();
        assert_eq!(f.per_100.kcal, None);
        assert_eq!(f.per_100.protein_g, None);
    }

    #[test]
    fn liquids_and_servings() {
        // Boisson : valeurs pour 100 ml, portion en ml.
        let f = food(
            r#"{"code":"5449000000996","product_name":"Soda","product_quantity_unit":"ml",
                "serving_size":"250 ml","serving_quantity":"250","serving_quantity_unit":"ml",
                "nutriments":{"energy-kcal_100g":42}}"#,
        )
        .unwrap();
        assert_eq!(f.density, Some(1.0));
        assert_eq!(f.portions[0].grams, 250.0);
        assert_eq!(f.grams_for(330.0, &Unit::Milliliter), Ok(330.0));
        // Portion dans une unité inconnue, ou nulle : pas de portion.
        let f = food(
            r#"{"code":"12345678","product_name":"X","serving_quantity":2,
                "serving_quantity_unit":"oz"}"#,
        )
        .unwrap();
        assert!(f.portions.is_empty());
        let f = food(r#"{"code":"12345678","product_name":"X","serving_quantity":0}"#).unwrap();
        assert!(f.portions.is_empty());
    }

    #[test]
    fn unusable_products_are_skipped() {
        assert!(food(r#"{"code":"12345678"}"#).is_none()); // sans nom
        assert!(food(r#"{"code":"12345678","product_name":"  "}"#).is_none());
        assert!(food(r#"{"product_name":"Sans code"}"#).is_none());
        assert!(food(r#"{"code":"abc","product_name":"Code invalide"}"#).is_none());
        // Nom générique à défaut de nom de produit.
        let f = food(r#"{"code":"12345678","generic_name_fr":"Pâtes de blé dur"}"#).unwrap();
        assert_eq!(f.name, "Pâtes de blé dur");
    }

    #[test]
    fn search_response() {
        let json = r#"{"count":3,"page":1,"page_count":3,"page_size":20,"skip":0,"products":[
            {"code":"3017620422003","product_name":"Pâte à tartiner","brands":"Marque",
             "nutriments":{"energy-kcal_100g":539}},
            {"code":"","product_name":"Sans code"},
            {"code":"3017620425035","product_name_fr":"Pâte à tartiner 1 kg"}
        ]}"#;
        let foods = parse_search_response(json, 7).unwrap();
        assert_eq!(foods.len(), 2);
        assert_eq!(foods[0].per_100.kcal, Some(539.0));
        assert_eq!(foods[1].name, "Pâte à tartiner 1 kg");
        assert_eq!(parse_search_response(r#"{"products":[]}"#, 0), Ok(vec![]));
        assert_eq!(
            parse_search_response(r#"{"error":"x"}"#, 0),
            Err(OffError::InvalidResponse)
        );
    }

    #[test]
    fn urls() {
        assert_eq!(
            product_url("3017620422003").unwrap(),
            format!("{HOST}/api/v2/product/3017620422003?fields={FIELDS}")
        );
        assert_eq!(product_url("12 34"), Err(OffError::InvalidBarcode));
        assert_eq!(product_url("1234567"), Err(OffError::InvalidBarcode));
        assert_eq!(
            product_url("../search?x=1234"),
            Err(OffError::InvalidBarcode)
        );
        let url = search_url(" crème & café ");
        assert!(url.starts_with(&format!(
            "{HOST}/cgi/search.pl?search_terms=cr%C3%A8me%20%26%20caf%C3%A9&"
        )));
        assert!(url.contains("&json=1&"));
        assert!(url.contains("&tag_0=france&"));
        assert_eq!(
            user_agent("0.1.0"),
            "FuckCalorie/0.1.0 (https://github.com/MoreIIo/Fuck-Calorie)"
        );
    }
}
