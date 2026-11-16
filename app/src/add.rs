//! Feuille d'ajout (§5.1) : recherche d'un aliment (§3.3), écran de quantité (§3.4), ajout
//! rapide et repas enregistrés (§3.6), aliments personnels (§3.7). Elle sert aussi à modifier
//! une ligne du journal (§3.5).
//!
//! La recherche locale (Ciqual, produits déjà consultés) répond à chaque frappe. La
//! recherche en ligne sur Open Food Facts part après une courte pause dans la frappe, ou
//! tout de suite avec la touche Entrée.

use std::time::Duration;

use chrono::NaiveDate;
use fc_core::api::{
    CustomFoodInput, FoodDetail, FoodHit, LogFoodInput, MealView, QuickAddInput, SavedMealView,
    UpdateEntryInput,
};
use fc_core::food::{Food, FoodSource, Unit};
use fc_core::nutrients::NutrientFacts;
use leptos::prelude::*;
use leptos::task::spawn_local;
use uuid::Uuid;

use crate::api;
use crate::format::{int, one_decimal, parse_number, quantity};
use crate::icons::{IconClose, IconLeft};

/// Pause dans la frappe avant d'interroger Open Food Facts. Plus longue que les 300 ms du
/// §3.1 : Open Food Facts n'accepte qu'une dizaine de recherches par minute.
const ONLINE_DELAY: Duration = Duration::from_millis(700);

/// Ce que montre la feuille.
#[derive(Clone)]
enum Mode {
    Search,
    /// Écran de quantité ; `Some(ligne)` quand on modifie une ligne existante du journal.
    Quantity(Box<FoodDetail>, Option<Uuid>),
    Quick,
    /// Création d'un aliment personnel (§3.7).
    Custom,
    /// Ouverture d'une ligne du journal en cours.
    Loading,
    /// Ligne sans fiche (ajout rapide) : seul son repas peut changer. Identifiant, libellé.
    Move(Uuid, String),
    Failed(String),
}

/// Ce pour quoi la feuille est ouverte.
#[derive(Clone, Copy, PartialEq)]
pub enum SheetRequest {
    /// Ajouter au repas indiqué.
    Add(Uuid),
    /// Modifier une ligne du journal : identifiant de la ligne, puis de son repas.
    Edit(Uuid, Uuid),
}

/// Listes proposées sous la barre de recherche (§3.3).
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Recent,
    Favorites,
    Mine,
    Meals,
}

const TABS: [(Tab, &str); 4] = [
    (Tab::Recent, "Récents"),
    (Tab::Favorites, "Favoris"),
    (Tab::Mine, "Mes aliments"),
    (Tab::Meals, "Repas"),
];

/// État de la recherche en ligne.
#[derive(Clone, PartialEq)]
enum Online {
    /// Requête trop courte : pas de recherche en ligne.
    Idle,
    /// En attente d'une pause dans la frappe.
    Waiting,
    Loading,
    Done(Vec<FoodHit>),
    Error(String),
}

#[component]
pub fn AddSheet(
    meals: Vec<MealView>,
    request: SheetRequest,
    date: NaiveDate,
    /// Calories restantes de la journée, pour montrer l'effet de l'ajout (§3.4).
    remaining_kcal: f64,
    sheet: RwSignal<Option<SheetRequest>>,
    reload: RwSignal<u32>,
) -> impl IntoView {
    let (initial_meal, editing) = match request {
        SheetRequest::Add(meal) => (meal, None),
        SheetRequest::Edit(entry, meal) => (meal, Some(entry)),
    };
    let meal = RwSignal::new(initial_meal);
    let mode = RwSignal::new(if editing.is_some() {
        Mode::Loading
    } else {
        Mode::Search
    });
    // Budget montré dans l'aperçu. Pour une modification, les calories actuelles de la ligne
    // sont d'abord rendues au budget : elles seront remplacées, pas ajoutées.
    let budget = RwSignal::new(remaining_kcal);
    if let Some(entry) = editing {
        spawn_local(async move {
            let next = match api::entry_detail(entry).await {
                Ok(detail) => match detail.food {
                    Some(food) => {
                        let _ = budget.try_set(remaining_kcal + detail.kcal);
                        Mode::Quantity(Box::new(food), Some(detail.id))
                    }
                    None => Mode::Move(detail.id, detail.label),
                },
                Err(message) => Mode::Failed(message),
            };
            let _ = mode.try_set(next);
        });
    }
    let close = move || sheet.set(None);
    let title = move || match mode.get() {
        Mode::Search => "Ajouter",
        Mode::Quantity(_, None) => "Quantité",
        Mode::Quick => "Ajout rapide",
        Mode::Custom => "Nouvel aliment",
        Mode::Quantity(_, Some(_)) | Mode::Loading | Mode::Move(..) | Mode::Failed(_) => "Modifier",
    };

    view! {
        <div
            class="scrim"
            on:click=move |_| close()
            on:keydown=move |ev| {
                if ev.key() == "Escape" {
                    close();
                }
            }
        >
            <div
                class="glass sheet"
                role="dialog"
                aria-label=title
                on:click=|ev| ev.stop_propagation()
            >
                <div class="sheet-head">
                    {move || {
                        matches!(mode.get(), Mode::Quantity(_, None) | Mode::Quick | Mode::Custom)
                            .then(|| {
                                view! {
                                    <button
                                        type="button"
                                        class="icon-btn"
                                        aria-label="Retour à la recherche"
                                        on:click=move |_| mode.set(Mode::Search)
                                    >
                                        <IconLeft />
                                    </button>
                                }
                            })
                    }}
                    <h2 class="h2">{title}</h2>
                    <button type="button" class="icon-btn" aria-label="Fermer" on:click=move |_| close()>
                        <IconClose />
                    </button>
                </div>
                <div class="choices" role="group" aria-label="Repas">
                    {meals
                        .into_iter()
                        .map(|m| {
                            let id = m.id;
                            view! {
                                <button
                                    type="button"
                                    class="choice"
                                    class:on=move || meal.get() == id
                                    aria-pressed=move || (meal.get() == id).to_string()
                                    on:click=move |_| meal.set(id)
                                >
                                    {m.name}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
                {move || match mode.get() {
                    Mode::Search => {
                        view! {
                            <SearchPane mode=mode meal=meal date=date sheet=sheet reload=reload />
                        }
                            .into_any()
                    }
                    Mode::Custom => view! { <CustomPane mode=mode /> }.into_any(),
                    Mode::Loading => view! { <p class="muted">"Chargement…"</p> }.into_any(),
                    Mode::Failed(message) => view! { <p class="error">{message}</p> }.into_any(),
                    Mode::Move(id, label) => {
                        view! { <MovePane id=id label=label meal=meal sheet=sheet reload=reload /> }
                            .into_any()
                    }
                    Mode::Quantity(detail, editing) => {
                        view! {
                            <QuantityPane
                                detail=*detail
                                editing=editing
                                meal=meal
                                date=date
                                remaining_kcal=budget.get_untracked()
                                mode=mode
                                sheet=sheet
                                reload=reload
                            />
                        }
                            .into_any()
                    }
                    Mode::Quick => {
                        view! { <QuickPane meal=meal date=date sheet=sheet reload=reload /> }
                            .into_any()
                    }
                }}
            </div>
        </div>
    }
}

// ----- Recherche (§3.3) -------------------------------------------------------------------

fn source_name(source: FoodSource) -> &'static str {
    match source {
        FoodSource::Ciqual => "Ciqual",
        FoodSource::Off => "Open Food Facts",
        FoodSource::Custom => "Perso",
        FoodSource::Recipe => "Recette",
    }
}

/// Une ligne de résultat : nom, kcal / 100 g, puis marque, portion par défaut et source.
fn hit_row(hit: FoodHit, open: impl Fn(FoodHit) + Copy + 'static) -> impl IntoView {
    let kcal = match hit.kcal_100 {
        Some(kcal) => format!("{} kcal/100 g", int(kcal)),
        None => "calories inconnues".to_owned(),
    };
    let detail = [
        hit.brand.clone(),
        hit.portion
            .as_ref()
            .map(|(label, grams)| format!("{label} = {} g", quantity(*grams))),
        Some(source_name(hit.source).to_owned()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let name = hit.name.clone();
    view! {
        <button type="button" class="hit" on:click=move |_| open(hit.clone())>
            <span class="name">{name}</span>
            <span class="kcal muted">{kcal}</span>
            <span class="detail muted">{detail}</span>
        </button>
    }
}

#[component]
fn SearchPane(
    mode: RwSignal<Mode>,
    meal: RwSignal<Uuid>,
    date: NaiveDate,
    sheet: RwSignal<Option<SheetRequest>>,
    reload: RwSignal<u32>,
) -> impl IntoView {
    let query = RwSignal::new(String::new());
    // Liste de l'onglet choisi (récents, favoris, mes aliments) et repas enregistrés.
    let tab = RwSignal::new(Tab::Recent);
    let listed = RwSignal::new(Vec::<FoodHit>::new());
    let saved = RwSignal::new(Vec::<SavedMealView>::new());
    let local = RwSignal::new(Vec::<FoodHit>::new());
    let online = RwSignal::new(Online::Idle);
    let error = RwSignal::new(None::<String>);
    let opening = RwSignal::new(false);
    // Numéro de la dernière frappe : une réponse arrivée après une nouvelle frappe est ignorée.
    let typed = StoredValue::new(0_u32);
    let input_ref = NodeRef::<leptos::html::Input>::new();

    Effect::new(move |_| {
        if let Some(el) = input_ref.get() {
            let _ = el.focus();
        }
    });
    let load = move |which: Tab| {
        spawn_local(async move {
            if which == Tab::Meals {
                if let Ok(meals) = api::saved_meals().await {
                    let _ = saved.try_set(meals);
                }
                return;
            }
            let result = match which {
                Tab::Recent => api::recent_foods().await,
                Tab::Favorites => api::favorite_foods().await,
                Tab::Mine | Tab::Meals => api::custom_foods().await,
            };
            // L'onglet a pu changer pendant le chargement.
            if tab.try_get_untracked() == Some(which) {
                let _ = listed.try_set(result.unwrap_or_default());
            }
        });
    };
    load(Tab::Recent);

    let search_online = move |text: String, id: u32| {
        online.set(Online::Loading);
        spawn_local(async move {
            let result = api::search_off(text).await;
            if typed.try_get_value() != Some(id) {
                return;
            }
            let _ = online.try_set(match result {
                Ok(hits) => Online::Done(hits),
                Err(message) => Online::Error(message),
            });
        });
    };

    let on_input = move |ev: leptos::ev::Event| {
        let text = event_target_value(&ev);
        query.set(text.clone());
        let id = typed.get_value().wrapping_add(1);
        typed.set_value(id);
        error.set(None);
        let length = text.trim().chars().count();
        if length < 2 {
            local.set(Vec::new());
            online.set(Online::Idle);
            return;
        }
        let local_query = text.clone();
        spawn_local(async move {
            let result = api::search_foods(local_query).await;
            if typed.try_get_value() != Some(id) {
                return;
            }
            match result {
                Ok(hits) => {
                    let _ = local.try_set(hits);
                }
                Err(message) => {
                    let _ = error.try_set(Some(message));
                }
            }
        });
        if length < 3 {
            online.set(Online::Idle);
            return;
        }
        online.set(Online::Waiting);
        set_timeout(
            move || {
                let still_waiting = typed.try_get_value() == Some(id)
                    && online.try_get_untracked() == Some(Online::Waiting);
                if still_waiting {
                    search_online(text, id);
                }
            },
            ONLINE_DELAY,
        );
    };

    // Entrée : recherche en ligne sans attendre la pause.
    let on_keydown = move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Enter" && online.get_untracked() == Online::Waiting {
            search_online(query.get_untracked(), typed.get_value());
        }
    };

    let open = move |hit: FoodHit| {
        if opening.get_untracked() {
            return;
        }
        opening.set(true);
        error.set(None);
        spawn_local(async move {
            let result = match (hit.id, hit.barcode) {
                (Some(id), _) => api::food_detail(id).await,
                (None, Some(barcode)) => api::off_product(barcode).await,
                (None, None) => Err("Cette fiche est introuvable.".to_owned()),
            };
            let _ = opening.try_set(false);
            match result {
                Ok(detail) => {
                    let _ = mode.try_set(Mode::Quantity(Box::new(detail), None));
                }
                Err(message) => {
                    let _ = error.try_set(Some(message));
                }
            }
        });
    };

    let rows = move |hits: Vec<FoodHit>| {
        hits.into_iter()
            .map(|hit| hit_row(hit, open))
            .collect_view()
    };

    // Un repas enregistré : un clic l'ajoute au repas choisi en haut de la feuille.
    let saved_row = move |saved_meal: SavedMealView| {
        let id = saved_meal.id;
        let count = match saved_meal.items {
            1 => "1 aliment".to_owned(),
            n => format!("{n} aliments"),
        };
        view! {
            <div class="saved">
                <button
                    type="button"
                    class="hit"
                    on:click=move |_| {
                        let target = meal.get_untracked();
                        spawn_local(async move {
                            match api::add_saved_meal(id, date, target).await {
                                Ok(_) => {
                                    sheet.set(None);
                                    reload.update(|n| *n += 1);
                                }
                                Err(message) => {
                                    let _ = error.try_set(Some(message));
                                }
                            }
                        });
                    }
                >
                    <span class="name">{saved_meal.name}</span>
                    <span class="kcal muted">{format!("{} kcal", int(saved_meal.kcal))}</span>
                    <span class="detail muted">{format!("{count} · ajouter au repas choisi")}</span>
                </button>
                <button
                    type="button"
                    class="icon-btn"
                    aria-label="Supprimer ce repas enregistré"
                    on:click=move |_| {
                        spawn_local(async move {
                            if api::delete_saved_meal(id).await.is_ok() {
                                load(Tab::Meals);
                            }
                        });
                    }
                >
                    <IconClose />
                </button>
            </div>
        }
    };

    let results = move || {
        if query.get().trim().chars().count() < 2 {
            let current = tab.get();
            let tabs = TABS
                .into_iter()
                .map(|(which, name)| {
                    view! {
                        <button
                            type="button"
                            class="choice"
                            class:on=current == which
                            aria-pressed=(current == which).to_string()
                            on:click=move |_| {
                                listed.set(Vec::new());
                                tab.set(which);
                                load(which);
                            }
                        >
                            {name}
                        </button>
                    }
                })
                .collect_view();
            let empty = |text: &'static str| view! { <p class="muted empty">{text}</p> }.into_any();
            let body = if current == Tab::Meals {
                let meals = saved.get();
                if meals.is_empty() {
                    empty(
                        "Aucun repas enregistré. Dans la carte d'un repas, « Enregistrer ce repas » le garde pour le rajouter en un clic.",
                    )
                } else {
                    meals.into_iter().map(saved_row).collect_view().into_any()
                }
            } else {
                let hits = listed.get();
                if hits.is_empty() {
                    empty(match current {
                        Tab::Recent => {
                            "Tape au moins 2 lettres : aliments courants (Ciqual) et produits de supermarché (Open Food Facts)."
                        }
                        Tab::Favorites => {
                            "Aucun favori. Ouvre un aliment et choisis « Mettre en favori »."
                        }
                        Tab::Mine | Tab::Meals => "Aucun aliment personnel pour l'instant.",
                    })
                } else {
                    rows(hits).into_any()
                }
            };
            return view! {
                <div class="choices" role="group" aria-label="Listes">
                    {tabs}
                </div>
                {body}
                {(current == Tab::Mine)
                    .then(|| {
                        view! {
                            <button
                                type="button"
                                class="btn text"
                                on:click=move |_| mode.set(Mode::Custom)
                            >
                                "+ Créer un aliment"
                            </button>
                        }
                    })}
            }
            .into_any();
        }
        let found = local.get();
        // Un produit déjà affiché dans les résultats locaux n'est pas répété.
        let known = found.clone();
        let is_new = move |hit: &FoodHit| {
            !known.iter().any(|k| {
                (hit.id.is_some() && k.id == hit.id)
                    || (hit.barcode.is_some() && k.barcode == hit.barcode)
            })
        };
        let online_part = match online.get() {
            Online::Idle => ().into_any(),
            Online::Waiting | Online::Loading => {
                view! { <p class="muted empty">"Recherche sur Open Food Facts…"</p> }.into_any()
            }
            Online::Error(message) => view! { <p class="muted empty">{message}</p> }.into_any(),
            Online::Done(hits) => {
                let hits: Vec<FoodHit> = hits.into_iter().filter(|h| is_new(h)).collect();
                if hits.is_empty() {
                    view! { <p class="muted empty">"Aucun autre produit sur Open Food Facts."</p> }
                        .into_any()
                } else {
                    view! {
                        <p class="label">"Open Food Facts"</p>
                        {rows(hits)}
                    }
                    .into_any()
                }
            }
        };
        let none_local = found.is_empty();
        view! {
            {none_local
                .then(|| view! { <p class="muted empty">"Aucun aliment trouvé sur cet appareil."</p> })}
            {rows(found)}
            {online_part}
        }
            .into_any()
    };

    view! {
        <label class="field">
            <span class="label">"Rechercher un aliment"</span>
            <input
                class="input"
                type="search"
                autocomplete="off"
                placeholder="Yaourt, pomme, une marque…"
                node_ref=input_ref
                prop:value=move || query.get()
                on:input=on_input
                on:keydown=on_keydown
            />
        </label>
        {move || error.get().map(|m| view! { <p class="error">{m}</p> })}
        <div class="results" class:busy=move || opening.get() aria-live="polite">
            {results}
        </div>
        <button type="button" class="btn text" on:click=move |_| mode.set(Mode::Quick)>
            "Ajout rapide (calories seules)"
        </button>
    }
}

// ----- Quantité (§3.4) --------------------------------------------------------------------

/// Libellé d'une unité : « g », « ml », « 1 portion (125 g) », « pièce (50 g) ».
fn unit_label(food: &Food, unit: &Unit) -> String {
    let with_weight = |label: &str| match food.grams_for(1.0, unit) {
        Ok(grams) => format!("{label} ({} g)", quantity(grams)),
        Err(_) => label.to_owned(),
    };
    match unit {
        Unit::Gram => "g".to_owned(),
        Unit::Milliliter => "ml".to_owned(),
        Unit::Portion(label) => with_weight(label),
        Unit::Piece => with_weight("pièce"),
    }
}

/// Quantité proposée quand on change d'unité : 100 g ou 100 ml, sinon 1 portion ou 1 pièce.
fn default_amount(unit: &Unit) -> f64 {
    match unit {
        Unit::Gram | Unit::Milliliter => 100.0,
        Unit::Portion(_) | Unit::Piece => 1.0,
    }
}

#[component]
fn QuantityPane(
    detail: FoodDetail,
    /// Ligne du journal en cours de modification ; `None` pour un ajout.
    editing: Option<Uuid>,
    meal: RwSignal<Uuid>,
    date: NaiveDate,
    remaining_kcal: f64,
    mode: RwSignal<Mode>,
    sheet: RwSignal<Option<SheetRequest>>,
    reload: RwSignal<u32>,
) -> impl IntoView {
    let FoodDetail {
        food,
        quantity: initial,
        unit: initial_unit,
        units,
        favorite,
    } = detail;
    let favorite = RwSignal::new(favorite);
    let has_energy = food.per_100.has_energy();
    let per_100 = match (food.per_100.kcal, food.density.is_some()) {
        (Some(kcal), true) => format!("{} kcal pour 100 ml", int(kcal)),
        (Some(kcal), false) => format!("{} kcal pour 100 g", int(kcal)),
        (None, _) => "Calories inconnues".to_owned(),
    };
    let attribution = match food.source {
        FoodSource::Ciqual => Some("Source : Table Ciqual, ANSES"),
        FoodSource::Off => Some("Données Open Food Facts"),
        FoodSource::Custom | FoodSource::Recipe => None,
    };
    let name = food.name.clone();
    let brand = food.brand.clone();
    let unit_choices: Vec<(Unit, String)> = units
        .into_iter()
        .map(|unit| {
            let label = unit_label(&food, &unit);
            (unit, label)
        })
        .collect();
    let food_id = food.meta.id;
    let food = StoredValue::new(food);

    let unit = RwSignal::new(initial_unit);
    let amount = RwSignal::new(quantity(initial));
    // Quantité de référence des raccourcis ×½, ×1, ×2.
    let base = RwSignal::new(initial);
    let error = RwSignal::new(None::<String>);
    let input_ref = NodeRef::<leptos::html::Input>::new();

    Effect::new(move |_| {
        if let Some(el) = input_ref.get() {
            let _ = el.focus();
            el.select();
        }
    });

    // Aperçu en temps réel : le calcul est celui de `fc-core`, le même qu'à l'enregistrement.
    let preview = Memo::new(move |_| {
        let value = parse_number(&amount.get())?;
        let unit = unit.get();
        food.with_value(|f| f.nutrients_for(value, &unit).ok())
    });

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let Some(value) = parse_number(&amount.get_untracked()) else {
            error.set(Some("Indique une quantité.".into()));
            return;
        };
        let meal_slot_id = meal.get_untracked();
        let unit = unit.get_untracked();
        spawn_local(async move {
            let result = match editing {
                Some(id) => {
                    let input = UpdateEntryInput {
                        id,
                        meal_slot_id,
                        quantity: Some(value),
                        unit: Some(unit),
                    };
                    api::update_entry(input).await
                }
                None => {
                    let input = LogFoodInput {
                        date,
                        meal_slot_id,
                        food_id,
                        quantity: value,
                        unit,
                    };
                    api::log_food(input).await
                }
            };
            match result {
                Ok(()) => {
                    sheet.set(None);
                    reload.update(|n| *n += 1);
                }
                Err(message) => {
                    let _ = error.try_set(Some(message));
                }
            }
        });
    };

    let shortcut = move |label: &'static str, factor: f64| {
        view! {
            <button
                type="button"
                class="choice"
                on:click=move |_| amount.set(quantity(base.get_untracked() * factor))
            >
                {label}
            </button>
        }
    };
    let step = move || match unit.get() {
        Unit::Gram => ("+10 g", 10.0),
        Unit::Milliliter => ("+10 ml", 10.0),
        Unit::Portion(_) | Unit::Piece => ("+1", 1.0),
    };

    view! {
        <form class="quantity" on:submit=submit>
            <div>
                <p class="h2">{name}</p>
                <button
                    type="button"
                    class="btn text"
                    aria-pressed=move || favorite.get().to_string()
                    on:click=move |_| {
                        let next = !favorite.get_untracked();
                        spawn_local(async move {
                            if api::set_favorite(food_id, next).await.is_ok() {
                                let _ = favorite.try_set(next);
                            }
                        });
                    }
                >
                    {move || if favorite.get() { "★ Favori" } else { "☆ Mettre en favori" }}
                </button>
                <p class="muted">
                    {brand.map(|b| format!("{b} · ")).unwrap_or_default()} {per_100}
                </p>
            </div>
            <div class="row">
                <label class="field">
                    <span class="label">"Quantité"</span>
                    <input
                        class="input"
                        inputmode="decimal"
                        node_ref=input_ref
                        prop:value=move || amount.get()
                        on:input=move |ev| amount.set(event_target_value(&ev))
                    />
                </label>
            </div>
            <div class="choices" role="group" aria-label="Unité">
                {unit_choices
                    .into_iter()
                    .map(|(choice, label)| {
                        let selected = choice.clone();
                        let pressed = choice.clone();
                        view! {
                            <button
                                type="button"
                                class="choice"
                                class:on=move || unit.get() == selected
                                aria-pressed=move || (unit.get() == pressed).to_string()
                                on:click=move |_| {
                                    let value = default_amount(&choice);
                                    unit.set(choice.clone());
                                    base.set(value);
                                    amount.set(quantity(value));
                                }
                            >
                                {label}
                            </button>
                        }
                    })
                    .collect_view()}
            </div>
            <div class="choices" role="group" aria-label="Raccourcis">
                {shortcut("×½", 0.5)}
                {shortcut("×1", 1.0)}
                {shortcut("×2", 2.0)}
                <button
                    type="button"
                    class="choice"
                    on:click=move |_| {
                        let current = parse_number(&amount.get_untracked()).unwrap_or(0.0);
                        amount.set(quantity(current + step().1));
                    }
                >
                    {move || step().0}
                </button>
            </div>
            <div class="preview" aria-live="polite">
                {move || match preview.get() {
                    Some(n) => {
                        let left = remaining_kcal - n.kcal;
                        let budget = if left >= 0.0 {
                            format!("Il restera {} kcal sur la journée.", int(left))
                        } else {
                            format!("La journée dépassera la cible de {} kcal.", int(-left))
                        };
                        view! {
                            <p class="metric">
                                {int(n.kcal)}
                                <span class="unit">"kcal"</span>
                            </p>
                            <p class="muted">
                                {format!(
                                    "Protéines {} g · Glucides {} g · Lipides {} g",
                                    one_decimal(n.protein_g),
                                    one_decimal(n.carbs_g),
                                    one_decimal(n.fat_g),
                                )}
                            </p>
                            <p class="muted">{budget}</p>
                        }
                            .into_any()
                    }
                    None => view! { <p class="muted">"Indique une quantité."</p> }.into_any(),
                }}
            </div>
            {(!has_energy)
                .then(|| {
                    view! {
                        <p class="error">
                            "Cette fiche n'indique pas les calories. "
                            <button type="button" class="btn text" on:click=move |_| mode.set(Mode::Quick)>
                                "Passer par l'ajout rapide"
                            </button>
                        </p>
                    }
                })}
            {move || error.get().map(|m| view! { <p class="error">{m}</p> })}
            <button class="btn primary block" type="submit" disabled=!has_energy>
                {if editing.is_some() { "Enregistrer" } else { "Ajouter" }}
            </button>
            {attribution.map(|text| view! { <p class="attribution muted">{text}</p> })}
        </form>
    }
}

// ----- Ajout rapide (§3.6) ----------------------------------------------------------------

/// Calories, libellé et macros facultatifs.
#[component]
fn QuickPane(
    meal: RwSignal<Uuid>,
    date: NaiveDate,
    sheet: RwSignal<Option<SheetRequest>>,
    reload: RwSignal<u32>,
) -> impl IntoView {
    let kcal = RwSignal::new(String::new());
    let label = RwSignal::new(String::new());
    let protein = RwSignal::new(String::new());
    let carbs = RwSignal::new(String::new());
    let fat = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let input_ref = NodeRef::<leptos::html::Input>::new();

    // Le champ des calories prend le focus à l'ouverture.
    Effect::new(move |_| {
        if let Some(el) = input_ref.get() {
            let _ = el.focus();
        }
    });

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let Some(value) = parse_number(&kcal.get_untracked()) else {
            error.set(Some("Indique les calories.".into()));
            return;
        };
        let input = QuickAddInput {
            date,
            meal_slot_id: meal.get_untracked(),
            label: label.get_untracked(),
            kcal: value,
            protein_g: parse_number(&protein.get_untracked()),
            carbs_g: parse_number(&carbs.get_untracked()),
            fat_g: parse_number(&fat.get_untracked()),
        };
        spawn_local(async move {
            match api::quick_add(input).await {
                Ok(()) => {
                    sheet.set(None);
                    reload.update(|n| *n += 1);
                }
                Err(message) => {
                    let _ = error.try_set(Some(message));
                }
            }
        });
    };

    let small_field = move |name: &'static str, signal: RwSignal<String>| {
        view! {
            <label class="field">
                <span class="label">{name}</span>
                <input
                    class="input"
                    inputmode="decimal"
                    placeholder="g"
                    prop:value=move || signal.get()
                    on:input=move |ev| signal.set(event_target_value(&ev))
                />
            </label>
        }
    };

    view! {
        <form class="quantity" on:submit=submit>
            <label class="field">
                <span class="label">"Calories"</span>
                <input
                    class="input"
                    inputmode="decimal"
                    placeholder="kcal"
                    node_ref=input_ref
                    prop:value=move || kcal.get()
                    on:input=move |ev| kcal.set(event_target_value(&ev))
                />
            </label>
            <label class="field">
                <span class="label">"Libellé (facultatif)"</span>
                <input
                    class="input"
                    placeholder="Resto italien"
                    prop:value=move || label.get()
                    on:input=move |ev| label.set(event_target_value(&ev))
                />
            </label>
            <div class="row">
                {small_field("Protéines", protein)} {small_field("Glucides", carbs)}
                {small_field("Lipides", fat)}
            </div>
            {move || error.get().map(|m| view! { <p class="error">{m}</p> })}
            <button class="btn primary block" type="submit">
                "Ajouter"
            </button>
        </form>
    }
}

// ----- Déplacement d'un ajout rapide (§3.5) -----------------------------------------------

/// Un ajout rapide n'a ni fiche ni quantité : on ne peut que le changer de repas.
#[component]
fn MovePane(
    id: Uuid,
    label: String,
    meal: RwSignal<Uuid>,
    sheet: RwSignal<Option<SheetRequest>>,
    reload: RwSignal<u32>,
) -> impl IntoView {
    let error = RwSignal::new(None::<String>);
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let input = UpdateEntryInput {
            id,
            meal_slot_id: meal.get_untracked(),
            quantity: None,
            unit: None,
        };
        spawn_local(async move {
            match api::update_entry(input).await {
                Ok(()) => {
                    sheet.set(None);
                    reload.update(|n| *n += 1);
                }
                Err(message) => {
                    let _ = error.try_set(Some(message));
                }
            }
        });
    };
    view! {
        <form class="quantity" on:submit=submit>
            <p class="h2">{label}</p>
            <p class="muted">
                "Ajout rapide : choisis ci-dessus le repas où le déplacer. Pour changer ses calories, supprime-le et recrée-le."
            </p>
            {move || error.get().map(|m| view! { <p class="error">{m}</p> })}
            <button class="btn primary block" type="submit">
                "Enregistrer"
            </button>
        </form>
    }
}

// ----- Aliment personnel (§3.7) -----------------------------------------------------------

/// Création manuelle : nom, marque, valeurs pour 100 g (ou 100 ml), portion facultative.
#[component]
fn CustomPane(mode: RwSignal<Mode>) -> impl IntoView {
    let name = RwSignal::new(String::new());
    let brand = RwSignal::new(String::new());
    let kcal = RwSignal::new(String::new());
    let protein = RwSignal::new(String::new());
    let carbs = RwSignal::new(String::new());
    let sugars = RwSignal::new(String::new());
    let fat = RwSignal::new(String::new());
    let sat_fat = RwSignal::new(String::new());
    let fiber = RwSignal::new(String::new());
    let salt = RwSignal::new(String::new());
    let portion = RwSignal::new(String::new());
    let liquid = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let input_ref = NodeRef::<leptos::html::Input>::new();

    Effect::new(move |_| {
        if let Some(el) = input_ref.get() {
            let _ = el.focus();
        }
    });

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let number = |signal: RwSignal<String>| parse_number(&signal.get_untracked());
        let input = CustomFoodInput {
            name: name.get_untracked(),
            brand: brand.get_untracked(),
            per_100: NutrientFacts {
                kcal: number(kcal),
                protein_g: number(protein),
                carbs_g: number(carbs),
                sugars_g: number(sugars),
                fat_g: number(fat),
                sat_fat_g: number(sat_fat),
                fiber_g: number(fiber),
                salt_g: number(salt),
            },
            portion_g: number(portion),
            liquid: liquid.get_untracked(),
        };
        spawn_local(async move {
            match api::create_custom_food(input).await {
                Ok(detail) => {
                    let _ = mode.try_set(Mode::Quantity(Box::new(detail), None));
                }
                Err(message) => {
                    let _ = error.try_set(Some(message));
                }
            }
        });
    };

    let field = move |label: &'static str, signal: RwSignal<String>, hint: &'static str| {
        view! {
            <label class="field">
                <span class="label">{label}</span>
                <input
                    class="input"
                    inputmode="decimal"
                    placeholder=hint
                    prop:value=move || signal.get()
                    on:input=move |ev| signal.set(event_target_value(&ev))
                />
            </label>
        }
    };

    view! {
        <form class="quantity" on:submit=submit>
            <label class="field">
                <span class="label">"Nom"</span>
                <input
                    class="input"
                    placeholder="Granola maison"
                    node_ref=input_ref
                    prop:value=move || name.get()
                    on:input=move |ev| name.set(event_target_value(&ev))
                />
            </label>
            <label class="field">
                <span class="label">"Marque (facultatif)"</span>
                <input
                    class="input"
                    prop:value=move || brand.get()
                    on:input=move |ev| brand.set(event_target_value(&ev))
                />
            </label>
            <p class="label">
                {move || if liquid.get() { "Valeurs pour 100 ml" } else { "Valeurs pour 100 g" }}
            </p>
            <div class="row">
                {field("Calories", kcal, "kcal")} {field("Protéines", protein, "g")}
                {field("Glucides", carbs, "g")} {field("dont sucres", sugars, "g")}
                {field("Lipides", fat, "g")} {field("dont saturés", sat_fat, "g")}
                {field("Fibres", fiber, "g")} {field("Sel", salt, "g")}
            </div>
            <div class="row">{field("Portion (facultatif)", portion, "g")}</div>
            <label class="check">
                <input
                    type="checkbox"
                    prop:checked=move || liquid.get()
                    on:change=move |ev| liquid.set(event_target_checked(&ev))
                />
                "Liquide"
            </label>
            <p class="muted">"Seules les calories sont obligatoires."</p>
            {move || error.get().map(|m| view! { <p class="error">{m}</p> })}
            <button class="btn primary block" type="submit">
                "Créer l'aliment"
            </button>
        </form>
    }
}
