//! Journal : repas, entrées, repas enregistrés et état des journées (§3.5, §3.6).

use chrono::NaiveDate;
use fc_core::journal::{DayStatus, LogEntry, MealSlot, SavedMeal};
use fc_core::nutrients::Nutrients;
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::convert::{self, date_text, id_text, time_text, to_json, upsert_sql};
use crate::{Db, Result};

const ENTRY_COLUMNS: &str = "id, hlc, deleted, date, meal_slot_id, food_id, label, quantity, \
                             unit, grams, kcal, protein_g, carbs_g, sugars_g, fat_g, sat_fat_g, \
                             fiber_g, salt_g, created_at_ms";

fn entry_from_row(row: &Row<'_>) -> rusqlite::Result<LogEntry> {
    Ok(LogEntry {
        meta: convert::meta(row)?,
        date: convert::date(row, 3)?,
        meal_slot_id: convert::uuid(row, 4)?,
        food_id: convert::opt_uuid(row, 5)?,
        label: row.get(6)?,
        quantity: row.get(7)?,
        unit: convert::json(row, 8)?,
        grams: row.get(9)?,
        nutrients_snapshot: Nutrients {
            kcal: row.get(10)?,
            protein_g: row.get(11)?,
            carbs_g: row.get(12)?,
            sugars_g: row.get(13)?,
            fat_g: row.get(14)?,
            sat_fat_g: row.get(15)?,
            fiber_g: row.get(16)?,
            salt_g: row.get(17)?,
        },
        created_at_ms: row.get(18)?,
    })
}

fn upsert_entry(conn: &Connection, entry: &LogEntry) -> Result<bool> {
    let columns: Vec<&str> = ENTRY_COLUMNS.split(", ").map(str::trim).collect();
    let sql = upsert_sql("log_entries", "id", &columns);
    let n = &entry.nutrients_snapshot;
    let changed = conn.prepare_cached(&sql)?.execute(params![
        id_text(entry.meta.id),
        entry.meta.hlc.to_string(),
        entry.meta.deleted,
        date_text(entry.date),
        id_text(entry.meal_slot_id),
        entry.food_id.map(id_text),
        entry.label,
        entry.quantity,
        to_json(&entry.unit)?,
        entry.grams,
        n.kcal,
        n.protein_g,
        n.carbs_g,
        n.sugars_g,
        n.fat_g,
        n.sat_fat_g,
        n.fiber_g,
        n.salt_g,
        entry.created_at_ms,
    ])?;
    Ok(changed > 0)
}

impl Db {
    // ----- Repas -----

    /// Enregistre un repas configurable. `false` si une version plus récente existe déjà.
    pub fn upsert_meal_slot(&self, slot: &MealSlot) -> Result<bool> {
        let sql = upsert_sql(
            "meal_slots",
            "id",
            &[
                "id",
                "hlc",
                "deleted",
                "name",
                "sort_order",
                "time_start",
                "time_end",
            ],
        );
        let changed = self.conn.prepare_cached(&sql)?.execute(params![
            id_text(slot.meta.id),
            slot.meta.hlc.to_string(),
            slot.meta.deleted,
            slot.name,
            slot.order,
            slot.time_start.map(time_text),
            slot.time_end.map(time_text),
        ])?;
        Ok(changed > 0)
    }

    /// Les repas, dans leur ordre d'affichage.
    pub fn meal_slots(&self) -> Result<Vec<MealSlot>> {
        let mut statement = self.conn.prepare_cached(
            "SELECT id, hlc, deleted, name, sort_order, time_start, time_end FROM meal_slots
             WHERE deleted = 0 ORDER BY sort_order, id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(MealSlot {
                meta: convert::meta(row)?,
                name: row.get(3)?,
                order: row.get(4)?,
                time_start: convert::opt_time(row, 5)?,
                time_end: convert::opt_time(row, 6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ----- Entrées du journal -----

    /// Enregistre une entrée du journal (ajout, modification ou suppression par tombstone).
    /// `false` si une version plus récente existe déjà.
    pub fn upsert_log_entry(&self, entry: &LogEntry) -> Result<bool> {
        upsert_entry(&self.conn, entry)
    }

    /// Enregistre plusieurs entrées **en une seule transaction** : tout ou rien (copie d'un
    /// repas ou d'une journée, ajout d'un repas enregistré). Renvoie le nombre d'entrées
    /// effectivement écrites.
    pub fn upsert_log_entries(&mut self, entries: &[LogEntry]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut written = 0;
        for entry in entries {
            if upsert_entry(&tx, entry)? {
                written += 1;
            }
        }
        tx.commit()?;
        Ok(written)
    }

    /// Une entrée du journal (non supprimée).
    pub fn log_entry(&self, id: uuid::Uuid) -> Result<Option<LogEntry>> {
        let sql = format!("SELECT {ENTRY_COLUMNS} FROM log_entries WHERE id = ?1 AND deleted = 0");
        Ok(self
            .conn
            .prepare_cached(&sql)?
            .query_row([id_text(id)], entry_from_row)
            .optional()?)
    }

    /// Entrées d'un jour, dans l'ordre de saisie.
    pub fn log_entries_on(&self, date: NaiveDate) -> Result<Vec<LogEntry>> {
        self.log_entries_between(date, date)
    }

    /// Entrées d'une période (bornes incluses), par date puis ordre de saisie.
    pub fn log_entries_between(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<LogEntry>> {
        let sql = format!(
            "SELECT {ENTRY_COLUMNS} FROM log_entries
             WHERE deleted = 0 AND date BETWEEN ?1 AND ?2 ORDER BY date, created_at_ms, id"
        );
        let mut statement = self.conn.prepare_cached(&sql)?;
        let rows = statement.query_map([date_text(from), date_text(to)], entry_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Total des nutriments de chaque jour journalisé d'une période (bornes incluses), pour
    /// les graphiques et l'algorithme adaptatif. Les jours sans entrée sont absents.
    pub fn daily_totals_between(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<(NaiveDate, Nutrients)>> {
        let mut statement = self.conn.prepare_cached(
            "SELECT date, SUM(kcal), SUM(protein_g), SUM(carbs_g), SUM(sugars_g), SUM(fat_g),
                    SUM(sat_fat_g), SUM(fiber_g), SUM(salt_g)
             FROM log_entries WHERE deleted = 0 AND date BETWEEN ?1 AND ?2
             GROUP BY date ORDER BY date",
        )?;
        let rows = statement.query_map([date_text(from), date_text(to)], |row| {
            Ok((
                convert::date(row, 0)?,
                Nutrients {
                    kcal: row.get(1)?,
                    protein_g: row.get(2)?,
                    carbs_g: row.get(3)?,
                    sugars_g: row.get(4)?,
                    fat_g: row.get(5)?,
                    sat_fat_g: row.get(6)?,
                    fiber_g: row.get(7)?,
                    salt_g: row.get(8)?,
                },
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ----- État des journées -----

    /// Enregistre l'état d'une journée (*Journée terminée*, note). Une seule ligne par date ;
    /// `false` si une version plus récente existe déjà.
    pub fn set_day_status(&self, status: &DayStatus) -> Result<bool> {
        let sql = upsert_sql("day_status", "date", &["date", "hlc", "complete", "note"]);
        let changed = self.conn.prepare_cached(&sql)?.execute(params![
            date_text(status.date),
            status.hlc.to_string(),
            status.complete,
            status.note,
        ])?;
        Ok(changed > 0)
    }

    /// État d'une journée, s'il a été enregistré.
    pub fn day_status(&self, date: NaiveDate) -> Result<Option<DayStatus>> {
        Ok(self.day_statuses_between(date, date)?.into_iter().next())
    }

    /// États des journées d'une période (bornes incluses).
    pub fn day_statuses_between(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<DayStatus>> {
        let mut statement = self.conn.prepare_cached(
            "SELECT date, hlc, complete, note FROM day_status
             WHERE date BETWEEN ?1 AND ?2 ORDER BY date",
        )?;
        let rows = statement.query_map([date_text(from), date_text(to)], |row| {
            Ok(DayStatus {
                date: convert::date(row, 0)?,
                hlc: convert::hlc(row, 1)?,
                complete: row.get(2)?,
                note: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ----- Repas enregistrés -----

    /// Enregistre un repas enregistré (« Mon petit-déj »).
    pub fn upsert_saved_meal(&self, meal: &SavedMeal) -> Result<bool> {
        let sql = upsert_sql(
            "saved_meals",
            "id",
            &["id", "hlc", "deleted", "name", "items"],
        );
        let changed = self.conn.prepare_cached(&sql)?.execute(params![
            id_text(meal.meta.id),
            meal.meta.hlc.to_string(),
            meal.meta.deleted,
            meal.name,
            to_json(&meal.items)?,
        ])?;
        Ok(changed > 0)
    }

    /// Les repas enregistrés, par nom.
    pub fn saved_meals(&self) -> Result<Vec<SavedMeal>> {
        let mut statement = self.conn.prepare_cached(
            "SELECT id, hlc, deleted, name, items FROM saved_meals
             WHERE deleted = 0 ORDER BY name COLLATE NOCASE, id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SavedMeal {
                meta: convert::meta(row)?,
                name: row.get(3)?,
                items: convert::json(row, 4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Un repas enregistré.
    pub fn saved_meal(&self, id: uuid::Uuid) -> Result<Option<SavedMeal>> {
        Ok(self
            .conn
            .prepare_cached(
                "SELECT id, hlc, deleted, name, items FROM saved_meals
                 WHERE id = ?1 AND deleted = 0",
            )?
            .query_row([id_text(id)], |row| {
                Ok(SavedMeal {
                    meta: convert::meta(row)?,
                    name: row.get(3)?,
                    items: convert::json(row, 4)?,
                })
            })
            .optional()?)
    }
}
