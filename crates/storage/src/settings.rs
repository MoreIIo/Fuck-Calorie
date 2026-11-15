//! Réglages propres à l'appareil (clé → valeur), non synchronisés.

use rusqlite::{OptionalExtension, params};

use crate::{Db, Result};

impl Db {
    /// Enregistre un réglage. **Jamais de jeton ni de clé API** (cf. §8.5).
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Lit un réglage.
    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    /// Supprime un réglage.
    pub fn remove_setting(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM settings WHERE key = ?1", [key])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::Db;

    #[test]
    fn settings_round_trip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.setting("theme").unwrap(), None);
        db.set_setting("theme", "system").unwrap();
        db.set_setting("theme", "dark").unwrap();
        assert_eq!(db.setting("theme").unwrap().as_deref(), Some("dark"));
        db.remove_setting("theme").unwrap();
        assert_eq!(db.setting("theme").unwrap(), None);
    }
}
