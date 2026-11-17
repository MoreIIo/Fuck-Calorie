//! Écran *Progrès* (§5.3) : sélecteur de période, graphiques du poids et des calories, bilan
//! de la semaine (§5.4).

use fc_core::api::{ProgressView, WeekReport};
use fc_core::progress::ProgressPeriod;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::Shell;
use crate::api;
use crate::charts::{CaloriesChart, WeightChart};
use crate::today::WeekCard;

const PERIODS: [(ProgressPeriod, &str); 5] = [
    (ProgressPeriod::Week, "7 j"),
    (ProgressPeriod::Month, "30 j"),
    (ProgressPeriod::Quarter, "90 j"),
    (ProgressPeriod::Year, "1 an"),
    (ProgressPeriod::All, "Tout"),
];

#[component]
pub fn Progress() -> impl IntoView {
    let shell = expect_context::<Shell>();
    let period = RwSignal::new(ProgressPeriod::Month);
    let data = RwSignal::new(None::<Result<ProgressView, String>>);
    let report = RwSignal::new(None::<WeekReport>);

    Effect::new(move |_| {
        let chosen = period.get();
        shell.reload.track();
        spawn_local(async move {
            let _ = data.try_set(Some(api::progress(chosen, shell.today).await));
        });
    });
    Effect::new(move |_| {
        shell.reload.track();
        spawn_local(async move {
            if let Ok(next) = api::week_report(shell.today).await {
                let _ = report.try_set(next);
            }
        });
    });
    let open = Callback::new(move |day| shell.open_day(day));

    view! {
        <header class="page-head">
            <h1 class="h1">"Progrès"</h1>
            <div class="choices" role="group" aria-label="Période">
                {PERIODS
                    .iter()
                    .map(|(value, label)| {
                        let value = *value;
                        view! {
                            <button
                                class="choice"
                                class:on=move || period.get() == value
                                aria-pressed=move || (period.get() == value).to_string()
                                on:click=move |_| period.set(value)
                            >
                                {*label}
                            </button>
                        }
                    })
                    .collect_view()}
            </div>
        </header>
        {move || match data.get() {
            None => view! { <p class="muted">"Chargement…"</p> }.into_any(),
            Some(Err(message)) => view! { <p class="error">{message}</p> }.into_any(),
            Some(Ok(view)) => {
                let calories_title = if view.weekly {
                    "Calories vs cible, par semaine"
                } else {
                    "Calories vs cible"
                };
                view! {
                    <div class="progress-grid">
                        <section class="glass card chart-card" aria-labelledby="weight-title">
                            <p class="label" id="weight-title">
                                "Poids"
                            </p>
                            <WeightChart data=view.clone() />
                        </section>
                        <section class="glass card chart-card" aria-labelledby="calories-title">
                            <p class="label" id="calories-title">
                                {calories_title}
                            </p>
                            <CaloriesChart data=view on_open=open />
                        </section>
                    </div>
                }
                    .into_any()
            }
        }}
        {move || {
            report
                .get()
                .map(|r| {
                    view! {
                        <div class="progress-week">
                            <WeekCard report=r reload=shell.reload />
                        </div>
                    }
                })
        }}
    }
}
