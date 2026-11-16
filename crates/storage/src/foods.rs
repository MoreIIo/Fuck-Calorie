//! Fiches aliments : Ciqual, cache Open Food Facts, aliments personnels (§3.1 à §3.3, §3.7).

use fc_core::food::{Food, FoodSource, Unit, fuzzy_match, search_key};
use fc_core::hlc::Hlc;
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

    /// Enregistre des fiches de **référence** (table Ciqual embarquée, §3.1) en une seule
    /// transaction. Contrairement à [`Db::upsert_food`], la fiche existante est remplacée
    /// sans comparer les horodatages : ces fiches ne sont pas des objets synchronisés, elles
    /// sont identiques sur tous les appareils et ne changent qu'avec une nouvelle édition de
    /// la table.
    pub fn import_reference_foods(&mut self, foods: &[Food]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        {
            let mut statement = tx.prepare(
                "INSERT INTO foods (id, hlc, deleted, source, source_ref, barcode, name, brand,
                                    search_text, per_100, density, unit_weight_g, portions,
                                    updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                 ON CONFLICT (id) DO UPDATE SET
                    hlc = excluded.hlc, deleted = excluded.deleted, source = excluded.source,
                    source_ref = excluded.source_ref, barcode = excluded.barcode,
                    name = excluded.name, brand = excluded.brand,
                    search_text = excluded.search_text, per_100 = excluded.per_100,
                    density = excluded.density, portions = excluded.portions,
                    updated_at_ms = excluded.updated_at_ms",
            )?;
            for food in foods {
                statement.execute(params![
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
            }
        }
        tx.commit()?;
        Ok(foods.len())
    }

    /// Recherche locale d'aliments (§3.3) : dès 2 caractères, insensible aux accents et à la
    /// casse ; chaque mot de la requête doit apparaître dans le nom ou la marque.
    ///
    /// Ordre : aliments personnels et recettes, puis Ciqual, puis Open Food Facts ; à source
    /// égale, les noms qui commencent par la requête d'abord, puis les plus courts.
    ///
    /// S'il reste de la place, la liste est complétée par les fiches qui correspondent **à
    /// quelques fautes de frappe près** (« yahourt » trouve « yaourt »), les moins fautives
    /// d'abord.
    pub fn search_foods(&self, query: &str, limit: usize) -> Result<Vec<Food>> {
        let key = search_key(query);
        if key.chars().count() < 2 {
            return Ok(Vec::new());
        }
        let mut foods = self.search_foods_exact(&key, limit)?;
        if foods.len() < limit {
            let missing = limit - foods.len();
            let found: Vec<Uuid> = foods.iter().map(|f| f.meta.id).collect();
            for id in self.fuzzy_food_ids(&key, &found, missing)? {
                foods.extend(self.food(id)?);
            }
        }
        Ok(foods)
    }

    fn search_foods_exact(&self, key: &str, limit: usize) -> Result<Vec<Food>> {
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
        values.push(Value::Text(format!("{}%", like_escape(key))));
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

    /// Identifiants des fiches qui correspondent à la requête à quelques fautes de frappe
    /// près, hors `exclude`, les moins fautives puis les noms les plus courts d'abord.
    fn fuzzy_food_ids(&self, key: &str, exclude: &[Uuid], limit: usize) -> Result<Vec<Uuid>> {
        let mut statement = self
            .conn
            .prepare_cached("SELECT id, search_text FROM foods WHERE deleted = 0")?;
        let mut rows = statement.query([])?;
        let mut matches: Vec<(usize, usize, Uuid)> = Vec::new();
        while let Some(row) = rows.next()? {
            let text = row.get_ref(1)?.as_str().map_err(rusqlite::Error::from)?;
            if let Some(typos) = fuzzy_match(key, text) {
                let id = convert::uuid(row, 0)?;
                if !exclude.contains(&id) {
                    matches.push((typos, text.len(), id));
                }
            }
        }
        matches.sort_unstable();
        Ok(matches.into_iter().take(limit).map(|m| m.2).collect())
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

    /// Aliments d'une source, par nom (onglet *Mes aliments*, §3.3).
    pub fn foods_by_source(&self, source: FoodSource, limit: usize) -> Result<Vec<Food>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM foods WHERE source = ?1 AND deleted = 0
             ORDER BY name COLLATE NOCASE, id LIMIT ?2"
        );
        let mut statement = self.conn.prepare_cached(&sql)?;
        let rows = statement.query_map(
            params![
                to_enum_text(&source)?,
                i64::try_from(limit).unwrap_or(i64::MAX)
            ],
            food_from_row,
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ----- Favoris (§3.6) -----

    /// Marque ou démarque un aliment comme favori. `false` si la base contient déjà un
    /// choix plus récent (rien n'est alors modifié).
    pub fn set_favorite(&self, food_id: Uuid, favorite: bool, hlc: Hlc) -> Result<bool> {
        let changed = self
            .conn
            .prepare_cached(
                "INSERT INTO favorites (food_id, hlc, favorite) VALUES (?1, ?2, ?3)
                 ON CONFLICT (food_id) DO UPDATE SET hlc = excluded.hlc,
                    favorite = excluded.favorite
                 WHERE excluded.hlc > favorites.hlc",
            )?
            .execute(params![id_text(food_id), hlc.to_string(), favorite])?;
        Ok(changed > 0)
    }

    /// L'aliment est-il un favori ?
    pub fn is_favorite(&self, food_id: Uuid) -> Result<bool> {
        Ok(self
            .conn
            .prepare_cached("SELECT favorite FROM favorites WHERE food_id = ?1")?
            .query_row([id_text(food_id)], |row| row.get(0))
            .optional()?
            .unwrap_or(false))
    }

    /// Aliments favoris, par nom (onglet *Favoris*, §3.3).
    pub fn favorite_foods(&self, limit: usize) -> Result<Vec<Food>> {
        let columns = COLUMNS.replace(", ", ", f.");
        let sql = format!(
            "SELECT f.{columns} FROM foods f
             JOIN favorites v ON v.food_id = f.id AND v.favorite = 1
             WHERE f.deleted = 0
             ORDER BY f.name COLLATE NOCASE, f.id LIMIT ?1"
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
