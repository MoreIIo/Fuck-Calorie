//! Saisie du profil (§2.1) : au premier lancement, puis dans *Réglages*.

use chrono::{NaiveDate, Weekday};
use fc_core::api::ProfileInput;
use fc_core::goal::Goal;
use fc_core::profile::{ActivityLevel, Profile, Sex};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::format::{one_decimal, parse_number, weekday};

const ACTIVITY_LEVELS: [(ActivityLevel, &str); 5] = [
    (ActivityLevel::Sedentary, "Sédentaire"),
    (ActivityLevel::LightlyActive, "Légèrement actif"),
    (ActivityLevel::ModeratelyActive, "Modérément actif"),
    (ActivityLevel::VeryActive, "Très actif"),
    (ActivityLevel::ExtremelyActive, "Extrêmement actif"),
];

const WEEKDAYS: [Weekday; 7] = [
    Weekday::Mon,
    Weekday::Tue,
    Weekday::Wed,
    Weekday::Thu,
    Weekday::Fri,
    Weekday::Sat,
    Weekday::Sun,
];

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Lose,
    Maintain,
    Gain,
    Recomp,
}

/// Premier lancement.
#[component]
pub fn Onboarding(today: NaiveDate, reload: RwSignal<u32>) -> impl IntoView {
    view! {
        <div class="onboarding">
            <div>
                <p class="label">"Bienvenue"</p>
                <h1 class="h1">"Ton profil"</h1>
                <p class="muted">
                    "Ces informations servent à estimer ta dépense de départ. Elles restent sur cet appareil."
                </p>
            </div>
            <ProfileForm
                initial=None
                today=today
                submit_label="Commencer"
                on_saved=Callback::new(move |()| reload.update(|n| *n += 1))
            />
        </div>
    }
}

/// Formulaire du profil. Sans profil `initial` (premier lancement), il demande aussi le
/// poids actuel ; avec, il propose le premier jour de la semaine et garde le poids (le suivi
/// passe par les pesées, §2.8).
#[component]
pub fn ProfileForm(
    initial: Option<Profile>,
    today: NaiveDate,
    submit_label: &'static str,
    on_saved: Callback<()>,
) -> impl IntoView {
    let editing = initial.is_some();
    let number = |v: f64| one_decimal(v).replace(",0", "");
    let sex = RwSignal::new(initial.as_ref().map_or(Sex::Female, |p| p.sex));
    let birth = RwSignal::new(
        initial
            .as_ref()
            .map(|p| p.birth_date.to_string())
            .unwrap_or_default(),
    );
    let height = RwSignal::new(
        initial
            .as_ref()
            .map(|p| number(p.height_cm))
            .unwrap_or_default(),
    );
    let weight = RwSignal::new(
        initial
            .as_ref()
            .map(|p| one_decimal(p.weight_kg))
            .unwrap_or_default(),
    );
    let goal_weight = RwSignal::new(
        initial
            .as_ref()
            .map(|p| one_decimal(p.goal_weight_kg))
            .unwrap_or_default(),
    );
    let (initial_mode, initial_rate) = match initial.as_ref().map(|p| p.goal) {
        None | Some(Goal::Lose { .. }) => (
            Mode::Lose,
            initial
                .as_ref()
                .and_then(|p| p.goal.target_kg_per_week())
                .map_or(0.5, f64::abs),
        ),
        Some(Goal::Maintain) => (Mode::Maintain, 0.5),
        Some(Goal::Gain { kg_per_week }) => (Mode::Gain, kg_per_week),
        Some(Goal::Recomp) => (Mode::Recomp, 0.5),
    };
    let mode = RwSignal::new(initial_mode);
    let rate = RwSignal::new(initial_rate);
    let activity = RwSignal::new(
        initial
            .as_ref()
            .and_then(|p| {
                ACTIVITY_LEVELS
                    .iter()
                    .position(|(l, _)| *l == p.activity_level)
            })
            .unwrap_or(0),
    );
    let week_start = RwSignal::new(initial.as_ref().map_or(Weekday::Mon, |p| p.week_start));
    let error = RwSignal::new(None::<String>);
    let saving = RwSignal::new(false);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let Ok(birth_date) = birth.get().parse::<NaiveDate>() else {
            error.set(Some("Indique ta date de naissance.".into()));
            return;
        };
        let (Some(height_cm), Some(weight_kg), Some(goal_weight_kg)) = (
            parse_number(&height.get()),
            parse_number(&weight.get()),
            parse_number(&goal_weight.get()),
        ) else {
            error.set(Some(if editing {
                "Indique ta taille et ton poids objectif.".into()
            } else {
                "Indique ta taille, ton poids et ton poids objectif.".into()
            }));
            return;
        };
        let goal = match mode.get() {
            Mode::Lose => Goal::Lose {
                kg_per_week: rate.get(),
            },
            Mode::Maintain => Goal::Maintain,
            Mode::Gain => Goal::Gain {
                kg_per_week: rate.get().min(0.5),
            },
            Mode::Recomp => Goal::Recomp,
        };
        let input = ProfileInput {
            sex: sex.get(),
            birth_date,
            height_cm,
            weight_kg,
            goal_weight_kg,
            goal,
            activity_level: ACTIVITY_LEVELS[activity.get()].0,
            week_start: editing.then(|| week_start.get()),
        };
        error.set(None);
        saving.set(true);
        spawn_local(async move {
            match api::save_profile(input, today).await {
                Ok(_) => on_saved.run(()),
                Err(message) => {
                    let _ = error.try_set(Some(message));
                }
            }
            let _ = saving.try_set(false);
        });
    };

    let sex_choice = move |value: Sex, label: &'static str| {
        view! {
            <button
                type="button"
                class="choice"
                class:on=move || sex.get() == value
                on:click=move |_| sex.set(value)
            >
                {label}
            </button>
        }
    };
    let mode_choice = move |value: Mode, label: &'static str| {
        view! {
            <button
                type="button"
                class="choice"
                class:on=move || mode.get() == value
                on:click=move |_| {
                    mode.set(value);
                    if value == Mode::Gain && rate.get() > 0.5 {
                        rate.set(0.25);
                    }
                }
            >
                {label}
            </button>
        }
    };
    let rate_choice = move |value: f64| {
        view! {
            <button
                type="button"
                class="choice"
                class:on=move || (rate.get() - value).abs() < 1e-9
                on:click=move |_| rate.set(value)
            >
                {format!("{} kg/sem.", value.to_string().replace('.', ","))}
            </button>
        }
    };
    let text_field =
        move |label: &'static str, signal: RwSignal<String>, placeholder: &'static str| {
            view! {
                <label class="field">
                    <span class="label">{label}</span>
                    <input
                        class="input"
                        inputmode="decimal"
                        placeholder=placeholder
                        prop:value=move || signal.get()
                        on:input=move |ev| signal.set(event_target_value(&ev))
                    />
                </label>
            }
        };

    view! {
        <form class="profile-form" on:submit=submit>
            <div class="glass card">
                <div class="field">
                    <span class="label">"Sexe pour le calcul"</span>
                    <div class="choices">
                        {sex_choice(Sex::Female, "Femme")}
                        {sex_choice(Sex::Male, "Homme")}
                        {sex_choice(Sex::Unspecified, "Non précisé")}
                    </div>
                </div>
                <label class="field">
                    <span class="label">"Date de naissance"</span>
                    <input
                        class="input"
                        type="date"
                        prop:value=move || birth.get()
                        on:input=move |ev| birth.set(event_target_value(&ev))
                    />
                </label>
                <div class="row">
                    {text_field("Taille (cm)", height, "170")}
                    {(!editing).then(|| text_field("Poids (kg)", weight, "70,0"))}
                    {text_field("Poids objectif (kg)", goal_weight, "62,0")}
                </div>
            </div>
            <div class="glass card">
                <div class="field">
                    <span class="label">"Objectif"</span>
                    <div class="choices">
                        {mode_choice(Mode::Lose, "Perte de poids")}
                        {mode_choice(Mode::Maintain, "Maintien")}
                        {mode_choice(Mode::Gain, "Prise de masse")}
                        {mode_choice(Mode::Recomp, "Recomposition")}
                    </div>
                </div>
                <Show when=move || matches!(mode.get(), Mode::Lose | Mode::Gain)>
                    <div class="field">
                        <span class="label">"Rythme"</span>
                        <div class="choices">
                            {move || {
                                let rates: &[f64] = if mode.get() == Mode::Gain {
                                    &[0.25, 0.5]
                                } else {
                                    &[0.25, 0.5, 0.75, 1.0]
                                };
                                rates.iter().map(|r| rate_choice(*r)).collect_view()
                            }}
                        </div>
                    </div>
                </Show>
                <label class="field">
                    <span class="label">"Niveau d'activité (hors sport enregistré)"</span>
                    <select
                        class="select"
                        on:change=move |ev| {
                            activity.set(event_target_value(&ev).parse().unwrap_or(0));
                        }
                    >
                        {ACTIVITY_LEVELS
                            .iter()
                            .enumerate()
                            .map(|(i, (_, label))| {
                                view! {
                                    <option value=i.to_string() selected=move || activity.get() == i>
                                        {*label}
                                    </option>
                                }
                            })
                            .collect_view()}
                    </select>
                </label>
                {editing
                    .then(|| {
                        view! {
                            <label class="field">
                                <span class="label">
                                    "Premier jour de la semaine (jour du bilan)"
                                </span>
                                <select
                                    class="select"
                                    on:change=move |ev| {
                                        let index: usize = event_target_value(&ev)
                                            .parse()
                                            .unwrap_or(0);
                                        week_start.set(WEEKDAYS[index.min(6)]);
                                    }
                                >
                                    {WEEKDAYS
                                        .iter()
                                        .enumerate()
                                        .map(|(i, day)| {
                                            let day = *day;
                                            view! {
                                                <option
                                                    value=i.to_string()
                                                    selected=move || week_start.get() == day
                                                >
                                                    {capitalized(weekday(day))}
                                                </option>
                                            }
                                        })
                                        .collect_view()}
                                </select>
                            </label>
                        }
                    })}
            </div>
            {move || error.get().map(|message| view! { <p class="error">{message}</p> })}
            <button class="btn primary block" type="submit" disabled=move || saving.get()>
                {submit_label}
            </button>
        </form>
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
