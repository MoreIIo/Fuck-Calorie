//! Poids de tendance (§2.3, point 1 ; §2.8).
//!
//! Le poids brut varie d'un jour à l'autre (eau, sel, variations hormonales). Le poids de
//! tendance le lisse par une moyenne mobile exponentielle : chaque jour, la tendance se
//! rapproche du poids du jour d'une fraction `alpha` de l'écart.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Coefficient de lissage par défaut (ajustable dans les réglages).
pub const DEFAULT_ALPHA: f64 = 0.1;

/// Un jour de la série de tendance.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrendPoint {
    pub date: NaiveDate,
    /// Poids pesé ce jour-là (moyenne s'il y a plusieurs pesées), `None` sans pesée.
    pub raw_kg: Option<f64>,
    /// Poids de tendance.
    pub trend_kg: f64,
}

/// Calcule la série de tendance, un point par jour, de la première pesée jusqu'à `until`
/// (inclus).
///
/// - Les jours sans pesée sont **interpolés** linéairement entre la pesée précédente et la
///   suivante ; après la dernière pesée, le dernier poids connu est conservé.
/// - Plusieurs pesées le même jour sont moyennées.
/// - Les pesées postérieures à `until` sont ignorées, ainsi que les valeurs invalides.
///
/// Renvoie une série vide s'il n'y a aucune pesée avant `until`.
#[must_use]
pub fn trend_series(
    weigh_ins: &[(NaiveDate, f64)],
    until: NaiveDate,
    alpha: f64,
) -> Vec<TrendPoint> {
    let alpha = if alpha.is_finite() {
        alpha.clamp(0.01, 1.0)
    } else {
        DEFAULT_ALPHA
    };

    // Moyenne par jour, triée par date.
    let mut by_day: BTreeMap<NaiveDate, (f64, u32)> = BTreeMap::new();
    for &(date, kg) in weigh_ins {
        if date <= until && kg.is_finite() && kg > 0.0 {
            let slot = by_day.entry(date).or_insert((0.0, 0));
            slot.0 += kg;
            slot.1 += 1;
        }
    }
    let known: Vec<(NaiveDate, f64)> = by_day
        .into_iter()
        .map(|(date, (sum, n))| (date, sum / f64::from(n)))
        .collect();
    let Some(&(first_date, first_kg)) = known.first() else {
        return Vec::new();
    };

    let mut series = Vec::new();
    let mut trend = first_kg;
    let mut next = 0; // index de la prochaine pesée à venir dans `known`
    let mut date = first_date;
    while date <= until {
        while next < known.len() && known[next].0 < date {
            next += 1;
        }
        let raw = (next < known.len() && known[next].0 == date).then(|| known[next].1);
        let value = match raw {
            Some(kg) => kg,
            None if next < known.len() => {
                // Interpolation entre la pesée précédente et la suivante.
                let (d0, w0) = known[next - 1];
                let (d1, w1) = known[next];
                let span = (d1 - d0).num_days() as f64;
                let elapsed = (date - d0).num_days() as f64;
                w0 + (w1 - w0) * elapsed / span
            }
            // Après la dernière pesée : on garde le dernier poids connu.
            None => known[known.len() - 1].1,
        };
        if date > first_date {
            trend += alpha * (value - trend);
        }
        series.push(TrendPoint {
            date,
            raw_kg: raw,
            trend_kg: trend,
        });
        let Some(tomorrow) = date.succ_opt() else {
            break;
        };
        date = tomorrow;
    }
    series
}

/// Poids de tendance à la date `on`, si la série couvre ce jour.
#[must_use]
pub fn trend_on(series: &[TrendPoint], on: NaiveDate) -> Option<f64> {
    let first = series.first()?.date;
    let index = usize::try_from((on - first).num_days()).ok()?;
    series.get(index).map(|p| p.trend_kg)
}

/// Variation du poids de tendance sur les `days` derniers jours de la série (ex. 7 pour la
/// mini-carte poids du §5.2). `None` si la série est trop courte.
#[must_use]
pub fn trend_change(series: &[TrendPoint], days: usize) -> Option<f64> {
    let last = series.last()?;
    let earlier = series
        .len()
        .checked_sub(days + 1)
        .and_then(|i| series.get(i))?;
    Some(last.trend_kg - earlier.trend_kg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Days;

    fn day(n: u64) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + Days::new(n)
    }

    #[test]
    fn empty_without_weigh_ins() {
        assert!(trend_series(&[], day(10), DEFAULT_ALPHA).is_empty());
        // Pesée postérieure à la fin demandée.
        assert!(trend_series(&[(day(5), 70.0)], day(3), DEFAULT_ALPHA).is_empty());
        // Valeurs invalides ignorées.
        assert!(trend_series(&[(day(1), f64::NAN), (day(2), -3.0)], day(3), 0.1).is_empty());
    }

    #[test]
    fn ema_follows_spec_formula() {
        let series = trend_series(
            &[(day(0), 70.0), (day(1), 71.0), (day(2), 69.0)],
            day(2),
            0.1,
        );
        assert_eq!(series.len(), 3);
        assert_eq!(series[0].trend_kg, 70.0);
        assert!((series[1].trend_kg - 70.1).abs() < 1e-9); // 70 + 0,1 × (71 − 70)
        assert!((series[2].trend_kg - 69.99).abs() < 1e-9); // 70,1 + 0,1 × (69 − 70,1)
        assert_eq!(series[1].raw_kg, Some(71.0));
    }

    #[test]
    fn missing_days_are_interpolated() {
        // Pesées à J0 (70) et J4 (68) : les jours 1 à 3 valent 69,5 / 69 / 68,5.
        let series = trend_series(&[(day(4), 68.0), (day(0), 70.0)], day(4), 1.0);
        let values: Vec<f64> = series.iter().map(|p| p.trend_kg).collect();
        assert_eq!(values, vec![70.0, 69.5, 69.0, 68.5, 68.0]);
        assert_eq!(series[2].raw_kg, None);
        assert_eq!(series[4].raw_kg, Some(68.0));
    }

    #[test]
    fn last_weight_is_carried_forward() {
        let series = trend_series(&[(day(0), 70.0), (day(1), 69.0)], day(4), 0.5);
        assert_eq!(series.len(), 5);
        assert!(series[2..].iter().all(|p| p.raw_kg.is_none()));
        // La tendance continue de converger vers le dernier poids connu.
        assert!((series[4].trend_kg - 69.0625).abs() < 1e-9);
    }

    #[test]
    fn same_day_weigh_ins_are_averaged() {
        let series = trend_series(&[(day(0), 70.0), (day(0), 71.0)], day(0), 0.1);
        assert_eq!(series[0].raw_kg, Some(70.5));
    }

    #[test]
    fn smoothing_dampens_noise() {
        // Poids stable à 70 kg avec un pic d'eau de +1,5 kg un seul jour.
        let mut weigh_ins: Vec<(NaiveDate, f64)> = (0..14).map(|n| (day(n), 70.0)).collect();
        weigh_ins[7].1 = 71.5;
        let series = trend_series(&weigh_ins, day(13), DEFAULT_ALPHA);
        let max = series.iter().map(|p| p.trend_kg).fold(f64::MIN, f64::max);
        assert!(max < 70.2, "le pic est lissé : {max}");
    }

    #[test]
    fn lookups() {
        let weigh_ins: Vec<(NaiveDate, f64)> = (0..10).map(|n| (day(n), 70.0 - n as f64)).collect();
        let series = trend_series(&weigh_ins, day(9), 1.0);
        assert_eq!(trend_on(&series, day(3)), Some(67.0));
        assert_eq!(trend_on(&series, day(10)), None);
        assert_eq!(trend_on(&series, day(0).pred_opt().unwrap()), None);
        assert_eq!(trend_on(&[], day(0)), None);
        assert_eq!(trend_change(&series, 7), Some(-7.0));
        assert_eq!(trend_change(&series, 10), None);
        assert_eq!(trend_change(&[], 7), None);
    }

    #[test]
    fn invalid_alpha_falls_back() {
        let weigh_ins = [(day(0), 70.0), (day(1), 71.0)];
        let fallback = trend_series(&weigh_ins, day(1), f64::NAN);
        assert!((fallback[1].trend_kg - 70.1).abs() < 1e-9);
    }
}
