//! Pesées et activités physiques (§2.6, §2.8).

use chrono::NaiveDate;
use fc_core::activity::{Activity, ActivitySource};
use fc_core::journal::WeighIn;
use rusqlite::{OptionalExtension, Row, params};

use crate::convert::{self, date_text, id_text, to_enum_text, upsert_sql};
use crate::{Db, Result};

const ACTIVITY_COLUMNS: &str =
    "id, hlc, deleted, date, source, kind, duration_min, kcal, external_id";

fn activity_from_row(row: &Row<'_>) -> rusqlite::Result<Activity> {
    Ok(Activity {
        meta: convert::meta(row)?,
        date: convert::date(row, 3)?,
        source: convert::enum_text(row, 4)?,
        kind: row.get(5)?,
        duration_min: row.get(6)?,
        kcal: row.get(7)?,
        external_id: row.get(8)?,
    })
}

impl Db {
    /// Enregistre une pesée. `false` si une version plus récente existe déjà.
    pub fn upsert_weigh_in(&self, weigh_in: &WeighIn) -> Result<bool> {
        let sql = upsert_sql(
            "weigh_ins",
            "id",
            &["id", "hlc", "deleted", "date", "weight_kg", "body_fat_pct"],
        );
        let changed = self.conn.prepare_cached(&sql)?.execute(params![
            id_text(weigh_in.meta.id),
            weigh_in.meta.hlc.to_string(),
            weigh_in.meta.deleted,
            date_text(weigh_in.date),
            weigh_in.weight_kg,
            weigh_in.body_fat_pct,
        ])?;
        Ok(changed > 0)
    }

    /// Pesées d'une période (bornes incluses), par date.
    pub fn weigh_ins_between(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<WeighIn>> {
        let mut statement = self.conn.prepare_cached(
            "SELECT id, hlc, deleted, date, weight_kg, body_fat_pct FROM weigh_ins
             WHERE deleted = 0 AND date BETWEEN ?1 AND ?2 ORDER BY date, id",
        )?;
        let rows = statement.query_map([date_text(from), date_text(to)], |row| {
            Ok(WeighIn {
                meta: convert::meta(row)?,
                date: convert::date(row, 3)?,
                weight_kg: row.get(4)?,
                body_fat_pct: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Enregistre une activité. `false` si une version plus récente existe déjà.
    pub fn upsert_activity(&self, activity: &Activity) -> Result<bool> {
        let columns: Vec<&str> = ACTIVITY_COLUMNS.split(", ").collect();
        let sql = upsert_sql("activities", "id", &columns);
        let changed = self.conn.prepare_cached(&sql)?.execute(params![
            id_text(activity.meta.id),
            activity.meta.hlc.to_string(),
            activity.meta.deleted,
            date_text(activity.date),
            to_enum_text(&activity.source)?,
            activity.kind,
            activity.duration_min,
            activity.kcal,
            activity.external_id,
        ])?;
        Ok(changed > 0)
    }

    /// Activités d'une période (bornes incluses), par date.
    pub fn activities_between(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<Activity>> {
        let sql = format!(
            "SELECT {ACTIVITY_COLUMNS} FROM activities
             WHERE deleted = 0 AND date BETWEEN ?1 AND ?2 ORDER BY date, id"
        );
        let mut statement = self.conn.prepare_cached(&sql)?;
        let rows = statement.query_map([date_text(from), date_text(to)], activity_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Activité déjà importée d'une source externe (pour ne pas importer deux fois la même
    /// activité Strava). Une activité importée puis supprimée par l'utilisatrice est
    /// renvoyée aussi (`meta.deleted`), afin de ne pas la réimporter.
    pub fn activity_by_external_id(
        &self,
        source: ActivitySource,
        external_id: &str,
    ) -> Result<Option<Activity>> {
        let sql = format!(
            "SELECT {ACTIVITY_COLUMNS} FROM activities
             WHERE source = ?1 AND external_id = ?2 ORDER BY hlc DESC LIMIT 1"
        );
        Ok(self
            .conn
            .prepare_cached(&sql)?
            .query_row(
                params![to_enum_text(&source)?, external_id],
                activity_from_row,
            )
            .optional()?)
    }
}
