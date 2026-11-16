//! Journal alimentaire : repas, entrées, pesées et bilan du jour (§3.5, §3.6, §3.8, §5.2).

use chrono::{NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::activity::{Activity, activity_kcal_on};
use crate::food::{Food, QuantityError, Unit};
use crate::goal::{CalorieTarget, clamp_recovery_factor, day_target_kcal};
use crate::hlc::{Hlc, SyncMeta};
use crate::nutrients::Nutrients;

/// Un repas configurable (§3.5) : nom, ordre d'affichage et plage horaire facultative, qui
/// sert à pré-sélectionner le bon repas à l'ajout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MealSlot {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub name: String,
    pub order: u32,
    pub time_start: Option<NaiveTime>,
    pub time_end: Option<NaiveTime>,
}

impl MealSlot {
    /// L'heure `time` tombe-t-elle dans la plage du repas ? (plage à cheval sur minuit gérée)
    #[must_use]
    pub fn covers(&self, time: NaiveTime) -> bool {
        match (self.time_start, self.time_end) {
            (Some(start), Some(end)) if start <= end => time >= start && time < end,
            (Some(start), Some(end)) => time >= start || time < end,
            _ => false,
        }
    }
}

/// Les quatre repas par défaut, avec leurs plages horaires. `names` donne les noms dans la
/// langue de l'interface (en français : Petit-déjeuner, Déjeuner, Dîner, Collation) ; le
/// dernier n'a pas de plage et sert de repli le reste de la journée.
#[cfg(feature = "rng")]
#[must_use]
pub fn default_meal_slots(names: [&str; 4], mut next_hlc: impl FnMut() -> Hlc) -> Vec<MealSlot> {
    let hm = |h, m| NaiveTime::from_hms_opt(h, m, 0);
    let ranges = [
        (hm(5, 0), hm(10, 30)),
        (hm(11, 30), hm(14, 30)),
        (hm(18, 30), hm(22, 0)),
        (None, None),
    ];
    names
        .into_iter()
        .zip(ranges)
        .zip(0u32..)
        .map(|((name, (time_start, time_end)), order)| MealSlot {
            meta: SyncMeta::new(next_hlc()),
            name: name.to_owned(),
            order,
            time_start,
            time_end,
        })
        .collect()
}

/// Repas à pré-sélectionner pour un ajout à l'heure `time` : celui dont la plage contient
/// l'heure ; à défaut, le premier repas sans plage (la collation) ; à défaut, le dernier.
#[must_use]
pub fn slot_for_time(slots: &[MealSlot], time: NaiveTime) -> Option<&MealSlot> {
    let mut active: Vec<&MealSlot> = slots.iter().filter(|s| !s.meta.deleted).collect();
    active.sort_by_key(|s| s.order);
    active
        .iter()
        .find(|s| s.covers(time))
        .or_else(|| {
            active
                .iter()
                .find(|s| s.time_start.is_none() || s.time_end.is_none())
        })
        .or(active.last())
        .copied()
}

/// Une ligne du journal (§3.8).
///
/// `nutrients_snapshot` fige les valeurs au moment de la saisie : l'historique ne change pas
/// si la fiche de l'aliment est corrigée plus tard. Un **ajout rapide** (§3.6) est une
/// entrée sans `food_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub date: NaiveDate,
    pub meal_slot_id: Uuid,
    pub food_id: Option<Uuid>,
    /// Nom affiché (nom de l'aliment, ou libellé libre d'un ajout rapide).
    pub label: String,
    pub quantity: f64,
    pub unit: Unit,
    pub grams: f64,
    pub nutrients_snapshot: Nutrients,
    /// Moment de la saisie, en millisecondes Unix.
    pub created_at_ms: i64,
}

impl LogEntry {
    /// Entrée créée à partir d'une fiche aliment.
    pub fn from_food(
        meta: SyncMeta,
        date: NaiveDate,
        meal_slot_id: Uuid,
        food: &Food,
        quantity: f64,
        unit: Unit,
        created_at_ms: i64,
    ) -> Result<Self, QuantityError> {
        let grams = food.grams_for(quantity, &unit)?;
        Ok(Self {
            meta,
            date,
            meal_slot_id,
            food_id: Some(food.meta.id),
            label: food.name.clone(),
            quantity,
            unit,
            grams,
            nutrients_snapshot: food.per_100.for_grams(grams),
            created_at_ms,
        })
    }

    /// Ajout rapide : des calories (et éventuellement des macros) avec un libellé libre.
    #[must_use]
    pub fn quick_add(
        meta: SyncMeta,
        date: NaiveDate,
        meal_slot_id: Uuid,
        label: impl Into<String>,
        nutrients: Nutrients,
        created_at_ms: i64,
    ) -> Self {
        Self {
            meta,
            date,
            meal_slot_id,
            food_id: None,
            label: label.into(),
            quantity: 1.0,
            unit: Unit::Piece,
            grams: 0.0,
            nutrients_snapshot: nutrients,
            created_at_ms,
        }
    }

    #[must_use]
    pub fn is_quick_add(&self) -> bool {
        self.food_id.is_none()
    }

    /// Change la quantité d'une entrée et recalcule son instantané à partir de la fiche.
    pub fn set_quantity(
        &mut self,
        food: &Food,
        quantity: f64,
        unit: Unit,
        hlc: Hlc,
    ) -> Result<(), QuantityError> {
        let grams = food.grams_for(quantity, &unit)?;
        self.quantity = quantity;
        self.unit = unit;
        self.grams = grams;
        self.nutrients_snapshot = food.per_100.for_grams(grams);
        self.meta.touch(hlc);
        Ok(())
    }

    /// Copie de l'entrée vers une autre date et/ou un autre repas (§3.6 : copier un repas ou
    /// une journée ; §3.5 : dupliquer). La copie est une nouvelle entrée indépendante.
    #[must_use]
    pub fn copied_to(
        &self,
        meta: SyncMeta,
        date: NaiveDate,
        meal_slot_id: Uuid,
        created_at_ms: i64,
    ) -> Self {
        Self {
            meta,
            date,
            meal_slot_id,
            created_at_ms,
            ..self.clone()
        }
    }
}

/// Élément d'un repas enregistré.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedMealItem {
    pub food_id: Uuid,
    pub quantity: f64,
    pub unit: Unit,
}

/// Repas enregistré (« Mon petit-déj »), ajouté en un tap (§3.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedMeal {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub name: String,
    pub items: Vec<SavedMealItem>,
}

/// Une pesée (§2.8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeighIn {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub date: NaiveDate,
    pub weight_kg: f64,
    pub body_fat_pct: Option<f64>,
}

/// Pesées valides sous la forme attendue par [`crate::trend::trend_series`].
#[must_use]
pub fn weigh_in_points(weigh_ins: &[WeighIn]) -> Vec<(NaiveDate, f64)> {
    weigh_ins
        .iter()
        .filter(|w| !w.meta.deleted)
        .map(|w| (w.date, w.weight_kg))
        .collect()
}

/// État d'une journée : bouton *Journée terminée* (§3.5) et note libre. Une seule par date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayStatus {
    pub date: NaiveDate,
    pub complete: bool,
    pub note: Option<String>,
    pub hlc: Hlc,
}

/// Total des nutriments d'un ensemble d'entrées (les entrées supprimées sont ignorées).
#[must_use]
pub fn total_nutrients<'a>(entries: impl IntoIterator<Item = &'a LogEntry>) -> Nutrients {
    entries
        .into_iter()
        .filter(|e| !e.meta.deleted)
        .map(|e| e.nutrients_snapshot)
        .sum()
}

/// Total d'un repas pour une date.
#[must_use]
pub fn meal_total(entries: &[LogEntry], date: NaiveDate, meal_slot_id: Uuid) -> Nutrients {
    total_nutrients(
        entries
            .iter()
            .filter(|e| e.date == date && e.meal_slot_id == meal_slot_id),
    )
}

/// Bilan d'une journée, tel qu'affiché par la carte héro (§5.2) :
/// `cible − mangé + activité = restant`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DaySummary {
    /// Cible de base (hors activités).
    pub base_target_kcal: f64,
    /// Calories d'activité ajoutées au budget (après facteur de récupération).
    pub activity_credit_kcal: f64,
    /// Cible du jour = cible de base + activités créditées.
    pub target_kcal: f64,
    /// Total mangé.
    pub eaten: Nutrients,
    /// Calories restantes (négatif en cas de dépassement).
    pub remaining_kcal: f64,
}

impl DaySummary {
    /// La cible est-elle dépassée ?
    #[must_use]
    pub fn is_over(&self) -> bool {
        self.remaining_kcal < 0.0
    }

    /// Part de la cible déjà consommée, entre 0 et 1 (pour la jauge).
    #[must_use]
    pub fn progress(&self) -> f64 {
        if self.target_kcal <= 0.0 {
            return 0.0;
        }
        (self.eaten.kcal / self.target_kcal).clamp(0.0, 1.0)
    }
}

/// Calcule le bilan du jour `date`.
#[must_use]
pub fn day_summary(
    date: NaiveDate,
    base: &CalorieTarget,
    entries: &[LogEntry],
    activities: &[Activity],
    recovery_factor: f64,
) -> DaySummary {
    let eaten = total_nutrients(entries.iter().filter(|e| e.date == date));
    let activity_kcal = activity_kcal_on(activities, date);
    let target_kcal = day_target_kcal(base, activity_kcal, recovery_factor);
    DaySummary {
        base_target_kcal: base.kcal,
        activity_credit_kcal: activity_kcal * clamp_recovery_factor(recovery_factor),
        target_kcal,
        eaten,
        remaining_kcal: target_kcal - eaten.kcal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::ActivitySource;
    use crate::food::tests::yaourt;
    use crate::goal::{Goal, base_target};
    use crate::hlc::HlcClock;
    use crate::profile::Sex;

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 15).unwrap()
    }

    fn time(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn clock() -> HlcClock {
        HlcClock::new(Uuid::from_u128(1))
    }

    const NAMES: [&str; 4] = ["Petit-déjeuner", "Déjeuner", "Dîner", "Collation"];

    #[test]
    fn default_slots_and_time_selection() {
        let mut clock = clock();
        let slots = default_meal_slots(NAMES, || clock.tick(1));
        assert_eq!(slots.len(), 4);
        assert_eq!(
            slots.iter().map(|s| s.order).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        let name_at = |h, m| slot_for_time(&slots, time(h, m)).unwrap().name.as_str();
        assert_eq!(name_at(7, 30), "Petit-déjeuner");
        assert_eq!(name_at(12, 45), "Déjeuner");
        assert_eq!(name_at(20, 0), "Dîner");
        // Hors plage : collation.
        assert_eq!(name_at(16, 0), "Collation");
        assert_eq!(name_at(23, 30), "Collation");
        // Borne de fin exclue.
        assert_eq!(name_at(10, 30), "Collation");
    }

    #[test]
    fn slot_selection_edge_cases() {
        let mut clock = clock();
        assert!(slot_for_time(&[], time(12, 0)).is_none());
        let mut slots = default_meal_slots(NAMES, || clock.tick(1));
        // Repas de nuit à cheval sur minuit.
        slots[3].time_start = Some(time(22, 0));
        slots[3].time_end = Some(time(2, 0));
        assert_eq!(
            slot_for_time(&slots, time(23, 30)).unwrap().name,
            "Collation"
        );
        assert_eq!(slot_for_time(&slots, time(1, 0)).unwrap().name, "Collation");
        // Aucun repas ne couvre 16 h et aucun n'est sans plage : dernier repas.
        assert_eq!(
            slot_for_time(&slots, time(16, 0)).unwrap().name,
            "Collation"
        );
        // Un repas supprimé n'est jamais proposé.
        let hlc = clock.tick(2);
        slots[0].meta.delete(hlc);
        assert_ne!(
            slot_for_time(&slots, time(7, 30)).unwrap().name,
            "Petit-déjeuner"
        );
    }

    #[test]
    fn entry_from_food_snapshots_nutrients() {
        let mut clock = clock();
        let slot = Uuid::from_u128(7);
        let mut food = yaourt();
        let unit = Unit::Portion("1 pot".into());
        let entry = LogEntry::from_food(
            SyncMeta::new(clock.tick(1)),
            date(),
            slot,
            &food,
            1.0,
            unit,
            0,
        )
        .unwrap();
        assert_eq!(entry.grams, 125.0);
        assert!((entry.nutrients_snapshot.kcal - 76.25).abs() < 1e-9);
        assert_eq!(entry.food_id, Some(food.meta.id));
        assert!(!entry.is_quick_add());
        // La fiche est corrigée plus tard : l'historique ne change pas.
        food.per_100.kcal = Some(90.0);
        assert!((entry.nutrients_snapshot.kcal - 76.25).abs() < 1e-9);
        // Quantité invalide refusée.
        let err = LogEntry::from_food(
            SyncMeta::new(clock.tick(2)),
            date(),
            slot,
            &food,
            0.0,
            Unit::Gram,
            0,
        );
        assert_eq!(err.unwrap_err(), QuantityError::InvalidAmount);
    }

    #[test]
    fn edit_and_copy() {
        let mut clock = clock();
        let slot = Uuid::from_u128(7);
        let food = yaourt();
        let mut entry = LogEntry::from_food(
            SyncMeta::new(clock.tick(1)),
            date(),
            slot,
            &food,
            100.0,
            Unit::Gram,
            0,
        )
        .unwrap();
        let hlc = clock.tick(2);
        entry.set_quantity(&food, 200.0, Unit::Gram, hlc).unwrap();
        assert!((entry.nutrients_snapshot.kcal - 122.0).abs() < 1e-9);
        assert_eq!(entry.meta.hlc, hlc);
        // Modification invalide : l'entrée reste intacte.
        assert!(
            entry
                .set_quantity(&food, -1.0, Unit::Gram, clock.tick(3))
                .is_err()
        );
        assert_eq!(entry.grams, 200.0);

        let tomorrow = date().succ_opt().unwrap();
        let other_slot = Uuid::from_u128(8);
        let copy = entry.copied_to(SyncMeta::new(clock.tick(4)), tomorrow, other_slot, 99);
        assert_ne!(copy.meta.id, entry.meta.id);
        assert_eq!(
            (copy.date, copy.meal_slot_id, copy.created_at_ms),
            (tomorrow, other_slot, 99)
        );
        assert_eq!(copy.nutrients_snapshot, entry.nutrients_snapshot);
    }

    #[test]
    fn totals_and_day_summary() {
        let mut clock = clock();
        let (breakfast, lunch) = (Uuid::from_u128(7), Uuid::from_u128(8));
        let food = yaourt();
        let mut entries = vec![
            LogEntry::from_food(
                SyncMeta::new(clock.tick(1)),
                date(),
                breakfast,
                &food,
                200.0,
                Unit::Gram,
                0,
            )
            .unwrap(), // 122 kcal
            LogEntry::quick_add(
                SyncMeta::new(clock.tick(2)),
                date(),
                lunch,
                "Resto italien",
                Nutrients {
                    kcal: 900.0,
                    ..Nutrients::ZERO
                },
                0,
            ),
            LogEntry::quick_add(
                SyncMeta::new(clock.tick(3)),
                date().succ_opt().unwrap(), // un autre jour
                lunch,
                "Demain",
                Nutrients {
                    kcal: 5_000.0,
                    ..Nutrients::ZERO
                },
                0,
            ),
        ];
        assert!(entries[1].is_quick_add());
        assert!((meal_total(&entries, date(), breakfast).kcal - 122.0).abs() < 1e-9);
        assert!((meal_total(&entries, date(), lunch).kcal - 900.0).abs() < 1e-9);

        let activities = vec![Activity {
            meta: SyncMeta::new(clock.tick(4)),
            date: date(),
            source: ActivitySource::Strava,
            kind: "cycling_moderate".into(),
            duration_min: 90.0,
            kcal: 800.0,
            external_id: Some("123".into()),
        }];
        let base = base_target(
            2_300.0,
            Goal::Lose { kg_per_week: 0.5 },
            Sex::Female,
            70.0,
            None,
        );

        // cible 1 750 − mangé 1 022 + activité 800 = 1 528 restantes
        let s = day_summary(date(), &base, &entries, &activities, 1.0);
        assert!((s.target_kcal - 2_550.0).abs() < 1e-9);
        assert!((s.eaten.kcal - 1_022.0).abs() < 1e-9);
        assert!((s.remaining_kcal - 1_528.0).abs() < 1e-9);
        assert!((s.base_target_kcal + s.activity_credit_kcal - s.target_kcal).abs() < 1e-9);
        assert!(!s.is_over());
        assert!((s.progress() - 1_022.0 / 2_550.0).abs() < 1e-9);

        // Activité récupérée à 50 %.
        let s = day_summary(date(), &base, &entries, &activities, 0.5);
        assert!((s.activity_credit_kcal - 400.0).abs() < 1e-9);
        assert!((s.remaining_kcal - 1_128.0).abs() < 1e-9);

        // Entrée supprimée : ne compte plus.
        let hlc = clock.tick(5);
        entries[1].meta.delete(hlc);
        let s = day_summary(date(), &base, &entries, &activities, 1.0);
        assert!((s.eaten.kcal - 122.0).abs() < 1e-9);

        // Dépassement.
        entries.push(LogEntry::quick_add(
            SyncMeta::new(clock.tick(6)),
            date(),
            lunch,
            "Buffet",
            Nutrients {
                kcal: 3_000.0,
                ..Nutrients::ZERO
            },
            0,
        ));
        let s = day_summary(date(), &base, &entries, &activities, 1.0);
        assert!(s.is_over());
        assert_eq!(s.progress(), 1.0);
    }

    #[test]
    fn progress_without_target() {
        let s = DaySummary {
            base_target_kcal: 0.0,
            activity_credit_kcal: 0.0,
            target_kcal: 0.0,
            eaten: Nutrients::ZERO,
            remaining_kcal: 0.0,
        };
        assert_eq!(s.progress(), 0.0);
    }

    #[test]
    fn weigh_in_points_skip_deleted() {
        let mut clock = clock();
        let mut weigh_ins = vec![
            WeighIn {
                meta: SyncMeta::new(clock.tick(1)),
                date: date(),
                weight_kg: 70.4,
                body_fat_pct: None,
            },
            WeighIn {
                meta: SyncMeta::new(clock.tick(2)),
                date: date(),
                weight_kg: 700.4, // faute de frappe, supprimée ensuite
                body_fat_pct: None,
            },
        ];
        let hlc = clock.tick(3);
        weigh_ins[1].meta.delete(hlc);
        assert_eq!(weigh_in_points(&weigh_ins), vec![(date(), 70.4)]);
    }

    #[test]
    fn serde_round_trip() {
        let mut clock = clock();
        let entry = LogEntry::from_food(
            SyncMeta::new(clock.tick(1)),
            date(),
            Uuid::from_u128(7),
            &yaourt(),
            1.0,
            Unit::Portion("1 pot".into()),
            1_794_758_275_067,
        )
        .unwrap();
        let json = serde_json::to_string(&entry).unwrap();
        assert_eq!(serde_json::from_str::<LogEntry>(&json).unwrap(), entry);
        let saved = SavedMeal {
            meta: SyncMeta::new(clock.tick(2)),
            name: "Mon petit-déj".into(),
            items: vec![SavedMealItem {
                food_id: Uuid::from_u128(100),
                quantity: 1.0,
                unit: Unit::Portion("1 pot".into()),
            }],
        };
        let json = serde_json::to_string(&saved).unwrap();
        assert_eq!(serde_json::from_str::<SavedMeal>(&json).unwrap(), saved);
    }
}
