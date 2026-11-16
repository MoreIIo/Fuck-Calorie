//! Écran *Aujourd'hui* (§5.2).

use chrono::{Days, NaiveDate};
use fc_core::api::{MealView, TodayView, WeekReport, WeightView};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::add::{AddSheet, SheetRequest};
use crate::api;
use crate::format::{
    day_month, day_title, int, long_date, one_decimal, parse_number, short_date, signed_int,
    signed_one_decimal,
};
use crate::icons::{IconClose, IconLeft, IconPlus, IconRight};

#[component]
pub fn Today(
    data: TodayView,
    /// Bilan de la semaine écoulée, s'il y en a un (§5.4).
    report: Option<WeekReport>,
    today: NaiveDate,
    date: RwSignal<NaiveDate>,
    reload: RwSignal<u32>,
) -> impl IntoView {
    // Feuille d'ajout : `Some(repas pré-sélectionné)` quand elle est ouverte.
    let sheet = RwSignal::new(None::<SheetRequest>);
    let suggested = data
        .suggested_meal_id
        .or_else(|| data.meals.first().map(|m| m.id))
        .map(SheetRequest::Add);
    // Journée encore vide : proposer de recopier celle de la veille (§3.6).
    let day_empty = data.meals.iter().all(|m| m.entries.is_empty());
    let copy_error = RwSignal::new(None::<String>);
    let meals_for_sheet = data.meals.clone();

    let shift = move |days: i64| {
        date.update(|d| {
            let next = if days < 0 {
                d.checked_sub_days(Days::new(days.unsigned_abs()))
            } else {
                d.checked_add_days(Days::new(days.unsigned_abs()))
            };
            if let Some(next) = next {
                *d = next;
            }
        });
    };

    // Raccourcis clavier (§8.6) : ←/→ changer de jour, N ajouter, / rechercher (la feuille
    // d'ajout s'ouvre sur la recherche).
    let handle = window_event_listener(leptos::ev::keydown, move |ev| {
        if sheet.get_untracked().is_some() || typing_in_field() {
            return;
        }
        match ev.key().as_str() {
            "ArrowLeft" => shift(-1),
            "ArrowRight" => shift(1),
            "n" | "N" | "/" => {
                // Sans cela, la touche s'écrirait dans le champ de recherche qui s'ouvre.
                ev.prevent_default();
                sheet.set(suggested);
            }
            _ => {}
        }
    });
    on_cleanup(move || handle.remove());

    let current = data.date;
    let title = day_title(current, today);
    let subtitle = (current == today || (current - today).num_days().abs() == 1)
        .then(|| long_date(current, today));
    let complete = data.complete;
    // Le jour du recalcul, le bilan est en tête de l'accueil ; les autres jours, sous la
    // carte poids (§5.4).
    let report_first = report.as_ref().is_some_and(|r| r.computed_for == today);
    let (report_top, report_side) = if report_first {
        (report, None)
    } else {
        (None, report)
    };
    let remaining_kcal = data.remaining_kcal;

    view! {
        <header class="topbar">
            <button class="icon-btn" aria-label="Jour précédent" on:click=move |_| shift(-1)>
                <IconLeft />
            </button>
            <div class="date">
                <h1 class="h1">{title}</h1>
                {subtitle.map(|s| view! { <p class="label">{s}</p> })}
            </div>
            <button class="icon-btn" aria-label="Jour suivant" on:click=move |_| shift(1)>
                <IconRight />
            </button>
        </header>

        <div class="grid">
            <div class="stack">
                {report_top.map(|r| view! { <WeekCard report=r reload=reload /> })}
                <Hero data=data.clone() />
            </div>
            <div class="stack">
                {data
                    .meals
                    .iter()
                    .cloned()
                    .map(|meal| {
                        view! { <MealCard meal=meal date=current sheet=sheet reload=reload /> }
                    })
                    .collect_view()}
            </div>
            <div class="stack">
                <WeightCard
                    weight=data.weight.clone()
                    date=current
                    goal_weight=data.goal_weight_kg
                    projection=data.projected_goal_date
                    reload=reload
                />
                {report_side.map(|r| view! { <WeekCard report=r reload=reload /> })}
            </div>
            <div class="done">
                {day_empty
                    .then(|| {
                        view! {
                            <button
                                class="btn block"
                                on:click=move |_| {
                                    let Some(yesterday) = current.checked_sub_days(Days::new(1))
                                    else {
                                        return;
                                    };
                                    spawn_local(async move {
                                        match api::copy_entries(yesterday, None, current).await {
                                            Ok(_) => reload.update(|n| *n += 1),
                                            Err(message) => {
                                                let _ = copy_error.try_set(Some(message));
                                            }
                                        }
                                    });
                                }
                            >
                                "Copier toute la journée d'hier"
                            </button>
                        }
                    })}
                {move || copy_error.get().map(|m| view! { <p class="error">{m}</p> })}
                <button
                    class=if complete { "btn block" } else { "btn primary block" }
                    on:click=move |_| {
                        spawn_local(async move {
                            if api::set_day_complete(current, !complete).await.is_ok() {
                                reload.update(|n| *n += 1);
                            }
                        });
                    }
                >
                    {if complete { "Journée terminée ✓ — rouvrir" } else { "Journée terminée" }}
                </button>
            </div>
        </div>

        <button class="fab" aria-label="Ajouter (N)" on:click=move |_| sheet.set(suggested)>
            <IconPlus />
        </button>

        {move || {
            sheet
                .get()
                .map(|request| {
                    view! {
                        <AddSheet
                            meals=meals_for_sheet.clone()
                            request=request
                            date=current
                            remaining_kcal=remaining_kcal
                            sheet=sheet
                            reload=reload
                        />
                    }
                })
        }}
    }
}

/// Une saisie est-elle en cours dans un champ ? (les raccourcis clavier sont alors ignorés)
fn typing_in_field() -> bool {
    document()
        .active_element()
        .map(|el| matches!(el.tag_name().as_str(), "INPUT" | "SELECT" | "TEXTAREA"))
        .unwrap_or(false)
}

/// Carte héro : kcal restantes, équation du budget, jauges des macros.
#[component]
fn Hero(data: TodayView) -> impl IntoView {
    let over = data.remaining_kcal < 0.0;
    let gauge_style = format!("transform: scaleX({:.4})", data.progress);
    let equation = format!(
        "cible {} − mangé {} + activité {} = {}",
        int(data.base_target_kcal),
        int(data.eaten.kcal),
        int(data.activity_kcal),
        int(data.remaining_kcal)
    );
    let mut notes = Vec::new();
    if data.floor_applied {
        notes.push(format!(
            "Cible relevée au plancher de sécurité ({} kcal).",
            int(data.base_target_kcal)
        ));
    }
    if data.deficit_capped {
        notes.push("Déficit limité à 1 % de ton poids par semaine.".to_owned());
    }
    notes.push(if data.tdee_adaptive {
        format!("Dépense estimée d'après ton suivi : {} kcal/jour.", int(data.tdee_kcal))
    } else {
        format!(
            "Dépense de départ estimée : {} kcal/jour. Elle sera affinée après deux semaines de suivi.",
            int(data.tdee_kcal)
        )
    });

    let macros = [
        (
            "Protéines",
            data.eaten.protein_g,
            data.macros.protein_g,
            "gauge",
        ),
        (
            "Glucides",
            data.eaten.carbs_g,
            data.macros.carbs_g,
            "gauge t2",
        ),
        ("Lipides", data.eaten.fat_g, data.macros.fat_g, "gauge t3"),
    ];

    view! {
        <section class="glass hero" aria-label="Budget du jour">
            <p class="label">{if over { "Dépassement" } else { "Kcal restantes" }}</p>
            <p class="display" class:accent=!over class:over=over>
                {int(data.remaining_kcal.abs())}
                <span class="unit">"kcal"</span>
            </p>
            <p class="equation muted">{equation}</p>
            <div
                class="gauge thick"
                class:over=over
                role="progressbar"
                aria-valuemin="0"
                aria-valuemax="100"
                aria-valuenow=format!("{:.0}", data.progress * 100.0)
                aria-label=format!(
                    "{} kcal mangées sur {}",
                    int(data.eaten.kcal),
                    int(data.target_kcal),
                )
            >
                <span style=gauge_style></span>
            </div>
            {macros
                .into_iter()
                .map(|(name, eaten, target, class)| {
                    let ratio = if target > 0.0 { (eaten / target).clamp(0.0, 1.0) } else { 0.0 };
                    let text = format!("{} / {} g", int(eaten), int(target));
                    view! {
                        <div class="macro">
                            <div class="line">
                                <span class="label">{name}</span>
                                <span>{text.clone()}</span>
                            </div>
                            <div class=class role="img" aria-label=format!("{name} : {text}")>
                                <span style=format!("transform: scaleX({ratio:.4})")></span>
                            </div>
                        </div>
                    }
                })
                .collect_view()}
            <div class="notes muted">
                {notes.into_iter().map(|n| view! { <p style="margin:0">{n}</p> }).collect_view()}
            </div>
        </section>
    }
}

/// Carte d'un repas : total, lignes (un clic ouvre la ligne pour la modifier), ajout, copie
/// du même repas de la veille (§3.6) et enregistrement du repas (§3.6).
#[component]
fn MealCard(
    meal: MealView,
    date: NaiveDate,
    sheet: RwSignal<Option<SheetRequest>>,
    reload: RwSignal<u32>,
) -> impl IntoView {
    let meal_id = meal.id;
    let empty = meal.entries.is_empty();
    let error = RwSignal::new(None::<String>);
    // Nom du repas à enregistrer : `Some` quand le petit formulaire est ouvert.
    let saving = RwSignal::new(None::<String>);
    let default_name = meal.name.clone();

    let copy_yesterday = move |_| {
        let Some(yesterday) = date.checked_sub_days(Days::new(1)) else {
            return;
        };
        spawn_local(async move {
            match api::copy_entries(yesterday, Some(meal_id), date).await {
                Ok(_) => reload.update(|n| *n += 1),
                Err(message) => {
                    let _ = error.try_set(Some(message));
                }
            }
        });
    };
    let save = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let name = saving.get_untracked().unwrap_or_default();
        spawn_local(async move {
            match api::save_meal(date, meal_id, name).await {
                Ok(()) => {
                    let _ = saving.try_set(None);
                    let _ = error.try_set(None);
                }
                Err(message) => {
                    let _ = error.try_set(Some(message));
                }
            }
        });
    };

    view! {
        <section class="glass card">
            <div class="card-head">
                <h2 class="h2">{meal.name.clone()}</h2>
                <span class="muted">{format!("{} kcal", int(meal.kcal))}</span>
            </div>
            {empty.then(|| view! { <p class="empty muted">"Rien pour l'instant."</p> })}
            {meal
                .entries
                .into_iter()
                .map(|entry| {
                    let id = entry.id;
                    let label = format!("Modifier {}", entry.label);
                    view! {
                        <div class="entry">
                            <button
                                class="name entry-open"
                                aria-label=label
                                on:click=move |_| sheet.set(Some(SheetRequest::Edit(id, meal_id)))
                            >
                                {entry.label}
                            </button>
                            <span class="kcal muted">{format!("{} kcal", int(entry.kcal))}</span>
                            <button
                                class="icon-btn"
                                aria-label="Supprimer"
                                on:click=move |_| {
                                    spawn_local(async move {
                                        if api::delete_entry(id).await.is_ok() {
                                            reload.update(|n| *n += 1);
                                        }
                                    });
                                }
                            >
                                <IconClose />
                            </button>
                        </div>
                    }
                })
                .collect_view()}
            <div class="card-actions">
                <button class="btn text" on:click=move |_| sheet.set(Some(SheetRequest::Add(meal_id)))>
                    "+ Ajouter"
                </button>
                {if empty {
                    view! {
                        <button class="btn text" on:click=copy_yesterday>
                            "Copier d'hier"
                        </button>
                    }
                        .into_any()
                } else {
                    view! {
                        <button
                            class="btn text"
                            on:click=move |_| saving.set(Some(default_name.clone()))
                        >
                            "Enregistrer ce repas"
                        </button>
                    }
                        .into_any()
                }}
            </div>
            {move || {
                saving
                    .with(Option::is_some)
                    .then(|| {
                        view! {
                            <form class="inline-form" on:submit=save>
                                <input
                                    class="input"
                                    aria-label="Nom du repas enregistré"
                                    placeholder="Mon petit-déj"
                                    prop:value=move || saving.get().unwrap_or_default()
                                    on:input=move |ev| saving.set(Some(event_target_value(&ev)))
                                />
                                <button class="btn" type="submit">"OK"</button>
                                <button
                                    class="icon-btn"
                                    type="button"
                                    aria-label="Annuler"
                                    on:click=move |_| saving.set(None)
                                >
                                    <IconClose />
                                </button>
                            </form>
                        }
                    })
            }}
            {move || error.get().map(|m| view! { <p class="error">{m}</p> })}
        </section>
    }
}

/// Mini-carte poids : tendance, dernière pesée, pesée rapide (§2.8, §5.2).
#[component]
fn WeightCard(
    weight: Option<WeightView>,
    date: NaiveDate,
    goal_weight: f64,
    projection: Option<NaiveDate>,
    reload: RwSignal<u32>,
) -> impl IntoView {
    let input = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let save = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let Some(kg) = parse_number(&input.get()) else {
            error.set(Some("Indique un poids, par exemple 69,5.".into()));
            return;
        };
        spawn_local(async move {
            match api::add_weigh_in(date, kg).await {
                Ok(()) => reload.update(|n| *n += 1),
                Err(message) => error.set(Some(message)),
            }
        });
    };
    let weighed_today = weight.as_ref().is_some_and(|w| w.weighed_today);

    view! {
        <section class="glass card">
            <div class="card-head">
                <span class="label">"Poids · tendance"</span>
            </div>
            {match weight {
                Some(w) => {
                    view! {
                        <p class="metric" style="margin:0">
                            {one_decimal(w.trend_kg)}
                            <span class="unit">"kg"</span>
                        </p>
                        <p class="muted" style="margin:4px 0 0">
                            {format!(
                                "Dernière pesée : {} kg, le {}",
                                one_decimal(w.last_kg),
                                short_date(w.last_date),
                            )}
                        </p>
                        {w
                            .change_7d_kg
                            .map(|c| {
                                view! {
                                    <p class="muted" style="margin:4px 0 0">
                                        {format!("{} kg sur 7 jours", signed_one_decimal(c))}
                                    </p>
                                }
                            })}
                    }
                        .into_any()
                }
                None => view! { <p class="muted">"Aucune pesée pour l'instant."</p> }.into_any(),
            }}
            <p class="muted" style="margin:12px 0 0">
                {match projection {
                    Some(day) => {
                        format!(
                            "Objectif {} kg, estimé vers le {}.",
                            one_decimal(goal_weight),
                            short_date(day),
                        )
                    }
                    None => format!("Objectif {} kg.", one_decimal(goal_weight)),
                }}
            </p>
            <form on:submit=save style="display:flex; gap:8px; margin-top:16px">
                <input
                    class="input"
                    inputmode="decimal"
                    aria-label="Poids en kg"
                    placeholder=if weighed_today { "Nouvelle pesée (kg)" } else { "Pesée du jour (kg)" }
                    prop:value=move || input.get()
                    on:input=move |ev| input.set(event_target_value(&ev))
                />
                <button class="btn" type="submit">"Peser"</button>
            </form>
            {move || error.get().map(|m| view! { <p class="error">{m}</p> })}
        </section>
    }
}

/// Carte « Bilan de la semaine » (§5.4) : la semaine écoulée, puis le résultat du recalcul
/// hebdomadaire de la dépense, expliqué en une phrase (§2.3). Ton neutre (§2.7).
#[component]
fn WeekCard(report: WeekReport, reload: RwSignal<u32>) -> impl IntoView {
    let period = format!("du {} au {}", day_month(report.from), day_month(report.to));
    let mut lines = vec![format!("Jours journalisés : {} sur 7", report.logged_days)];
    if let (Some(eaten), Some(target)) = (report.mean_eaten_kcal, report.mean_target_kcal) {
        lines.push(format!(
            "Moyenne : {} kcal par jour, pour une cible de {}",
            int(eaten),
            int(target)
        ));
    }
    if let Some(gap) = report.cumulative_gap_kcal {
        lines.push(format!("Écart cumulé : {} kcal", signed_int(gap)));
    }
    if let Some(change) = report.trend_change_kg {
        lines.push(format!(
            "Poids de tendance : {} kg",
            signed_one_decimal(change)
        ));
    }

    let changed = (report.tdee_kcal - report.previous_tdee_kcal).abs() >= 0.5;
    let explanation = match report.computed_tdee_kcal {
        Some(computed) if report.frozen => format!(
            "Cible figée : ta dépense estimée reste à {} kcal (le calcul de la semaine donne {}).",
            int(report.tdee_kcal),
            int(computed)
        ),
        Some(computed) if changed => {
            let target = if (report.target_kcal - report.previous_target_kcal).abs() >= 0.5 {
                format!(
                    "Ta cible passe de {} à {} kcal.",
                    int(report.previous_target_kcal),
                    int(report.target_kcal)
                )
            } else {
                format!("Ta cible reste à {} kcal.", int(report.target_kcal))
            };
            let damped = if (computed - report.tdee_kcal).abs() >= 0.5 {
                format!(
                    " Le calcul donne {} kcal ; l'estimation change de 100 kcal par semaine au plus.",
                    int(computed)
                )
            } else {
                String::new()
            };
            format!(
                "Ta dépense estimée passe de {} à {} kcal. {target}{damped}",
                int(report.previous_tdee_kcal),
                int(report.tdee_kcal)
            )
        }
        Some(_) => format!("Ta dépense estimée reste à {} kcal.", int(report.tdee_kcal)),
        None => format!(
            "Pas encore assez de données pour recalculer ta dépense : {} jours journalisés sur les 14 nécessaires et {} jours avec pesée sur les 6 nécessaires, ces trois dernières semaines. Les pesées doivent s'étaler sur au moins une semaine.",
            report.window_logged_days, report.window_weigh_in_days
        ),
    };
    let frozen = report.frozen;

    view! {
        <section class="glass card" aria-label="Bilan de la semaine">
            <div class="card-head">
                <span class="label">"Bilan de la semaine"</span>
                <span class="muted">{period}</span>
            </div>
            {lines
                .into_iter()
                .map(|line| view! { <p class="week-line">{line}</p> })
                .collect_view()}
            <p class="muted week-note">{explanation}</p>
            <button
                class="btn text"
                on:click=move |_| {
                    spawn_local(async move {
                        if api::set_target_frozen(!frozen).await.is_ok() {
                            reload.update(|n| *n += 1);
                        }
                    });
                }
            >
                {if frozen { "Libérer ma cible" } else { "Figer ma cible" }}
            </button>
        </section>
    }
}
