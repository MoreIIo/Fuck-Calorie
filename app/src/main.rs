//! Interface de Fuck Calorie (Leptos, rendu côté client).
//!
//! L'interface ne calcule rien : elle affiche ce que renvoie l'application (`fc-service`,
//! via les commandes Tauri) et lui transmet les saisies.

mod add;
mod api;
mod format;
mod icons;
mod onboarding;
mod today;

use chrono::NaiveDate;
use fc_core::api::{TodayView, WeekReport};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::onboarding::Onboarding;
use crate::today::Today;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

/// État de chargement de l'écran.
#[derive(Clone)]
enum Screen {
    Loading,
    /// Ouverte dans un navigateur, hors de l'application.
    OutsideApp,
    /// Premier lancement : profil à renseigner.
    NeedProfile,
    Ready(Box<TodayView>, Option<Box<WeekReport>>),
    Error(String),
}

#[component]
fn App() -> impl IntoView {
    let today: NaiveDate = api::local_today();
    let date = RwSignal::new(today);
    // Incrémenté après chaque modification pour recharger l'écran.
    let reload = RwSignal::new(0_u32);
    let screen = RwSignal::new(Screen::Loading);

    Effect::new(move |_| {
        let day = date.get();
        reload.track();
        if !api::in_tauri() {
            screen.set(Screen::OutsideApp);
            return;
        }
        spawn_local(async move {
            // Le bilan d'abord : en début de semaine, il recalcule la dépense, donc la cible
            // affichée ensuite. S'il échoue, l'écran s'affiche sans lui.
            let report = api::week_report(today).await.ok().flatten().map(Box::new);
            let next = match api::today(day, api::local_time()).await {
                Ok(Some(view)) => Screen::Ready(Box::new(view), report),
                Ok(None) => Screen::NeedProfile,
                Err(error) => Screen::Error(error),
            };
            screen.set(next);
        });
    });

    view! {
        <div class="backdrop" aria-hidden="true">
            <div class="light l1"></div>
            <div class="light l2"></div>
            <div class="light l3"></div>
            <div class="grain"></div>
        </div>
        <main class="app">
            {move || match screen.get() {
                Screen::Loading => view! { <p class="muted">"Chargement…"</p> }.into_any(),
                Screen::OutsideApp => view! { <OutsideApp /> }.into_any(),
                Screen::NeedProfile => view! { <Onboarding today=today reload=reload /> }.into_any(),
                Screen::Ready(data, report) => {
                    view! {
                        <Today
                            data=*data
                            report=report.map(|r| *r)
                            today=today
                            date=date
                            reload=reload
                        />
                    }
                        .into_any()
                }
                Screen::Error(error) => {
                    view! {
                        <div class="glass banner">
                            <p class="h2">"Quelque chose s'est mal passé"</p>
                            <p class="error">{error}</p>
                            <button class="btn" on:click=move |_| reload.update(|n| *n += 1)>
                                "Réessayer"
                            </button>
                        </div>
                    }
                        .into_any()
                }
            }}
            <p class="disclaimer muted">
                "Fuck Calorie n'est pas un dispositif médical. Demandez conseil à un professionnel de santé avant un changement important d'alimentation."
            </p>
        </main>
    }
}

/// Affiché quand l'interface est ouverte dans un navigateur : la version web (stockage
/// dans le navigateur) n'existe pas encore.
#[component]
fn OutsideApp() -> impl IntoView {
    view! {
        <div class="glass banner">
            <p class="label">"Fuck Calorie"</p>
            <p class="h2">"Ouvre l'application de bureau"</p>
            <p class="muted">
                "Cette page est l'interface seule. Pour l'utiliser avec tes données, lance "
                <code>"cargo tauri dev"</code>
                " dans le dossier app. La version web viendra plus tard."
            </p>
        </div>
    }
}
