//! Aliments, portions et saisie des quantités (§3.4, §3.8).

use serde::{Deserialize, Serialize};

use crate::hlc::SyncMeta;
use crate::nutrients::{NutrientFacts, Nutrients};

/// Origine d'une fiche aliment (§3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoodSource {
    /// Table Ciqual (ANSES), embarquée dans l'application.
    Ciqual,
    /// Open Food Facts.
    Off,
    /// Aliment personnel créé par l'utilisatrice.
    Custom,
    /// Recette maison.
    Recipe,
}

/// Portion nommée d'un aliment (« 1 pot », « 1 tranche »).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Portion {
    pub label: String,
    pub grams: f64,
}

/// Fiche d'un aliment (§3.8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Food {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub source: FoodSource,
    /// Identifiant dans la source (code Ciqual, code-barres OFF…).
    pub source_ref: Option<String>,
    pub barcode: Option<String>,
    pub name: String,
    pub brand: Option<String>,
    /// Valeurs pour 100 g (ou 100 ml pour un liquide).
    pub per_100: NutrientFacts,
    /// Densité en g/ml, pour convertir les millilitres (1 par défaut).
    pub density: Option<f64>,
    /// Poids moyen d'une pièce en grammes (œuf ≈ 50 g, banane ≈ 120 g).
    pub unit_weight_g: Option<f64>,
    /// Portions disponibles ; la première est la portion par défaut.
    pub portions: Vec<Portion>,
    /// Dernière mise à jour de la fiche, en millisecondes Unix (rafraîchissement du cache OFF).
    pub updated_at_ms: i64,
}

/// Unité de saisie d'une quantité (§3.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "label", rename_all = "snake_case")]
pub enum Unit {
    Gram,
    /// Millilitres, convertis avec la densité de l'aliment.
    Milliliter,
    /// Portion nommée de l'aliment (« 1 pot »).
    Portion(String),
    /// Pièce / unité (« 2 œufs »).
    Piece,
}

/// Raison pour laquelle une quantité ne peut pas être convertie en grammes.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum QuantityError {
    #[error("la quantité doit être un nombre positif")]
    InvalidAmount,
    #[error("cet aliment n'a pas de portion « {0} »")]
    UnknownPortion(String),
    #[error("le poids d'une pièce n'est pas connu pour cet aliment")]
    NoUnitWeight,
}

impl Food {
    /// Convertit une quantité saisie en grammes.
    pub fn grams_for(&self, amount: f64, unit: &Unit) -> Result<f64, QuantityError> {
        if !amount.is_finite() || amount <= 0.0 {
            return Err(QuantityError::InvalidAmount);
        }
        match unit {
            Unit::Gram => Ok(amount),
            Unit::Milliliter => Ok(amount * self.density.unwrap_or(1.0)),
            Unit::Portion(label) => self
                .portions
                .iter()
                .find(|p| &p.label == label)
                .map(|p| amount * p.grams)
                .ok_or_else(|| QuantityError::UnknownPortion(label.clone())),
            Unit::Piece => self
                .unit_weight_g
                .map(|g| amount * g)
                .ok_or(QuantityError::NoUnitWeight),
        }
    }

    /// Nutriments apportés par une quantité saisie.
    pub fn nutrients_for(&self, amount: f64, unit: &Unit) -> Result<Nutrients, QuantityError> {
        Ok(self.per_100.for_grams(self.grams_for(amount, unit)?))
    }

    /// Unités utilisables pour cet aliment (§3.4), dans l'ordre d'affichage : grammes
    /// toujours, millilitres pour un liquide (densité connue), ses portions, la pièce si son
    /// poids est connu.
    #[must_use]
    pub fn units(&self) -> Vec<Unit> {
        let mut units = vec![Unit::Gram];
        if self.density.is_some() {
            units.push(Unit::Milliliter);
        }
        units.extend(self.portions.iter().map(|p| Unit::Portion(p.label.clone())));
        if self.unit_weight_g.is_some() {
            units.push(Unit::Piece);
        }
        units
    }

    /// Quantité pré-remplie à l'ouverture de l'écran de saisie (§3.4) : la dernière quantité
    /// utilisée pour cet aliment, sinon la portion par défaut, sinon 100 g.
    #[must_use]
    pub fn default_quantity(&self, last_used: Option<(f64, Unit)>) -> (f64, Unit) {
        if let Some((amount, unit)) = last_used {
            // La dernière quantité n'est reprise que si elle est encore valable (la portion
            // peut avoir été supprimée depuis).
            if self.grams_for(amount, &unit).is_ok() {
                return (amount, unit);
            }
        }
        match self.portions.first() {
            Some(portion) => (1.0, Unit::Portion(portion.label.clone())),
            None => (100.0, Unit::Gram),
        }
    }
}

/// Un code-barres de produit : 8 à 14 chiffres (EAN-8, UPC-A, EAN-13, ITF-14), §4.2.
#[must_use]
pub fn is_barcode(text: &str) -> bool {
    (8..=14).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit())
}

/// Clé de recherche d'un texte : minuscules, sans accents ni ponctuation (§3.3 : recherche
/// insensible aux accents et à la casse). « Crème fraîche 30 % » → `creme fraiche 30`.
#[must_use]
pub fn search_key(text: &str) -> String {
    let mut key = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        let folded: &str = match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => "a",
            'ç' => "c",
            'è' | 'é' | 'ê' | 'ë' => "e",
            'ì' | 'í' | 'î' | 'ï' => "i",
            'ñ' => "n",
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => "o",
            'ù' | 'ú' | 'û' | 'ü' => "u",
            'ý' | 'ÿ' => "y",
            'œ' => "oe",
            'æ' => "ae",
            'ß' => "ss",
            c if c.is_alphanumeric() => {
                key.push(c);
                continue;
            }
            // Ponctuation et espaces : un seul espace de séparation.
            _ => {
                if !key.is_empty() && !key.ends_with(' ') {
                    key.push(' ');
                }
                continue;
            }
        };
        key.push_str(folded);
    }
    key.truncate(key.trim_end().len());
    key
}

/// Nombre de fautes de frappe tolérées pour un mot de la requête (§3.3 : recherche floue) :
/// aucune jusqu'à 3 lettres, une de 4 à 7 lettres, deux au-delà.
fn allowed_typos(word_len: usize) -> usize {
    match word_len {
        0..=3 => 0,
        4..=7 => 1,
        _ => 2,
    }
}

/// Distance d'édition entre deux mots : insertions, suppressions, remplacements et
/// inversions de deux lettres voisines comptent chacun pour 1.
fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut before: Vec<usize> = Vec::new();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut current = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let substitution = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (previous[j] + 1)
                .min(current[j - 1] + 1)
                .min(previous[j - 1] + substitution);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(before[j - 2] + 1);
            }
            current[j] = best;
        }
        before = previous;
        previous = current;
    }
    previous[b.len()]
}

/// Nombre de fautes entre un mot de la requête et le **début** d'un mot du texte (la
/// recherche se fait pendant la frappe : « yahou » doit déjà trouver « yaourt »).
fn word_typos(query: &[char], word: &[char], allowed: usize) -> usize {
    let shortest = query.len().saturating_sub(allowed).max(1);
    let longest = (query.len() + allowed).min(word.len());
    (shortest..=longest)
        .map(|len| edit_distance(query, &word[..len]))
        .min()
        .unwrap_or(usize::MAX)
}

/// Recherche tolérante aux fautes de frappe (§3.3). `query_key` et `text_key` sont des clés
/// de recherche ([`search_key`]). Renvoie le nombre total de fautes si **chaque mot** de la
/// requête correspond au début d'un mot du texte, à quelques fautes près ; `None` sinon.
/// `Some(0)` = correspondance exacte.
#[must_use]
pub fn fuzzy_match(query_key: &str, text_key: &str) -> Option<usize> {
    let words: Vec<Vec<char>> = text_key.split(' ').map(|w| w.chars().collect()).collect();
    let mut total = 0;
    let mut any = false;
    for query in query_key.split(' ').filter(|w| !w.is_empty()) {
        any = true;
        let query: Vec<char> = query.chars().collect();
        let allowed = allowed_typos(query.len());
        let best = words
            .iter()
            .map(|word| word_typos(&query, word, allowed))
            .min()
            .unwrap_or(usize::MAX);
        if best > allowed {
            return None;
        }
        total += best;
    }
    any.then_some(total)
}

impl Food {
    /// Clé de recherche de la fiche : nom et marque.
    #[must_use]
    pub fn search_text(&self) -> String {
        match &self.brand {
            Some(brand) => search_key(&format!("{} {}", self.name, brand)),
            None => search_key(&self.name),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn barcodes() {
        for code in [
            "12345670",
            "036000291452",
            "3017620422003",
            "10012345678902",
        ] {
            assert!(is_barcode(code), "{code}");
        }
        for text in [
            "1234567",
            "123456789012345",
            "30176204220a3",
            "3017 6204",
            "",
            "nutella",
        ] {
            assert!(!is_barcode(text), "{text}");
        }
    }
    use crate::hlc::Hlc;
    use uuid::Uuid;

    pub(crate) fn yaourt() -> Food {
        Food {
            meta: SyncMeta::with_id(Uuid::from_u128(100), Hlc::ZERO),
            source: FoodSource::Off,
            source_ref: Some("3033490004743".into()),
            barcode: Some("3033490004743".into()),
            name: "Yaourt nature".into(),
            brand: Some("Marque".into()),
            per_100: NutrientFacts {
                kcal: Some(61.0),
                protein_g: Some(3.8),
                carbs_g: Some(5.3),
                sugars_g: Some(5.3),
                fat_g: Some(2.7),
                sat_fat_g: Some(1.8),
                fiber_g: None,
                salt_g: Some(0.13),
            },
            density: None,
            unit_weight_g: None,
            portions: vec![Portion {
                label: "1 pot".into(),
                grams: 125.0,
            }],
            updated_at_ms: 0,
        }
    }

    #[test]
    fn unit_conversions() {
        let mut food = yaourt();
        assert_eq!(food.grams_for(80.0, &Unit::Gram), Ok(80.0));
        assert_eq!(
            food.grams_for(2.0, &Unit::Portion("1 pot".into())),
            Ok(250.0)
        );
        // Millilitres : densité 1 par défaut, puis densité de l'huile.
        assert_eq!(food.grams_for(250.0, &Unit::Milliliter), Ok(250.0));
        food.density = Some(0.92);
        assert!((food.grams_for(10.0, &Unit::Milliliter).unwrap() - 9.2).abs() < 1e-9);
        // Pièces.
        assert_eq!(
            food.grams_for(2.0, &Unit::Piece),
            Err(QuantityError::NoUnitWeight)
        );
        food.unit_weight_g = Some(50.0);
        assert_eq!(food.grams_for(2.0, &Unit::Piece), Ok(100.0));
    }

    #[test]
    fn conversion_errors() {
        let food = yaourt();
        assert_eq!(
            food.grams_for(0.0, &Unit::Gram),
            Err(QuantityError::InvalidAmount)
        );
        assert_eq!(
            food.grams_for(-5.0, &Unit::Gram),
            Err(QuantityError::InvalidAmount)
        );
        assert_eq!(
            food.grams_for(f64::NAN, &Unit::Gram),
            Err(QuantityError::InvalidAmount)
        );
        assert_eq!(
            food.grams_for(1.0, &Unit::Portion("1 bol".into())),
            Err(QuantityError::UnknownPortion("1 bol".into()))
        );
    }

    #[test]
    fn nutrients_for_quantity() {
        let n = yaourt()
            .nutrients_for(1.0, &Unit::Portion("1 pot".into()))
            .unwrap();
        assert!((n.kcal - 76.25).abs() < 1e-9);
        assert!((n.protein_g - 4.75).abs() < 1e-9);
        assert_eq!(n.fiber_g, 0.0); // inconnu → compté zéro
    }

    #[test]
    fn default_quantity_priority() {
        let mut food = yaourt();
        // 1. Dernière quantité utilisée.
        assert_eq!(
            food.default_quantity(Some((80.0, Unit::Gram))),
            (80.0, Unit::Gram)
        );
        // 2. Portion par défaut.
        assert_eq!(
            food.default_quantity(None),
            (1.0, Unit::Portion("1 pot".into()))
        );
        // Dernière quantité devenue invalide → portion par défaut.
        assert_eq!(
            food.default_quantity(Some((1.0, Unit::Portion("1 bol".into())))),
            (1.0, Unit::Portion("1 pot".into()))
        );
        // 3. 100 g.
        food.portions.clear();
        assert_eq!(food.default_quantity(None), (100.0, Unit::Gram));
    }

    #[test]
    fn available_units() {
        let mut food = yaourt();
        assert_eq!(food.units(), [Unit::Gram, Unit::Portion("1 pot".into())]);
        food.density = Some(1.03);
        food.unit_weight_g = Some(125.0);
        assert_eq!(
            food.units(),
            [
                Unit::Gram,
                Unit::Milliliter,
                Unit::Portion("1 pot".into()),
                Unit::Piece
            ]
        );
    }

    #[test]
    fn search_keys() {
        assert_eq!(
            search_key("Crème fraîche 30 % M.G."),
            "creme fraiche 30 m g"
        );
        assert_eq!(search_key("  ŒUF dur, BIO  "), "oeuf dur bio");
        assert_eq!(search_key("Jalapeño"), "jalapeno");
        assert_eq!(search_key("---"), "");
        assert_eq!(yaourt().search_text(), "yaourt nature marque");
    }

    #[test]
    fn fuzzy_search_tolerates_typos() {
        let text = search_key("Yaourt nature au lait entier");
        // Exact, ou début de mot : aucune faute.
        assert_eq!(fuzzy_match("yaourt", &text), Some(0));
        assert_eq!(fuzzy_match("yao nat", &text), Some(0));
        // Une lettre en trop, une lettre manquante, deux lettres inversées.
        assert_eq!(fuzzy_match("yahourt", &text), Some(1));
        assert_eq!(fuzzy_match("yaort", &text), Some(1));
        assert_eq!(fuzzy_match("yauort", &text), Some(1));
        // Pendant la frappe : le mot n'est pas encore fini.
        assert_eq!(fuzzy_match("yahou", &text), Some(1));
        // Les fautes de chaque mot s'additionnent.
        assert_eq!(fuzzy_match("yahourt natrue", &text), Some(2));
        // Trop de fautes, ou un mot absent.
        assert_eq!(fuzzy_match("yoghurt", &text), None);
        assert_eq!(fuzzy_match("yaourt fraise", &text), None);
        // Mots courts : aucune faute tolérée (« lit » n'est pas « lait »).
        assert_eq!(fuzzy_match("lit", &text), None);
        assert_eq!(fuzzy_match("au", &text), Some(0));
        // Mots longs : deux fautes.
        assert_eq!(
            fuzzy_match("camenbret", &search_key("Camembert au lait cru")),
            Some(2)
        );
        assert_eq!(fuzzy_match("", &text), None);
    }

    #[test]
    fn serde_round_trip() {
        let food = yaourt();
        let json = serde_json::to_string(&food).unwrap();
        assert_eq!(serde_json::from_str::<Food>(&json).unwrap(), food);
        assert_eq!(
            serde_json::to_string(&Unit::Gram).unwrap(),
            r#"{"type":"gram"}"#
        );
        assert_eq!(
            serde_json::to_string(&Unit::Portion("1 pot".into())).unwrap(),
            r#"{"type":"portion","label":"1 pot"}"#
        );
    }
}
