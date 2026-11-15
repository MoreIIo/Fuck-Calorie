//! Conversions entre les types de `fc-core` et les colonnes SQLite.

use std::error::Error;

use chrono::{NaiveDate, NaiveTime};
use fc_core::hlc::{Hlc, SyncMeta};
use rusqlite::Row;
use rusqlite::types::Type;
use serde::Serialize;
use serde::de::DeserializeOwned;
use uuid::Uuid;

/// Erreur de lecture d'une colonne dont le contenu n'a pas le format attendu.
fn bad(col: usize, err: impl Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(col, Type::Text, Box::new(err))
}

pub(crate) fn uuid(row: &Row<'_>, col: usize) -> rusqlite::Result<Uuid> {
    let text: String = row.get(col)?;
    Uuid::parse_str(&text).map_err(|e| bad(col, e))
}

pub(crate) fn opt_uuid(row: &Row<'_>, col: usize) -> rusqlite::Result<Option<Uuid>> {
    let text: Option<String> = row.get(col)?;
    text.map(|t| Uuid::parse_str(&t).map_err(|e| bad(col, e)))
        .transpose()
}

pub(crate) fn hlc(row: &Row<'_>, col: usize) -> rusqlite::Result<Hlc> {
    let text: String = row.get(col)?;
    text.parse().map_err(|e| bad(col, e))
}

pub(crate) fn date(row: &Row<'_>, col: usize) -> rusqlite::Result<NaiveDate> {
    let text: String = row.get(col)?;
    text.parse().map_err(|e| bad(col, e))
}

pub(crate) fn opt_time(row: &Row<'_>, col: usize) -> rusqlite::Result<Option<NaiveTime>> {
    let text: Option<String> = row.get(col)?;
    text.map(|t| t.parse().map_err(|e| bad(col, e))).transpose()
}

/// Colonne JSON → valeur.
pub(crate) fn json<T: DeserializeOwned>(row: &Row<'_>, col: usize) -> rusqlite::Result<T> {
    let text: String = row.get(col)?;
    serde_json::from_str(&text).map_err(|e| bad(col, e))
}

/// Colonne texte contenant la forme `snake_case` d'une énumération (« off », « strava »).
pub(crate) fn enum_text<T: DeserializeOwned>(row: &Row<'_>, col: usize) -> rusqlite::Result<T> {
    let text: String = row.get(col)?;
    serde_json::from_value(serde_json::Value::String(text)).map_err(|e| bad(col, e))
}

/// Les trois premières colonnes de toute table synchronisée : `id`, `hlc`, `deleted`.
pub(crate) fn meta(row: &Row<'_>) -> rusqlite::Result<SyncMeta> {
    Ok(SyncMeta {
        id: uuid(row, 0)?,
        hlc: hlc(row, 1)?,
        deleted: row.get(2)?,
    })
}

/// Valeur → texte JSON.
pub(crate) fn to_json<T: Serialize>(value: &T) -> crate::Result<String> {
    Ok(serde_json::to_string(value)?)
}

/// Énumération → sa forme texte `snake_case`, sans guillemets.
pub(crate) fn to_enum_text<T: Serialize>(value: &T) -> crate::Result<String> {
    match serde_json::to_value(value)? {
        serde_json::Value::String(text) => Ok(text),
        other => Ok(other.to_string()),
    }
}

pub(crate) fn id_text(id: Uuid) -> String {
    id.as_hyphenated().to_string()
}

pub(crate) fn date_text(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

pub(crate) fn time_text(time: NaiveTime) -> String {
    time.format("%H:%M:%S").to_string()
}

/// Échappe les caractères spéciaux de `LIKE` (avec `ESCAPE '\'`).
pub(crate) fn like_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Requête d'écriture idempotente : insère la ligne, ou la remplace **seulement si la
/// version fournie est plus récente** (HLC supérieur) que celle déjà en base.
pub(crate) fn upsert_sql(table: &str, key: &str, columns: &[&str]) -> String {
    let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("?{i}")).collect();
    let updates: Vec<String> = columns
        .iter()
        .filter(|c| **c != key)
        .map(|c| format!("{c} = excluded.{c}"))
        .collect();
    format!(
        "INSERT INTO {table} ({}) VALUES ({}) ON CONFLICT ({key}) DO UPDATE SET {} \
         WHERE excluded.hlc > {table}.hlc",
        columns.join(", "),
        placeholders.join(", "),
        updates.join(", ")
    )
}
