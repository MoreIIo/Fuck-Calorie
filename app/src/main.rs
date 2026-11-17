//! Interface de Fuck Calorie (Leptos, rendu côté client).
//!
//! L'interface ne calcule rien : elle affiche ce que renvoie l'application (`fc-service`,
//! via les commandes Tauri) et lui transmet les saisies.

mod add;
mod api;
mod charts;
mod format;
mod icons;
mod nav;
mod onboarding;
mod progress;
mod settings;
mod today;

use chrono::NaiveDate;
use fc_core::api::{TodayView, WeekReport};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::nav::{Nav, Page};
use crate::onboarding::Onboarding;
use crate::progress::Progress;
use crate::settings::Settings;
use crate::today::Today;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

/// État partagé entre les écrans.
#[derive(Clone, Copy)]
pub struct Shell {
    /// Date du jour (fixée à l'ouverture de l'application).
    pub today: NaiveDate,
    /// Jour affiché par l'écran *Aujourd'hui*.
    pub date: RwSignal<NaiveDate>,
    /// Incrémenté après chaque modification pour recharger l'écran affiché.
    pub reload: RwSignal<u32>,
    pub page: RwSignal<Page>,
    /// Demande d'ouverture de la feuille d'ajout (bouton « + » de la navigation, §5.1).
    pub add_requested: RwSignal<bool>,
    /// Le scan de code-barres est disponible (mobile, §4.1).
    pub can_scan: RwSignal<bool>,
}

impl Shell {
    pub fn refresh(&self) {
        self.reload.update(|n| *n += 1);
    }

    /// Ouvre le journal du jour `day` (depuis un graphique de l'écran *Progrès*, §5.3).
    pub fn open_day(&self, day: NaiveDate) {
        self.date.set(day);
        self.page.set(Page::Today);
    }
}

#[component]
fn App() -> impl IntoView {
    let shell = Shell {
        today: api::local_today(),
        date: RwSignal::new(api::local_today()),
        reload: RwSignal::new(0),
        page: RwSignal::new(Page::Today),
        add_requested: RwSignal::new(false),
        can_scan: RwSignal::new(false),
    };
    provide_context(shell);
    if api::in_tauri() {
        spawn_local(async move { shell.can_scan.set(api::can_scan().await) });
    }
    // La navigation n'apparaît qu'une fois le profil renseigné.
    let has_profile = RwSignal::new(false);

    view! {
        <div class="backdrop" aria-hidden="true">
            <div class="light l1"></div>
            <div class="light l2"></div>
            <div class="light l3"></div>
            <div class="grain"></div>
        </div>
        <Show when=move || has_profile.get()>
            <Nav />
        </Show>
        <main class="app" class:with-nav=move || has_profile.get()>
            {move || {
                if !api::in_tauri() {
                    return view! { <OutsideApp /> }.into_any();
                }
                match shell.page.get() {
                    Page::Today => view! { <TodayPage has_profile=has_profile /> }.into_any(),
                    Page::Progress => view! { <Progress /> }.into_any(),
                    Page::Settings => view! { <Settings /> }.into_any(),
                }
            }}
            <p class="disclaimer muted">
                "Fuck Calorie n'est pas un dispositif médical. Demandez conseil à un professionnel de santé avant un changement important d'alimentation."
            </p>
        </main>
    }
}

/// État de chargement de l'écran *Aujourd'hui*.
#[derive(Clone)]
enum Screen {
    Loading,
    /// Premier lancement : profil à renseigner.
    NeedProfile,
    Ready(Box<TodayView>, Option<Box<WeekReport>>),
    Error(String),
}

#[component]
fn TodayPage(has_profile: RwSignal<bool>) -> impl IntoView {
    let shell = expect_context::<Shell>();
    let today = shell.today;
    let screen = RwSignal::new(Screen::Loading);

    Effect::new(move |_| {
        let day = shell.date.get();
        shell.reload.track();
        spawn_local(async move {
            // Le bilan d'abord : en début de semaine, il recalcule la dépense, donc la cible
            // affichée ensuite. S'il échoue, l'écran s'affiche sans lui.
            let report = api::week_report(today).await.ok().flatten().map(Box::new);
            let next = match api::today(day, api::local_time()).await {
                Ok(Some(view)) => Screen::Ready(Box::new(view), report),
                Ok(None) => Screen::NeedProfile,
                Err(error) => Screen::Error(error),
            };
            if matches!(next, Screen::Ready(..)) {
                has_profile.set(true);
            }
            // L'écran a pu être quitté pendant le chargement.
            let _ = screen.try_set(next);
        });
    });

    move || match screen.get() {
        Screen::Loading => view! { <p class="muted">"Chargement…"</p> }.into_any(),
        Screen::NeedProfile => view! { <Onboarding today=today reload=shell.reload /> }.into_any(),
        Screen::Ready(data, report) => view! {
            <Today
                data=*data
                report=report.map(|r| *r)
                today=today
                date=shell.date
                reload=shell.reload
                add_requested=shell.add_requested
            />
        }
        .into_any(),
        Screen::Error(error) => view! {
            <div class="glass banner">
                <p class="h2">"Quelque chose s'est mal passé"</p>
                <p class="error">{error}</p>
                <button class="btn" on:click=move |_| shell.refresh()>
                    "Réessayer"
                </button>
            </div>
        }
        .into_any(),
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
