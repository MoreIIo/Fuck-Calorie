//! Profil historisé (§2.1).

use fc_core::profile::ProfileRevision;
use rusqlite::params;

use crate::convert::{self, date_text, id_text, to_json, upsert_sql};
use crate::{Db, Result};

impl Db {
    /// Enregistre une révision du profil. `false` si une version plus récente existe déjà.
    pub fn upsert_profile_revision(&self, revision: &ProfileRevision) -> Result<bool> {
        let sql = upsert_sql(
            "profile_revisions",
            "id",
            &["id", "hlc", "deleted", "effective_from", "profile"],
        );
        let changed = self.conn.prepare_cached(&sql)?.execute(params![
            id_text(revision.meta.id),
            revision.meta.hlc.to_string(),
            revision.meta.deleted,
            date_text(revision.effective_from),
            to_json(&revision.profile)?,
        ])?;
        Ok(changed > 0)
    }

    /// Toutes les révisions du profil, de la plus ancienne à la plus récente. À combiner
    /// avec [`fc_core::profile::profile_on`] pour obtenir le profil en vigueur à une date.
    pub fn profile_revisions(&self) -> Result<Vec<ProfileRevision>> {
        let mut statement = self.conn.prepare_cached(
            "SELECT id, hlc, deleted, effective_from, profile FROM profile_revisions
             WHERE deleted = 0 ORDER BY effective_from, hlc",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ProfileRevision {
                meta: convert::meta(row)?,
                effective_from: convert::date(row, 3)?,
                profile: convert::json(row, 4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
