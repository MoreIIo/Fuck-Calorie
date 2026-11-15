//! Fiches aliments : Ciqual, cache Open Food Facts, aliments personnels (§3.1 à §3.3, §3.7).

use fc_core::food::{Food, Unit, search_key};
use rusqlite::types::Value;
use rusqlite::{OptionalExtension, Row, params, params_from_iter};
use uuid::Uuid;

use crate::convert::{self, id_text, like_escape, to_enum_text, to_json, upsert_sql};
use crate::{Db, Result};

const COLUMNS: &str = "id, hlc, deleted, source, source_ref, barcode, name, brand, per_100, \
                       density, unit_weight_g, portions, updated_at_ms";

fn food_from_row(row: &Row<'_>) -> rusqlite::Result<Food> {
    Ok(Food {
        meta: convert::meta(row)?,
        source: convert::enum_text(row, 3)?,
        source_ref: row.get(4)?,
        barcode: row.get(5)?,
        name: row.get(6)?,
        brand: row.get(7)?,
        per_100: convert::json(row, 8)?,
        density: row.get(9)?,
        unit_weight_g: row.get(10)?,
        portions: convert::json(row, 11)?,
        updated_at_ms: row.get(12)?,
    })
}

impl Db {
    /// Enregistre une fiche aliment. Renvoie `false` si la base contient déjà une version
    /// plus récente (rien n'est alors modifié).
    pub fn upsert_food(&self, food: &Food) -> Result<bool> {
        let sql = upsert_sql(
            "foods",
            "id",
            &[
                "id",
                "hlc",
                "deleted",
                "source",
                "source_ref",
                "barcode",
                "name",
                "brand",
                "search_text",
                "per_100",
                "density",
                "unit_weight_g",
                "portions",
                "updated_at_ms",
            ],
        );
        let changed = self.conn.prepare_cached(&sql)?.execute(params![
            id_text(food.meta.id),
            food.meta.hlc.to_string(),
            food.meta.deleted,
            to_enum_text(&food.source)?,
            food.source_ref,
            food.barcode,
            food.name,
            food.brand,
            food.search_text(),
            to_json(&food.per_100)?,
            food.density,
            food.unit_weight_g,
            to_json(&food.portions)?,
            food.updated_at_ms,
        ])?;
        Ok(changed > 0)
    }

    /// Fiche d'un aliment (non supprimé).
    pub fn food(&self, id: Uuid) -> Result<Option<Food>> {
        let sql = format!("SELECT {COLUMNS} FROM foods WHERE id = ?1 AND deleted = 0");
        Ok(self
            .conn
            .prepare_cached(&sql)?
            .query_row([id_text(id)], food_from_row)
            .optional()?)
    }

    /// Aliment correspondant à un code-barres. Un aliment personnel **masque** la fiche
    /// Open Food Facts du même code-barres (§3.7).
    pub fn food_by_barcode(&self, barcode: &str) -> Result<Option<Food>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM foods WHERE barcode = ?1 AND deleted = 0
             ORDER BY (source = 'custom') DESC, updated_at_ms DESC LIMIT 1"
        );
        Ok(self
            .conn
            .prepare_cached(&sql)?
            .query_row([barcode], food_from_row)
            .optional()?)
    }

    /// Recherche locale d'aliments (§3.3) : dès 2 caractères, insensible aux accents et à la
    /// casse ; chaque mot de la requête doit apparaître dans le nom ou la marque.
    ///
    /// Ordre : aliments personnels et recettes, puis Ciqual, puis Open Food Facts ; à source
    /// égale, les noms qui commencent par la requête d'abord, puis les plus courts.
    pub fn search_foods(&self, query: &str, limit: usize) -> Result<Vec<Food>> {
        let key = search_key(query);
        if key.chars().count() < 2 {
            return Ok(Vec::new());
        }
        let words: Vec<&str> = key.split(' ').collect();
        let mut conditions = String::new();
        let mut values: Vec<Value> = Vec::new();
        for word in &words {
            values.push(Value::Text(format!("%{}%", like_escape(word))));
            conditions.push_str(&format!(
                " AND search_text LIKE ?{} ESCAPE '\\'",
                values.len()
            ));
        }
        values.push(Value::Text(format!("{}%", like_escape(&key))));
        let prefix = values.len();
        values.push(Value::Integer(i64::try_from(limit).unwrap_or(i64::MAX)));
        let sql = format!(
            "SELECT {COLUMNS} FROM foods WHERE deleted = 0{conditions}
             ORDER BY CASE source WHEN 'custom' THEN 0 WHEN 'recipe' THEN 0
                                  WHEN 'ciqual' THEN 1 ELSE 2 END,
                      (search_text LIKE ?{prefix} ESCAPE '\\') DESC,
                      length(name), name
             LIMIT ?{}",
            prefix + 1
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(values), food_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Aliments récemment utilisés dans le journal, du plus récent au plus ancien (onglet
    /// *Récents*, §3.3).
    pub fn recent_foods(&self, limit: usize) -> Result<Vec<Food>> {
        let columns = COLUMNS.replace(", ", ", f.");
        let sql = format!(
            "SELECT f.{columns} FROM foods f
             JOIN (SELECT food_id, MAX(created_at_ms) AS last_used FROM log_entries
                   WHERE deleted = 0 AND food_id IS NOT NULL GROUP BY food_id) r
               ON r.food_id = f.id
             WHERE f.deleted = 0
             ORDER BY r.last_used DESC LIMIT ?1"
        );
        let mut statement = self.conn.prepare_cached(&sql)?;
        let rows =
            statement.query_map([i64::try_from(limit).unwrap_or(i64::MAX)], food_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Dernière quantité saisie pour un aliment, pour pré-remplir l'écran de quantité (§3.4).
    pub fn last_quantity_for(&self, food_id: Uuid) -> Result<Option<(f64, Unit)>> {
        Ok(self
            .conn
            .prepare_cached(
                "SELECT quantity, unit FROM log_entries WHERE food_id = ?1 AND deleted = 0
                 ORDER BY created_at_ms DESC, hlc DESC LIMIT 1",
            )?
            .query_row([id_text(food_id)], |row| {
                Ok((row.get(0)?, convert::json(row, 1)?))
            })
            .optional()?)
    }
}
