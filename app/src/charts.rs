//! Graphiques de l'écran *Progrès* (§5.3), dessinés en SVG par des composants maison :
//! angles droits, un seul accent, aucune librairie JavaScript.
//!
//! Les composants ne font que placer les valeurs reçues de l'application sur le dessin.
//! Survoler (ou toucher) un point affiche sa valeur et sa date ; toucher une barre ouvre le
//! journal du jour. Chaque graphique a une description textuelle pour les lecteurs d'écran
//! (§8.6).

use chrono::{Days, NaiveDate};
use fc_core::api::{ProgressView, WeightPoint};
use fc_core::progress::CalorieBar;
use leptos::prelude::*;

use crate::format::{day_month, int, one_decimal, short_date, signed_one_decimal};

/// Dimensions du dessin (unités du `viewBox`, mis à l'échelle de la carte).
const WIDTH: f64 = 640.0;
const HEIGHT: f64 = 260.0;
/// Marges : à gauche pour les valeurs, en bas pour les dates.
const LEFT: f64 = 48.0;
const RIGHT: f64 = 12.0;
const TOP: f64 = 12.0;
const BOTTOM: f64 = 28.0;

const PLOT_W: f64 = WIDTH - LEFT - RIGHT;
const PLOT_H: f64 = HEIGHT - TOP - BOTTOM;

fn days_between(from: NaiveDate, to: NaiveDate) -> f64 {
    (to - from).num_days() as f64
}

/// Échelle linéaire d'un intervalle de valeurs vers un intervalle du dessin.
#[derive(Clone, Copy)]
struct Scale {
    min: f64,
    max: f64,
    from: f64,
    to: f64,
}

impl Scale {
    fn at(self, value: f64) -> f64 {
        let span = self.max - self.min;
        if span.abs() < f64::EPSILON {
            return (self.from + self.to) / 2.0;
        }
        self.from + (value - self.min) / span * (self.to - self.from)
    }
}

fn path(points: impl IntoIterator<Item = (f64, f64)>) -> String {
    let mut d = String::new();
    for (i, (x, y)) in points.into_iter().enumerate() {
        let command = if i == 0 { 'M' } else { 'L' };
        d.push_str(&format!("{command}{x:.1} {y:.1} "));
    }
    d
}

/// Projection (§2.4) : du dernier poids de tendance jusqu'à l'objectif à la date estimée,
/// coupée à la fin de l'axe si elle va plus loin. Renvoie la fin de l'axe des dates et le
/// segment `(date, poids)` à tracer.
/// Un point du dessin : date et poids.
type Dated = (NaiveDate, f64);

fn projection_segment(
    data: &ProgressView,
    last: WeightPoint,
) -> (NaiveDate, Option<(Dated, Dated)>) {
    let Some(goal_date) = data.projected_goal_date.filter(|d| *d > data.to) else {
        return (data.to, None);
    };
    // L'axe s'allonge au plus de la durée de la période, pour ne pas écraser l'historique.
    let span = (data.to - data.from).num_days().max(7);
    let limit = data
        .to
        .checked_add_days(Days::new(span.unsigned_abs()))
        .unwrap_or(data.to);
    let end = goal_date.min(limit);
    let share = days_between(last.date, end) / days_between(last.date, goal_date).max(1.0);
    let kg = last.kg + (data.goal_weight_kg - last.kg) * share;
    (end, Some(((last.date, last.kg), (end, kg))))
}

/// Graphique du poids : pesées brutes (points), tendance (ligne), objectif (ligne fine) et
/// projection en pointillés jusqu'à la date estimée.
#[component]
pub fn WeightChart(data: ProgressView) -> impl IntoView {
    let Some(last) = data.trend.last().copied() else {
        return view! {
            <p class="muted chart-empty">
                "Pas encore de pesée sur cette période. Pèse-toi depuis l'écran Aujourd'hui."
            </p>
        }
        .into_any();
    };
    let (end, projection) = projection_segment(&data, last);

    let mut low = data.goal_weight_kg;
    let mut high = data.goal_weight_kg;
    for kg in data
        .trend
        .iter()
        .chain(data.weigh_ins.iter())
        .map(|p| p.kg)
        .chain(projection.map(|(_, (_, kg))| kg))
    {
        low = low.min(kg);
        high = high.max(kg);
    }
    // Au moins 2 kg d'amplitude, et un peu d'air au-dessus et au-dessous.
    let middle = (low + high) / 2.0;
    let half = ((high - low) / 2.0).max(1.0) + 0.4;
    let (low, high) = (middle - half, middle + half);

    let x = Scale {
        min: 0.0,
        max: days_between(data.from, end).max(1.0),
        from: LEFT,
        to: WIDTH - RIGHT,
    };
    let y = Scale {
        min: low,
        max: high,
        from: TOP + PLOT_H,
        to: TOP,
    };
    let px = move |date: NaiveDate| x.at(days_between(data.from, date));

    let trend_path = path(data.trend.iter().map(|p| (px(p.date), y.at(p.kg))));
    let goal_y = y.at(data.goal_weight_kg);
    let projection_path =
        projection.map(|(a, b)| path([(px(a.0), y.at(a.1)), (px(b.0), y.at(b.1))]));
    let ticks = [high - 0.4, middle, low + 0.4];

    // Description pour les lecteurs d'écran, et texte affiché sans survol.
    let mut summary = format!(
        "Poids de tendance : {} kg le {}",
        one_decimal(last.kg),
        day_month(last.date)
    );
    if let Some(change) = data.trend_change_kg {
        summary.push_str(&format!(
            ", {} kg sur la période",
            signed_one_decimal(change)
        ));
    }
    summary.push_str(&format!(
        ". Objectif : {} kg",
        one_decimal(data.goal_weight_kg)
    ));
    if let Some(date) = data.projected_goal_date {
        summary.push_str(&format!(", atteint vers le {}", short_date(date)));
    }
    summary.push('.');
    let readout = RwSignal::new(None::<String>);

    // Zones de survol : une colonne par jour de la tendance.
    let column = PLOT_W / x.max;
    let hover = data
        .trend
        .iter()
        .map(|p| {
            let raw = data
                .weigh_ins
                .iter()
                .find(|w| w.date == p.date)
                .map(|w| w.kg);
            let mut text = format!("{} : tendance {} kg", day_month(p.date), one_decimal(p.kg));
            if let Some(kg) = raw {
                text.push_str(&format!(", pesée {} kg", one_decimal(kg)));
            }
            let left = px(p.date) - column / 2.0;
            view! {
                <rect
                    class="hover-zone"
                    x=format!("{left:.1}")
                    y=TOP
                    width=format!("{:.2}", column.max(1.0))
                    height=PLOT_H
                    on:mouseenter=move |_| readout.set(Some(text.clone()))
                />
            }
        })
        .collect_view();

    let summary_text = summary.clone();
    view! {
        <p class="chart-readout" aria-live="polite">
            {move || readout.get().unwrap_or_else(|| summary_text.clone())}
        </p>
        <svg
            class="chart"
            viewBox=format!("0 0 {WIDTH} {HEIGHT}")
            role="img"
            aria-label=summary
            on:mouseleave=move |_| readout.set(None)
        >
            {ticks
                .iter()
                .map(|kg| {
                    let ty = y.at(*kg);
                    view! {
                        <line class="grid-line" x1=LEFT x2=WIDTH - RIGHT y1=ty y2=ty />
                        <text class="tick" x=LEFT - 8.0 y=ty + 4.0 text-anchor="end">
                            {one_decimal(*kg)}
                        </text>
                    }
                })
                .collect_view()}
            <line class="goal-line" x1=LEFT x2=WIDTH - RIGHT y1=goal_y y2=goal_y />
            <text class="tick goal" x=WIDTH - RIGHT y=goal_y - 6.0 text-anchor="end">
                {format!("objectif {}", one_decimal(data.goal_weight_kg))}
            </text>
            {data
                .weigh_ins
                .iter()
                .map(|p| {
                    view! {
                        <rect
                            class="raw-point"
                            x=format!("{:.1}", px(p.date) - 2.5)
                            y=format!("{:.1}", y.at(p.kg) - 2.5)
                            width="5"
                            height="5"
                        />
                    }
                })
                .collect_view()}
            <path class="trend-line" d=trend_path />
            {projection_path.map(|d| view! { <path class="projection-line" d=d /> })}
            <text class="tick" x=LEFT y=HEIGHT - 8.0>
                {day_month(data.from)}
            </text>
            <text class="tick" x=WIDTH - RIGHT y=HEIGHT - 8.0 text-anchor="end">
                {day_month(end)}
            </text>
            {hover}
        </svg>
    }
    .into_any()
}

/// Graphique « Calories vs cible » : une barre par jour (par semaine au-delà de 90 jours),
/// la cible de chaque barre et la moyenne de la période.
#[component]
pub fn CaloriesChart(
    data: ProgressView,
    /// Ouvre le journal d'un jour (premier jour de la semaine pour une barre hebdomadaire).
    on_open: Callback<NaiveDate>,
) -> impl IntoView {
    let bars = data.calories.clone();
    if bars.iter().all(|b| b.eaten_kcal.is_none()) {
        return view! {
            <p class="muted chart-empty">"Rien de journalisé sur cette période."</p>
        }
        .into_any();
    }
    let top = bars
        .iter()
        .flat_map(|b| [b.eaten_kcal.unwrap_or(0.0), b.target_kcal])
        .fold(0.0_f64, f64::max)
        * 1.1;
    let y = Scale {
        min: 0.0,
        max: top.max(1.0),
        from: TOP + PLOT_H,
        to: TOP,
    };
    let slot = PLOT_W / bars.len().max(1) as f64;
    let gap = (slot * 0.25).min(4.0);
    let unit = if data.weekly {
        "par jour en moyenne"
    } else {
        ""
    };

    let over_count = bars
        .iter()
        .filter(|b| b.eaten_kcal.is_some_and(|e| e > b.target_kcal) && !b.partial)
        .count();
    let mut summary = match (data.mean_eaten_kcal, data.mean_target_kcal) {
        (Some(eaten), Some(target)) => format!(
            "Moyenne : {} kcal par jour pour une cible de {}, sur {} jours journalisés sur {}.",
            int(eaten),
            int(target),
            data.logged_days,
            data.days
        ),
        _ => format!(
            "Aucun jour complet sur cette période ({} jours).",
            data.days
        ),
    };
    if over_count > 0 {
        let what = if data.weekly { "semaine" } else { "jour" };
        let plural = if over_count > 1 { "s" } else { "" };
        summary.push_str(&format!(
            " {over_count} {what}{plural} au-dessus de la cible."
        ));
    }
    let readout = RwSignal::new(None::<String>);

    let describe = move |bar: &CalorieBar| {
        let when = if bar.from == bar.to {
            day_month(bar.from)
        } else {
            format!("du {} au {}", day_month(bar.from), day_month(bar.to))
        };
        match bar.eaten_kcal {
            None => format!(
                "{when} : rien de journalisé, cible {} kcal",
                int(bar.target_kcal)
            ),
            Some(eaten) => {
                let position = if bar.partial {
                    " (journée incomplète)"
                } else if eaten > bar.target_kcal {
                    " — au-dessus de la cible"
                } else {
                    ""
                };
                format!(
                    "{when} : {} kcal {unit} pour une cible de {}{position}",
                    int(eaten),
                    int(bar.target_kcal)
                )
                .replace("  ", " ")
            }
        }
    };

    let target_path = {
        let mut d = String::new();
        for (i, bar) in bars.iter().enumerate() {
            let left = LEFT + slot * i as f64;
            let ty = y.at(bar.target_kcal);
            d.push_str(&format!("M{left:.1} {ty:.1} L{:.1} {ty:.1} ", left + slot));
        }
        d
    };
    let mean = data.mean_eaten_kcal.map(|m| y.at(m));

    let drawn = bars
        .iter()
        .enumerate()
        .map(|(i, bar)| {
            let left = LEFT + slot * i as f64 + gap / 2.0;
            let width = (slot - gap).max(1.0);
            let text = describe(bar);
            let focus_text = text.clone();
            let day = bar.from;
            let eaten = bar.eaten_kcal.unwrap_or(0.0);
            let class = if bar.partial {
                "bar partial"
            } else if eaten > bar.target_kcal {
                "bar over"
            } else {
                "bar"
            };
            let bar_top = y.at(eaten);
            view! {
                <g
                    class="bar-group"
                    role="button"
                    tabindex="0"
                    aria-label=text.clone()
                    on:mouseenter=move |_| readout.set(Some(text.clone()))
                    on:focus=move |_| readout.set(Some(focus_text.clone()))
                    on:click=move |_| on_open.run(day)
                    on:keydown=move |ev| {
                        if ev.key() == "Enter" || ev.key() == " " {
                            ev.prevent_default();
                            on_open.run(day);
                        }
                    }
                >
                    <rect class="hover-zone" x=left y=TOP width=width height=PLOT_H />
                    {(eaten > 0.0)
                        .then(|| {
                            view! {
                                <rect
                                    class=class
                                    x=format!("{left:.1}")
                                    y=format!("{bar_top:.1}")
                                    width=format!("{width:.2}")
                                    height=format!("{:.1}", TOP + PLOT_H - bar_top)
                                />
                            }
                        })}
                </g>
            }
        })
        .collect_view();

    let first = bars.first().map(|b| b.from);
    let last = bars.last().map(|b| b.to);
    let summary_text = summary.clone();
    view! {
        <p class="chart-readout" aria-live="polite">
            {move || readout.get().unwrap_or_else(|| summary_text.clone())}
        </p>
        <svg
            class="chart"
            viewBox=format!("0 0 {WIDTH} {HEIGHT}")
            role="group"
            aria-label=summary
            on:mouseleave=move |_| readout.set(None)
        >
            {[0.5, 1.0]
                .iter()
                .map(|share| {
                    let value = top / 1.1 * share;
                    let ty = y.at(value);
                    view! {
                        <line class="grid-line" x1=LEFT x2=WIDTH - RIGHT y1=ty y2=ty />
                        <text class="tick" x=LEFT - 8.0 y=ty + 4.0 text-anchor="end">
                            {int(value)}
                        </text>
                    }
                })
                .collect_view()}
            <line class="axis" x1=LEFT x2=WIDTH - RIGHT y1=TOP + PLOT_H y2=TOP + PLOT_H />
            {drawn}
            <path class="target-line" d=target_path />
            {mean
                .map(|my| {
                    view! {
                        <line class="mean-line" x1=LEFT x2=WIDTH - RIGHT y1=my y2=my />
                        <text class="tick" x=WIDTH - RIGHT y=my - 6.0 text-anchor="end">
                            "moyenne"
                        </text>
                    }
                })}
            {first
                .map(|d| {
                    view! {
                        <text class="tick" x=LEFT y=HEIGHT - 8.0>
                            {day_month(d)}
                        </text>
                    }
                })}
            {last
                .map(|d| {
                    view! {
                        <text class="tick" x=WIDTH - RIGHT y=HEIGHT - 8.0 text-anchor="end">
                            {day_month(d)}
                        </text>
                    }
                })}
        </svg>
    }
    .into_any()
}
