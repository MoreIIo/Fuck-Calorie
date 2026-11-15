//! Algorithme adaptatif : la vraie dépense déduite de ce que l'on mange et de l'évolution
//! réelle du poids (§2.3).
//!
//! Sur une fenêtre glissante de 21 jours :
//!
//! ```text
//! TDEE_base = apport moyen − activités enregistrées moyennes − variation de tendance × 7 700
//!             (kcal/jour)    (kcal/jour)                        (kg/jour)
//! ```
//!
//! C'est la formule du cahier des charges, écrite « par jour » : quand les 21 jours sont
//! journalisés, les deux écritures donnent exactement le même résultat. Quand il manque des
//! jours, l'apport des jours non journalisés est supposé égal à la moyenne des jours
//! journalisés (plutôt que zéro, ce qui fausserait le calcul).

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::energy::KCAL_PER_KG;
use crate::trend::{trend_on, trend_series};

/// Fenêtre de calcul, en jours.
pub const WINDOW_DAYS: u32 = 21;
/// Nombre minimum de jours journalisés dans la fenêtre.
pub const MIN_LOGGED_DAYS: usize = 14;
/// Nombre minimum de jours avec pesée dans la fenêtre.
pub const MIN_WEIGH_INS: usize = 6;
/// Amortissement : le TDEE ne bouge pas de plus de ±100 kcal par recalcul hebdomadaire.
pub const MAX_WEEKLY_CHANGE_KCAL: f64 = 100.0;
/// Un jour non marqué « terminé » est ignoré s'il est sous 50 % de sa cible.
pub const INCOMPLETE_RATIO: f64 = 0.5;

/// Ce que l'algorithme a besoin de savoir d'une journée.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DayRecord {
    pub date: NaiveDate,
    /// Calories mangées (total du journal).
    pub eaten_kcal: f64,
    /// Calories des activités enregistrées ce jour-là.
    pub activity_kcal: f64,
    /// Cible du jour (sert à repérer les journées manifestement incomplètes).
    pub target_kcal: f64,
    /// L'utilisatrice a appuyé sur *Journée terminée*.
    pub marked_complete: bool,
}

impl DayRecord {
    /// Ce jour compte-t-il comme journalisé ?
    ///
    /// Oui s'il est marqué *terminé*. Sinon, seulement s'il n'est pas manifestement
    /// incomplet (au moins 50 % de la cible).
    #[must_use]
    pub fn counts_as_logged(&self) -> bool {
        if self.marked_complete {
            return true;
        }
        self.eaten_kcal > 0.0 && self.eaten_kcal >= INCOMPLETE_RATIO * self.target_kcal
    }
}

/// Résultat du recalcul hebdomadaire.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AdaptiveOutcome {
    /// Assez de données : nouvelle estimation.
    Estimated(AdaptiveEstimate),
    /// Pas assez de données : on garde l'estimation précédente.
    InsufficientData {
        /// Jours journalisés dans la fenêtre (minimum requis : 14).
        logged_days: usize,
        /// Jours avec pesée dans la fenêtre (minimum requis : 6).
        weigh_in_days: usize,
    },
}

/// Détail d'une estimation, pour pouvoir expliquer le changement à l'utilisatrice
/// (« Ta dépense estimée passe de 2 310 à 2 380 kcal »).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveEstimate {
    /// Dépense de base retenue (hors activités enregistrées), après amortissement.
    pub tdee_base: f64,
    /// Dépense de base calculée, avant amortissement.
    pub raw_tdee_base: f64,
    /// Estimation précédente.
    pub previous_tdee: f64,
    pub logged_days: usize,
    pub weigh_in_days: usize,
    /// Apport moyen des jours journalisés, en kcal/jour.
    pub mean_intake_kcal: f64,
    /// Activités enregistrées moyennes sur la fenêtre, en kcal/jour.
    pub mean_activity_kcal: f64,
    /// Variation du poids de tendance, ramenée à une semaine (négatif = perte).
    pub trend_kg_per_week: f64,
}

impl AdaptiveEstimate {
    /// L'amortissement a-t-il limité le changement ?
    #[must_use]
    pub fn was_damped(&self) -> bool {
        (self.raw_tdee_base - self.tdee_base).abs() > 1e-9
    }
}

/// Recalcule la dépense de base sur la fenêtre de 21 jours qui se termine à `window_end`.
///
/// - `days` : les journées connues (l'ordre n'a pas d'importance ; les jours absents sont
///   considérés comme non journalisés).
/// - `weigh_ins` : toutes les pesées disponibles (la tendance a besoin de l'historique).
/// - `previous_tdee` : l'estimation en cours (au début, le TDEE initial du §2.2).
/// - `alpha` : coefficient de lissage du poids de tendance.
#[must_use]
pub fn estimate_tdee(
    window_end: NaiveDate,
    days: &[DayRecord],
    weigh_ins: &[(NaiveDate, f64)],
    previous_tdee: f64,
    alpha: f64,
) -> AdaptiveOutcome {
    let window_start = window_end - Days::new(u64::from(WINDOW_DAYS - 1));
    let in_window = |date: NaiveDate| date >= window_start && date <= window_end;

    // Une seule entrée par date (la dernière fournie gagne).
    let window_days: BTreeMap<NaiveDate, &DayRecord> = days
        .iter()
        .filter(|d| in_window(d.date))
        .map(|d| (d.date, d))
        .collect();
    let logged: Vec<&DayRecord> = window_days
        .values()
        .copied()
        .filter(|d| d.counts_as_logged())
        .collect();
    let weigh_in_days = weigh_ins
        .iter()
        .filter(|(date, kg)| in_window(*date) && kg.is_finite() && *kg > 0.0)
        .map(|(date, _)| *date)
        .collect::<BTreeSet<_>>()
        .len();

    let insufficient = AdaptiveOutcome::InsufficientData {
        logged_days: logged.len(),
        weigh_in_days,
    };
    if logged.len() < MIN_LOGGED_DAYS || weigh_in_days < MIN_WEIGH_INS {
        return insufficient;
    }

    // Variation quotidienne du poids de tendance sur la fenêtre.
    let series = trend_series(weigh_ins, window_end, alpha);
    let Some(first) = series.first() else {
        return insufficient;
    };
    let trend_start_date = first.date.max(window_start);
    let span_days = (window_end - trend_start_date).num_days();
    let (Some(trend_start), Some(trend_end)) = (
        trend_on(&series, trend_start_date),
        trend_on(&series, window_end),
    ) else {
        return insufficient;
    };
    if span_days < 1 {
        return insufficient;
    }
    let kg_per_day = (trend_end - trend_start) / span_days as f64;

    let mean_intake_kcal = logged.iter().map(|d| d.eaten_kcal).sum::<f64>() / logged.len() as f64;
    let mean_activity_kcal = window_days
        .values()
        .map(|d| d.activity_kcal.max(0.0))
        .sum::<f64>()
        / f64::from(WINDOW_DAYS);

    let raw_tdee_base = mean_intake_kcal - mean_activity_kcal - kg_per_day * KCAL_PER_KG;
    if !raw_tdee_base.is_finite() {
        return insufficient;
    }
    let change =
        (raw_tdee_base - previous_tdee).clamp(-MAX_WEEKLY_CHANGE_KCAL, MAX_WEEKLY_CHANGE_KCAL);

    AdaptiveOutcome::Estimated(AdaptiveEstimate {
        tdee_base: previous_tdee + change,
        raw_tdee_base,
        previous_tdee,
        logged_days: logged.len(),
        weigh_in_days,
        mean_intake_kcal,
        mean_activity_kcal,
        trend_kg_per_week: kg_per_day * 7.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(n: u64) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + Days::new(n)
    }

    fn record(n: u64, eaten: f64, activity: f64) -> DayRecord {
        DayRecord {
            date: day(n),
            eaten_kcal: eaten,
            activity_kcal: activity,
            target_kcal: 1_800.0,
            marked_complete: true,
        }
    }

    fn estimated(outcome: AdaptiveOutcome) -> AdaptiveEstimate {
        match outcome {
            AdaptiveOutcome::Estimated(e) => e,
            other => panic!("estimation attendue, obtenu {other:?}"),
        }
    }

    /// Simule une personne dont la vraie dépense de base est `true_tdee` : son poids évolue
    /// exactement selon le bilan énergétique. Renvoie (journées, pesées) pour `n` jours.
    fn simulate(
        n: u64,
        true_tdee: f64,
        eaten: impl Fn(u64) -> f64,
        activity: impl Fn(u64) -> f64,
    ) -> (Vec<DayRecord>, Vec<(NaiveDate, f64)>) {
        let mut weight = 70.0;
        let mut days = Vec::new();
        let mut weigh_ins = Vec::new();
        for i in 0..n {
            weigh_ins.push((day(i), weight)); // pesée du matin
            days.push(record(i, eaten(i), activity(i)));
            weight += (eaten(i) - true_tdee - activity(i)) / KCAL_PER_KG;
        }
        (days, weigh_ins)
    }

    #[test]
    fn day_counting_rules() {
        let base = record(0, 1_700.0, 0.0);
        assert!(base.counts_as_logged());
        // Non marqué mais plausible (≥ 50 % de la cible) : compte.
        assert!(
            DayRecord {
                marked_complete: false,
                eaten_kcal: 950.0,
                ..base
            }
            .counts_as_logged()
        );
        // Non marqué et manifestement incomplet : exclu.
        assert!(
            !DayRecord {
                marked_complete: false,
                eaten_kcal: 600.0,
                ..base
            }
            .counts_as_logged()
        );
        // Rien de saisi : exclu, même sans cible connue.
        let empty = DayRecord {
            marked_complete: false,
            eaten_kcal: 0.0,
            target_kcal: 0.0,
            ..base
        };
        assert!(!empty.counts_as_logged());
        // Marqué « terminé » avec très peu : compte (c'est l'utilisatrice qui décide).
        assert!(
            DayRecord {
                eaten_kcal: 300.0,
                ..base
            }
            .counts_as_logged()
        );
    }

    #[test]
    fn stable_weight_means_tdee_equals_intake() {
        let (days, weigh_ins) = simulate(21, 2_000.0, |_| 2_000.0, |_| 0.0);
        let e = estimated(estimate_tdee(day(20), &days, &weigh_ins, 2_000.0, 0.1));
        assert!((e.raw_tdee_base - 2_000.0).abs() < 1e-6);
        assert!((e.tdee_base - 2_000.0).abs() < 1e-6);
        assert!(!e.was_damped());
        assert_eq!((e.logged_days, e.weigh_in_days), (21, 21));
        assert!(e.trend_kg_per_week.abs() < 1e-9);
    }

    #[test]
    fn converges_to_true_expenditure() {
        // Vraie dépense 2 300 kcal, apport 1 750 kcal : perte de 0,5 kg/semaine.
        let (days, weigh_ins) = simulate(120, 2_300.0, |_| 1_750.0, |_| 0.0);
        let e = estimated(estimate_tdee(day(119), &days, &weigh_ins, 2_300.0, 0.1));
        assert!(
            (e.raw_tdee_base - 2_300.0).abs() < 5.0,
            "{}",
            e.raw_tdee_base
        );
        assert!((e.trend_kg_per_week + 0.5).abs() < 0.01);
    }

    #[test]
    fn recorded_activities_are_not_counted_twice() {
        // Dépense de base 2 000 ; une sortie de 700 kcal tous les 3 jours, entièrement
        // compensée dans l'assiette : le poids ne bouge pas.
        let activity = |i: u64| if i.is_multiple_of(3) { 700.0 } else { 0.0 };
        let (days, weigh_ins) = simulate(21, 2_000.0, |i| 2_000.0 + activity(i), activity);
        let e = estimated(estimate_tdee(day(20), &days, &weigh_ins, 2_000.0, 0.1));
        // La dépense de base reste 2 000 : les activités sont retirées, pas absorbées.
        assert!(
            (e.raw_tdee_base - 2_000.0).abs() < 1e-6,
            "{}",
            e.raw_tdee_base
        );
        assert!((e.mean_activity_kcal - 7.0 * 700.0 / 21.0).abs() < 1e-9);
    }

    #[test]
    fn requires_enough_logged_days_and_weigh_ins() {
        let (days, weigh_ins) = simulate(21, 2_000.0, |_| 2_000.0, |_| 0.0);

        // 13 jours journalisés seulement.
        let few_days: Vec<DayRecord> = days.iter().copied().take(13).collect();
        assert_eq!(
            estimate_tdee(day(20), &few_days, &weigh_ins, 2_000.0, 0.1),
            AdaptiveOutcome::InsufficientData {
                logged_days: 13,
                weigh_in_days: 21
            }
        );
        // 14 jours : suffisant.
        let enough: Vec<DayRecord> = days.iter().copied().take(14).collect();
        assert!(matches!(
            estimate_tdee(day(20), &enough, &weigh_ins, 2_000.0, 0.1),
            AdaptiveOutcome::Estimated(_)
        ));

        // 5 pesées seulement (deux le même jour ne comptent que pour une).
        let mut few_weigh_ins: Vec<(NaiveDate, f64)> = weigh_ins.iter().copied().take(5).collect();
        few_weigh_ins.push((day(0), 70.2));
        assert_eq!(
            estimate_tdee(day(20), &days, &few_weigh_ins, 2_000.0, 0.1),
            AdaptiveOutcome::InsufficientData {
                logged_days: 21,
                weigh_in_days: 5
            }
        );
        // Aucune donnée.
        assert_eq!(
            estimate_tdee(day(20), &[], &[], 2_000.0, 0.1),
            AdaptiveOutcome::InsufficientData {
                logged_days: 0,
                weigh_in_days: 0
            }
        );
    }

    #[test]
    fn only_the_window_counts() {
        // 60 jours de données, mais les 21 derniers ne sont pas journalisés.
        let (days, weigh_ins) = simulate(60, 2_000.0, |_| 2_000.0, |_| 0.0);
        let old_days: Vec<DayRecord> = days.iter().copied().take(39).collect();
        assert_eq!(
            estimate_tdee(day(59), &old_days, &weigh_ins, 2_000.0, 0.1),
            AdaptiveOutcome::InsufficientData {
                logged_days: 0,
                weigh_in_days: 21
            }
        );
    }

    #[test]
    fn incomplete_days_are_excluded_from_the_mean() {
        // 16 jours complets à 2 000 kcal, 5 jours non marqués avec 300 kcal saisis.
        let (mut days, weigh_ins) = simulate(21, 2_000.0, |_| 2_000.0, |_| 0.0);
        for d in days.iter_mut().skip(16) {
            d.eaten_kcal = 300.0;
            d.marked_complete = false;
        }
        let e = estimated(estimate_tdee(day(20), &days, &weigh_ins, 2_000.0, 0.1));
        assert_eq!(e.logged_days, 16);
        assert!((e.mean_intake_kcal - 2_000.0).abs() < 1e-9);
        assert!((e.raw_tdee_base - 2_000.0).abs() < 1e-6);
    }

    #[test]
    fn change_is_damped_to_100_kcal() {
        let (days, weigh_ins) = simulate(21, 2_000.0, |_| 2_000.0, |_| 0.0);
        // L'estimation précédente était trop haute de 400 kcal.
        let e = estimated(estimate_tdee(day(20), &days, &weigh_ins, 2_400.0, 0.1));
        assert!((e.raw_tdee_base - 2_000.0).abs() < 1e-6);
        assert!((e.tdee_base - 2_300.0).abs() < 1e-6);
        assert!(e.was_damped());
        // Trop basse de 250 kcal : +100 seulement.
        let e = estimated(estimate_tdee(day(20), &days, &weigh_ins, 1_750.0, 0.1));
        assert!((e.tdee_base - 1_850.0).abs() < 1e-6);
        // Écart de 60 kcal : appliqué en entier.
        let e = estimated(estimate_tdee(day(20), &days, &weigh_ins, 1_940.0, 0.1));
        assert!((e.tdee_base - 2_000.0).abs() < 1e-6);
    }

    #[test]
    fn weekly_recalculation_reaches_the_truth() {
        // Estimation initiale fausse de 400 kcal ; recalcul chaque semaine pendant 6 mois.
        let (days, weigh_ins) = simulate(180, 2_300.0, |_| 1_900.0, |_| 0.0);
        let mut tdee = 1_900.0;
        for week in 3..25 {
            let end = day(week * 7);
            if let AdaptiveOutcome::Estimated(e) = estimate_tdee(end, &days, &weigh_ins, tdee, 0.1)
            {
                assert!((e.tdee_base - tdee).abs() <= MAX_WEEKLY_CHANGE_KCAL + 1e-9);
                tdee = e.tdee_base;
            }
        }
        assert!((tdee - 2_300.0).abs() < 10.0, "{tdee}");
    }

    #[test]
    fn late_first_weigh_in_shortens_the_span() {
        // Les pesées ne commencent qu'au 8e jour de la fenêtre.
        let (days, weigh_ins) = simulate(21, 2_000.0, |_| 2_000.0, |_| 0.0);
        let late: Vec<(NaiveDate, f64)> = weigh_ins.iter().copied().skip(8).collect();
        let e = estimated(estimate_tdee(day(20), &days, &late, 2_000.0, 0.1));
        assert_eq!(e.weigh_in_days, 13);
        assert!((e.raw_tdee_base - 2_000.0).abs() < 1e-6);
    }
}
