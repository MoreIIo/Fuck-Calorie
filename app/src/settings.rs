//! Écran *Réglages* : modifier le profil (§2.1), figer la cible (§2.3), à propos (§3.1,
//! §8.1, §8.10).

use fc_core::api::SettingsView;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::Shell;
use crate::api;
use crate::onboarding::ProfileForm;

#[component]
pub fn Settings() -> impl IntoView {
    let shell = expect_context::<Shell>();
    let data = RwSignal::new(None::<Result<SettingsView, String>>);
    let saved = RwSignal::new(false);

    Effect::new(move |_| {
        shell.reload.track();
        spawn_local(async move {
            let _ = data.try_set(Some(api::settings(shell.today).await));
        });
    });

    let on_saved = Callback::new(move |()| {
        saved.set(true);
        shell.refresh();
    });

    view! {
        <header class="page-head">
            <h1 class="h1">"Réglages"</h1>
        </header>
        {move || match data.get() {
            None => view! { <p class="muted">"Chargement…"</p> }.into_any(),
            Some(Err(message)) => view! { <p class="error">{message}</p> }.into_any(),
            Some(Ok(view)) => {
                let frozen = view.target_frozen;
                view! {
                    <div class="settings">
                        <section class="stack" aria-labelledby="profile-title">
                            <div>
                                <h2 class="h2" id="profile-title">
                                    "Profil"
                                </h2>
                                <p class="muted note">
                                    "Les changements s'appliquent à partir d'aujourd'hui : les jours passés gardent leur cible. Ton poids se met à jour avec les pesées."
                                </p>
                            </div>
                            <ProfileForm
                                initial=Some(view.profile)
                                today=shell.today
                                submit_label="Enregistrer"
                                on_saved=on_saved
                            />
                            <Show when=move || saved.get()>
                                <p class="accent" role="status">
                                    "Profil enregistré."
                                </p>
                            </Show>
                        </section>
                        <div class="stack">
                            <section class="glass card" aria-labelledby="target-title">
                                <p class="label" id="target-title">
                                    "Cible"
                                </p>
                                <p class="note">
                                    {if frozen {
                                        "Ta cible est figée : le recalcul hebdomadaire est calculé mais pas appliqué."
                                    } else {
                                        "Ta dépense estimée est recalculée chaque semaine d'après ce que tu manges et l'évolution de ton poids."
                                    }}
                                </p>
                                <button
                                    class="btn"
                                    on:click=move |_| {
                                        spawn_local(async move {
                                            if api::set_target_frozen(!frozen).await.is_ok() {
                                                shell.refresh();
                                            }
                                        });
                                    }
                                >
                                    {if frozen { "Libérer ma cible" } else { "Figer ma cible" }}
                                </button>
                            </section>
                            <About />
                        </div>
                    </div>
                }
                    .into_any()
            }
        }}
    }
}

/// À propos : sources des données (attributions obligatoires, §3.1), confidentialité (§8.1)
/// et licence (§8.10).
#[component]
fn About() -> impl IntoView {
    view! {
        <section class="glass card about" aria-labelledby="about-title">
            <p class="label" id="about-title">
                "À propos"
            </p>
            <p class="note">
                "Fuck Calorie " {env!("CARGO_PKG_VERSION")}
                " — gratuit et open source (licence MPL-2.0)."
            </p>
            <p class="note muted">
                "Tes données restent sur cet appareil. Seul appel réseau : la recherche de produits sur Open Food Facts, à qui seuls le texte recherché ou le code-barres sont envoyés."
            </p>
            <p class="note muted">
                "Source : Table Ciqual, ANSES (Licence Ouverte Etalab). Données Open Food Facts (licence ODbL)."
            </p>
            <p class="note muted">"github.com/MoreIIo/Fuck-Calorie"</p>
        </section>
    }
}
