//! Écran *Progrès* (§5.3) : périodes affichées et regroupement des journées pour le
//! graphique « Calories vs cible ».

use chrono::{Days, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

use crate::adaptive::{DayRecord, week_start};

/// Au-delà de cette durée (en jours), les calories sont regroupées par semaine (§5.3).
pub const WEEKLY_ABOVE_DAYS: i64 = 90;

/// Période du sélecteur commun aux graphiques (§5.3) : 7 j · 30 j · 90 j · 1 an · Tout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressPeriod {
    Week,
    Month,
    Quarter,
    Year,
    All,
}

impl ProgressPeriod {
    pub const ALL: [Self; 5] = [
        Self::Week,
        Self::Month,
        Self::Quarter,
        Self::Year,
        Self::All,
    ];

    /// Nombre de jours affichés, `None` pour « Tout ».
    #[must_use]
    pub fn days(self) -> Option<u64> {
        match self {
            Self::Week => Some(7),
            Self::Month => Some(30),
            Self::Quarter => Some(90),
            Self::Year => Some(365),
            Self::All => None,
        }
    }

    /// Premier jour affiché, jusqu'à `today` inclus. Pour « Tout », le premier jour
    /// d'utilisation `first_use` ; au moins 7 jours sont toujours affichés.
    #[must_use]
    pub fn start(self, today: NaiveDate, first_use: NaiveDate) -> NaiveDate {
        let days = self.days().unwrap_or(1);
        let start = today.checked_sub_days(Days::new(days - 1)).unwrap_or(today);
        let start = if self == Self::All {
            first_use.min(today)
        } else {
            start
        };
        let week = today.checked_sub_days(Days::new(6)).unwrap_or(today);
        start.min(week)
    }
}

/// Les calories sont-elles regroupées par semaine pour cette plage ? (au-delà de 90 jours)
#[must_use]
pub fn weekly_buckets(from: NaiveDate, to: NaiveDate) -> bool {
    (to - from).num_days() + 1 > WEEKLY_ABOVE_DAYS
}

/// Une barre du graphique « Calories vs cible » : un jour, ou une semaine.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CalorieBar {
    /// Premier et dernier jour couverts (le même jour pour une barre quotidienne).
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Calories mangées : le total du jour, ou la moyenne des jours journalisés de la
    /// semaine. `None` : rien de journalisé.
    pub eaten_kcal: Option<f64>,
    /// Cible du jour (activités comprises), ou cible moyenne des jours journalisés de la
    /// semaine (de tous ses jours si aucun ne l'est).
    pub target_kcal: f64,
    /// Jours comptés comme journalisés (§2.3 : marqués terminés, ou au moins 50 % de la
    /// cible).
    pub logged_days: u32,
    /// Journée commencée mais manifestement incomplète (barre quotidienne seulement).
    pub partial: bool,
}

/// Moyennes d'une période : calories mangées et cible des jours journalisés, et leur nombre.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PeriodMeans {
    pub eaten_kcal: Option<f64>,
    pub target_kcal: Option<f64>,
    pub logged_days: u32,
}

/// Moyennes des jours qui comptent comme journalisés (même règle que le bilan de la
/// semaine, §5.4) : une journée à peine commencée ne fait pas baisser la moyenne.
#[must_use]
pub fn period_means(days: &[DayRecord]) -> PeriodMeans {
    let logged: Vec<&DayRecord> = days.iter().filter(|d| d.counts_as_logged()).collect();
    let count = logged.len() as f64;
    let mean = |value: fn(&DayRecord) -> f64| {
        (!logged.is_empty()).then(|| logged.iter().map(|d| value(d)).sum::<f64>() / count)
    };
    PeriodMeans {
        eaten_kcal: mean(|d| d.eaten_kcal),
        target_kcal: mean(|d| d.target_kcal),
        logged_days: u32::try_from(logged.len()).unwrap_or(u32::MAX),
    }
}

/// Barres du graphique « Calories vs cible » (§5.3), dans l'ordre chronologique.
///
/// `days` contient chaque jour de la période, dans l'ordre. Par jour : le total mangé (une
/// journée vide n'a pas de barre). Par semaine (`weekly`, semaines commençant le jour
/// `first_day`, §2.1) : la moyenne des jours journalisés, pour qu'une semaine partielle reste
/// comparable à la cible.
#[must_use]
pub fn calorie_bars(days: &[DayRecord], weekly: bool, first_day: Weekday) -> Vec<CalorieBar> {
    if !weekly {
        return days
            .iter()
            .map(|d| {
                let logged = d.counts_as_logged();
                let started = d.eaten_kcal > 0.0 || d.marked_complete;
                CalorieBar {
                    from: d.date,
                    to: d.date,
                    eaten_kcal: started.then_some(d.eaten_kcal),
                    target_kcal: d.target_kcal,
                    logged_days: u32::from(logged),
                    partial: started && !logged,
                }
            })
            .collect();
    }

    let mut bars = Vec::new();
    let mut index = 0;
    while let Some(first) = days.get(index) {
        let week = week_start(first.date, first_day);
        let end = week.checked_add_days(Days::new(6)).unwrap_or(week);
        let count = days[index..]
            .iter()
            .take_while(|d| d.date <= end)
            .count()
            .max(1);
        let chunk = &days[index..index + count];
        let means = period_means(chunk);
        let all_targets = chunk.iter().map(|d| d.target_kcal).sum::<f64>() / chunk.len() as f64;
        bars.push(CalorieBar {
            from: first.date,
            to: chunk.last().map_or(first.date, |d| d.date),
            eaten_kcal: means.eaten_kcal,
            target_kcal: means.target_kcal.unwrap_or(all_targets),
            logged_days: means.logged_days,
            partial: false,
        });
        index += count;
    }
    bars
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    fn record(d: u32, eaten: f64, target: f64, complete: bool) -> DayRecord {
        DayRecord {
            date: day(d),
            eaten_kcal: eaten,
            activity_kcal: 0.0,
            target_kcal: target,
            marked_complete: complete,
        }
    }

    #[test]
    fn period_start() {
        let today = day(17);
        let first_use = day(15);
        assert_eq!(ProgressPeriod::Week.start(today, first_use), day(11));
        assert_eq!(
            ProgressPeriod::Month.start(today, first_use),
            day(17) - Days::new(29)
        );
        assert_eq!(
            ProgressPeriod::Year.start(today, first_use),
            NaiveDate::from_ymd_opt(2025, 11, 18).unwrap()
        );
        // « Tout » : depuis le premier jour, mais au moins une semaine.
        assert_eq!(ProgressPeriod::All.start(today, first_use), day(11));
        assert_eq!(ProgressPeriod::All.start(today, day(1)), day(1));
        // Premier jour dans le futur (horloge déréglée) : on s'arrête à aujourd'hui.
        assert_eq!(ProgressPeriod::All.start(today, day(20)), day(11));
    }

    #[test]
    fn weekly_above_ninety_days() {
        let today = day(17);
        for period in ProgressPeriod::ALL {
            let from = period.start(today, day(1));
            let expected = matches!(period, ProgressPeriod::Year);
            assert_eq!(weekly_buckets(from, today), expected, "{period:?}");
        }
        assert!(!weekly_buckets(day(1), day(30)));
        let from = NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();
        assert!(weekly_buckets(from, day(17)));
    }

    #[test]
    fn means_skip_unfinished_days() {
        let days = [
            record(1, 1_800.0, 2_000.0, false), // compte (≥ 50 %)
            record(2, 500.0, 2_000.0, false),   // incomplet : ignoré
            record(3, 0.0, 2_000.0, false),     // vide
            record(4, 900.0, 2_200.0, true),    // marqué terminé : compte
        ];
        let means = period_means(&days);
        assert_eq!(means.logged_days, 2);
        assert_eq!(means.eaten_kcal, Some(1_350.0));
        assert_eq!(means.target_kcal, Some(2_100.0));
        assert_eq!(
            period_means(&days[1..3]),
            PeriodMeans {
                eaten_kcal: None,
                target_kcal: None,
                logged_days: 0
            }
        );
    }

    #[test]
    fn daily_bars() {
        let days = [
            record(1, 1_800.0, 2_000.0, false),
            record(2, 500.0, 2_000.0, false),
            record(3, 0.0, 2_100.0, false),
        ];
        let bars = calorie_bars(&days, false, Weekday::Mon);
        assert_eq!(bars.len(), 3);
        assert_eq!(bars[0].eaten_kcal, Some(1_800.0));
        assert!(!bars[0].partial);
        assert_eq!(bars[1].eaten_kcal, Some(500.0));
        assert!(bars[1].partial);
        assert_eq!(bars[2].eaten_kcal, None);
        assert_eq!(bars[2].target_kcal, 2_100.0);
        assert_eq!((bars[0].from, bars[0].to), (day(1), day(1)));
    }

    #[test]
    fn weekly_bars_follow_week_start() {
        // Novembre 2026 : le 2 est un lundi. Du jeudi 5 au mardi 17.
        let days: Vec<DayRecord> = (5..=17)
            .map(|d| {
                let eaten = if d == 10 {
                    0.0
                } else {
                    1_000.0 + f64::from(d) * 100.0
                };
                record(d, eaten, 2_000.0 + f64::from(d), false)
            })
            .collect();
        let bars = calorie_bars(&days, true, Weekday::Mon);
        let ranges: Vec<_> = bars.iter().map(|b| (b.from, b.to)).collect();
        assert_eq!(
            ranges,
            vec![(day(5), day(8)), (day(9), day(15)), (day(16), day(17))]
        );
        // Semaine du 9 au 15, sans le 10 (vide) : 6 jours journalisés.
        let week = bars[1];
        assert_eq!(week.logged_days, 6);
        let expected = [9, 11, 12, 13, 14, 15]
            .iter()
            .map(|d| 1_000.0 + f64::from(*d) * 100.0)
            .sum::<f64>()
            / 6.0;
        assert!((week.eaten_kcal.unwrap() - expected).abs() < 1e-9);
        let target = [9, 11, 12, 13, 14, 15]
            .iter()
            .map(|d| 2_000.0 + f64::from(*d))
            .sum::<f64>()
            / 6.0;
        assert!((week.target_kcal - target).abs() < 1e-9);

        // Semaines commençant le dimanche : coupure entre le 7 et le 8.
        let bars = calorie_bars(&days, true, Weekday::Sun);
        assert_eq!((bars[0].from, bars[0].to), (day(5), day(7)));
        assert_eq!((bars[1].from, bars[1].to), (day(8), day(14)));
    }

    #[test]
    fn empty_week_uses_mean_target() {
        let days = [
            record(16, 0.0, 2_000.0, false),
            record(17, 0.0, 2_200.0, false),
        ];
        let bars = calorie_bars(&days, true, Weekday::Mon);
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].eaten_kcal, None);
        assert_eq!(bars[0].target_kcal, 2_100.0);
        assert_eq!(bars[0].logged_days, 0);
        assert!(calorie_bars(&[], true, Weekday::Mon).is_empty());
    }
}
