//! Convertit la table Ciqual officielle (ANSES) en fichier compact embarqué dans
//! l'application (`data/ciqual.tsv`). À relancer à chaque nouvelle édition de la table (§3.1).
//!
//! ```sh
//! cargo run -p fc-food-db --example import_ciqual -- CIQUAL2025_FR_2025_11_03.csv crates/food-db/data/ciqual.tsv
//! ```
//!
//! Entrée : l'export CSV français de la table (séparateur tabulation, UTF-8), tel que publié
//! sur <https://ciqual.anses.fr> et repris dans le dépôt d'Open Food Facts
//! (`external-data/ciqual/ciqual/`).
//!
//! Lecture des valeurs de la table :
//! - `12,5` → 12,5 ;
//! - `traces` et `< 0,5` (sous le seuil de mesure) → 0 ;
//! - `-` ou vide → inconnu.
//!
//! Les aliments dont l'énergie est inconnue ne sont pas repris.

use std::error::Error;
use std::fmt::Write as _;
use std::{env, fs};

/// En-têtes de colonne recherchés (début de l'en-tête, espaces normalisés), dans l'ordre des
/// colonnes du fichier produit. Pour l'énergie, la seconde colonne sert de repli.
const COLUMNS: [(&str, &[&str]); 8] = [
    (
        "kcal",
        &[
            "Energie, Règlement UE N° 1169 2011 (kcal",
            "Energie, N x facteur Jones, avec fibres (kcal",
        ],
    ),
    ("protein_g", &["Protéines, N x facteur de Jones"]),
    ("carbs_g", &["Glucides"]),
    ("sugars_g", &["Sucres"]),
    ("fat_g", &["Lipides"]),
    ("sat_fat_g", &["AG saturés"]),
    ("fiber_g", &["Fibres alimentaires"]),
    ("salt_g", &["Sel chlorure de sodium"]),
];

fn normalize(header: &str) -> String {
    header.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Valeur de la table → texte du fichier compact (vide = inconnu).
fn value(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() || raw == "-" {
        return String::new();
    }
    if raw.eq_ignore_ascii_case("traces") || raw.starts_with('<') {
        return "0".to_owned();
    }
    match raw.replace(',', ".").parse::<f64>() {
        Ok(number) if number.is_finite() && number >= 0.0 => format!("{number}"),
        _ => String::new(),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    let [_, input, output] = args.as_slice() else {
        return Err("usage : import_ciqual <table Ciqual .csv> <sortie .tsv>".into());
    };
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .from_path(input)?;
    let headers: Vec<String> = reader.headers()?.iter().map(normalize).collect();
    let find = |prefix: &str| headers.iter().position(|h| h.starts_with(prefix));
    let code = find("alim_code").ok_or("colonne alim_code introuvable")?;
    let name = find("alim_nom_fr").ok_or("colonne alim_nom_fr introuvable")?;
    let mut columns = Vec::new();
    for (field, prefixes) in COLUMNS {
        let found: Vec<usize> = prefixes.iter().filter_map(|p| find(p)).collect();
        if found.len() != prefixes.len() {
            return Err(format!("colonne introuvable pour {field} : {prefixes:?}").into());
        }
        columns.push(found);
    }

    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        let code = record.get(code).unwrap_or("").trim();
        let name = normalize(record.get(name).unwrap_or(""));
        if code.is_empty() || name.is_empty() {
            continue;
        }
        let mut line = format!("{code}\t{name}");
        let mut has_energy = true;
        for (position, candidates) in columns.iter().enumerate() {
            let text = candidates
                .iter()
                .map(|&index| value(record.get(index).unwrap_or("")))
                .find(|v| !v.is_empty())
                .unwrap_or_default();
            if position == 0 && text.is_empty() {
                has_energy = false;
            }
            write!(line, "\t{text}")?;
        }
        // Sans calories, la fiche est inutilisable pour le suivi : elle n'est pas embarquée.
        if !has_energy {
            continue;
        }
        rows.push((code.parse::<u64>().unwrap_or(u64::MAX), line));
    }
    rows.sort();
    rows.dedup_by(|a, b| a.0 == b.0);

    let mut out = String::from(
        "# Table Ciqual (ANSES) — Licence Ouverte Etalab. Fichier généré par \
         examples/import_ciqual.rs : ne pas modifier à la main.\n\
         # code\tnom\tkcal\tprotein_g\tcarbs_g\tsugars_g\tfat_g\tsat_fat_g\tfiber_g\tsalt_g \
         (pour 100 g ; vide = inconnu)\n",
    );
    for (_, line) in &rows {
        out.push_str(line);
        out.push('\n');
    }
    fs::write(output, out)?;
    println!("{} aliments écrits dans {output}", rows.len());
    Ok(())
}
