//! # fc-storage — stockage local SQLite de Fuck Calorie
//!
//! - Schéma et **migrations versionnées** (`migrations/`), appliquées à l'ouverture (§8.4).
//! - Toutes les écritures passent par des transactions SQLite : aucune perte si
//!   l'application est tuée pendant une saisie.
//! - Écritures **idempotentes** : une ligne n'est remplacée que par une version plus récente
//!   (comparaison des horodatages HLC). Rejouer deux fois la même écriture, ou recevoir une
//!   ancienne version après une nouvelle, ne change rien — c'est ce qui rend la
//!   synchronisation sûre (§7.2, §8.4).
//! - Les suppressions sont des « tombstones » : la ligne reste, marquée `deleted`, et
//!   disparaît des lectures.
//!
//! Aucun jeton ni clé API ne doit être stocké ici : ils vont dans le coffre sécurisé de la
//! plateforme (§8.5).

mod body;
mod convert;
mod foods;
mod journal;
mod profile;
mod settings;

use std::path::Path;

use rusqlite::Connection;

/// Erreur du stockage local.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("erreur SQLite : {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("donnée JSON invalide : {0}")]
    Json(#[from] serde_json::Error),
    #[error(
        "cette base a été créée par une version plus récente de l'application \
         (schéma {found}, version maximale connue : {supported})"
    )]
    SchemaTooNew { found: u32, supported: u32 },
}

pub type Result<T> = std::result::Result<T, StorageError>;

/// Migrations du schéma, dans l'ordre. L'indice + 1 est le numéro de version, enregistré
/// dans `PRAGMA user_version`. **Ne jamais modifier une migration publiée** : en ajouter une.
const MIGRATIONS: &[&str] = &[include_str!("../migrations/0001_initial.sql")];

/// Version du schéma que cette version du code sait lire.
pub const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;

/// Base de données locale.
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Ouvre (ou crée) la base au chemin `path` et applique les migrations manquantes.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        // WAL : les lectures ne bloquent pas les écritures, et la base résiste mieux à un
        // arrêt brutal.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::init(conn)
    }

    /// Base en mémoire, pour les tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        migrate(&mut conn, MIGRATIONS)?;
        Ok(Self { conn })
    }

    /// Version du schéma de la base ouverte.
    pub fn schema_version(&self) -> Result<u32> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }
}

/// Applique, une par une et chacune dans sa transaction, les migrations que la base n'a pas
/// encore reçues. Si une migration échoue, la base reste dans l'état de la version précédente.
fn migrate(conn: &mut Connection, migrations: &[&str]) -> Result<()> {
    let current: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let supported = migrations.len() as u32;
    if current > supported {
        return Err(StorageError::SchemaTooNew {
            found: current,
            supported,
        });
    }
    for (index, sql) in migrations.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index as u32 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_database_is_at_latest_version() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
        let tables: Vec<String> = db
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            tables,
            [
                "activities",
                "day_status",
                "foods",
                "log_entries",
                "meal_slots",
                "profile_revisions",
                "saved_meals",
                "settings",
                "weigh_ins"
            ]
        );
    }

    #[test]
    fn reopening_keeps_data_and_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fc.db");
        {
            let db = Db::open(&path).unwrap();
            db.set_setting("theme", "dark").unwrap();
        }
        let db = Db::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(db.setting("theme").unwrap().as_deref(), Some("dark"));
    }

    #[test]
    fn upgrade_applies_only_missing_migrations_and_keeps_data() {
        let v1 = "CREATE TABLE t (a INTEGER); INSERT INTO t VALUES (1);";
        let v2 = "ALTER TABLE t ADD COLUMN b TEXT DEFAULT 'x';";
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn, &[v1]).unwrap();
        // Montée de version : seule la migration 2 est jouée, la donnée est conservée.
        migrate(&mut conn, &[v1, v2]).unwrap();
        migrate(&mut conn, &[v1, v2]).unwrap(); // idempotent
        let row: (i64, String) = conn
            .query_row("SELECT a, b FROM t", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(row, (1, "x".to_owned()));
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, 2);
    }

    #[test]
    fn failed_migration_leaves_previous_version_intact() {
        let v1 = "CREATE TABLE t (a INTEGER); INSERT INTO t VALUES (1);";
        let broken = "ALTER TABLE t ADD COLUMN b TEXT; INSERT INTO missing VALUES (1);";
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn, &[v1]).unwrap();
        assert!(migrate(&mut conn, &[v1, broken]).is_err());
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, 1);
        // La colonne `b` de la migration ratée n'existe pas.
        assert!(conn.prepare("SELECT b FROM t").is_err());
    }

    #[test]
    fn newer_schema_is_refused() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        let err = migrate(&mut conn, MIGRATIONS).unwrap_err();
        assert!(matches!(err, StorageError::SchemaTooNew { found: 99, .. }));
        assert!(err.to_string().contains("plus récente"));
    }
}
