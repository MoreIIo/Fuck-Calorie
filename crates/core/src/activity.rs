//! Activité physique (§2.6) : saisie manuelle, estimation par MET, import Strava.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::hlc::SyncMeta;

/// Origine d'une activité.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivitySource {
    Manual,
    Strava,
}

/// Une activité enregistrée (§3.8). Ses calories s'ajoutent au budget du jour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub date: NaiveDate,
    pub source: ActivitySource,
    /// Type d'activité : une clé de [`MET_TABLE`] (« cycling_moderate ») ou un libellé libre.
    pub kind: String,
    pub duration_min: f64,
    pub kcal: f64,
    /// Identifiant de l'activité chez Strava, pour ne pas l'importer deux fois.
    pub external_id: Option<String>,
}

/// Estimation des calories d'une activité : `MET × poids (kg) × durée (h)` (§2.6).
#[must_use]
pub fn met_kcal(met: f64, weight_kg: f64, duration_min: f64) -> f64 {
    (met * weight_kg * duration_min / 60.0).max(0.0)
}

/// Liste d'activités proposées à la saisie manuelle, avec leur MET.
///
/// Valeurs indicatives tirées du *Compendium of Physical Activities* (Ainsworth et al.,
/// 2011). La clé sert d'identifiant ; le libellé affiché vient des fichiers de traduction.
pub const MET_TABLE: &[(&str, f64)] = &[
    ("walking", 3.5),
    ("walking_brisk", 4.3),
    ("hiking", 6.0),
    ("running", 9.8),
    ("cycling_leisure", 4.0),
    ("cycling_moderate", 8.0),
    ("cycling_vigorous", 10.0),
    ("swimming", 5.8),
    ("strength_training", 3.5),
    ("strength_training_vigorous", 6.0),
    ("yoga", 2.5),
    ("elliptical", 5.0),
    ("rowing_machine", 7.0),
    ("dancing", 5.0),
    ("football", 7.0),
    ("tennis", 7.3),
];

/// MET d'une activité de [`MET_TABLE`].
#[must_use]
pub fn met_for(kind: &str) -> Option<f64> {
    MET_TABLE
        .iter()
        .find(|(key, _)| *key == kind)
        .map(|(_, met)| *met)
}

/// Calories d'une activité importée de Strava (§2.6).
///
/// Si Strava fournit le travail en **kilojoules** (capteur de puissance), on prend
/// `kcal = kJ × 1` : le rendement musculaire à vélo (~24 %) compense presque exactement la
/// conversion kJ → kcal. Sinon, on prend les calories estimées par Strava.
#[must_use]
pub fn strava_kcal(kilojoules: Option<f64>, calories: Option<f64>) -> Option<f64> {
    kilojoules
        .filter(|kj| *kj > 0.0)
        .or(calories.filter(|kcal| *kcal > 0.0))
}

/// Total des calories d'activité d'un jour (les activités supprimées sont ignorées).
#[must_use]
pub fn activity_kcal_on(activities: &[Activity], date: NaiveDate) -> f64 {
    activities
        .iter()
        .filter(|a| !a.meta.deleted && a.date == date)
        .map(|a| a.kcal.max(0.0))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hlc::HlcClock;
    use uuid::Uuid;

    #[test]
    fn met_estimate() {
        // 1 h de vélo modéré (MET 8) à 70 kg → 560 kcal.
        assert!((met_kcal(8.0, 70.0, 60.0) - 560.0).abs() < 1e-9);
        assert!((met_kcal(3.5, 70.0, 30.0) - 122.5).abs() < 1e-9);
        assert_eq!(met_kcal(8.0, 70.0, -10.0), 0.0);
    }

    #[test]
    fn met_table_lookup() {
        assert_eq!(met_for("cycling_moderate"), Some(8.0));
        assert_eq!(met_for("curling"), None);
        // Clés uniques, MET plausibles.
        for (i, (key, met)) in MET_TABLE.iter().enumerate() {
            assert!((1.0..=20.0).contains(met), "{key}");
            assert!(
                MET_TABLE[i + 1..].iter().all(|(other, _)| other != key),
                "{key}"
            );
        }
    }

    #[test]
    fn strava_prefers_power_meter() {
        assert_eq!(strava_kcal(Some(1_850.0), Some(2_100.0)), Some(1_850.0));
        assert_eq!(strava_kcal(None, Some(640.0)), Some(640.0));
        assert_eq!(strava_kcal(Some(0.0), Some(640.0)), Some(640.0));
        assert_eq!(strava_kcal(None, None), None);
    }

    #[test]
    fn daily_total_ignores_deleted() {
        let mut clock = HlcClock::new(Uuid::from_u128(1));
        let date = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        let mut make = |date, kcal| Activity {
            meta: SyncMeta::new(clock.tick(1)),
            date,
            source: ActivitySource::Manual,
            kind: "cycling_moderate".into(),
            duration_min: 60.0,
            kcal,
            external_id: None,
        };
        let mut activities = vec![
            make(date, 560.0),
            make(date, 200.0),
            make(date.succ_opt().unwrap(), 999.0),
        ];
        assert_eq!(activity_kcal_on(&activities, date), 760.0);
        let hlc = activities[1].meta.hlc;
        activities[1].meta.delete(hlc);
        assert_eq!(activity_kcal_on(&activities, date), 560.0);
    }
}
