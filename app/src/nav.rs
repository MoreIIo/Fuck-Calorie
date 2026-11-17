//! Navigation (§5.1) : barre en bas sur mobile, rail latéral à gauche sur PC.
//!
//! Ordre du cahier des charges : *Aujourd'hui · Progrès · + · Aliments · Réglages*.
//! L'écran *Aliments* n'existe pas encore : ses listes (récents, favoris, aliments
//! personnels, repas) sont pour l'instant dans la feuille d'ajout.

use leptos::prelude::*;

use crate::Shell;
use crate::icons::{IconChart, IconHome, IconPlus, IconSettings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Today,
    Progress,
    Settings,
}

#[component]
pub fn Nav() -> impl IntoView {
    let shell = expect_context::<Shell>();
    let item = move |page: Page, label: &'static str, icon: AnyView| {
        view! {
            <button
                class="nav-item"
                class:on=move || shell.page.get() == page
                aria-current=move || (shell.page.get() == page).then_some("page")
                on:click=move |_| {
                    // Revenir sur *Aujourd'hui* depuis un autre écran ramène au jour même.
                    if page == Page::Today && shell.page.get_untracked() != Page::Today {
                        shell.date.set(shell.today);
                    }
                    shell.page.set(page);
                }
            >
                {icon}
                <span>{label}</span>
            </button>
        }
    };
    // « + » ouvre la feuille d'ajout sur l'écran *Aujourd'hui* (le jour affiché, ou le jour
    // même depuis un autre écran).
    let add = move |_| {
        if shell.page.get_untracked() != Page::Today {
            shell.date.set(shell.today);
            shell.page.set(Page::Today);
        }
        shell.add_requested.set(true);
    };

    view! {
        <nav class="nav glass" aria-label="Navigation principale">
            {item(Page::Today, "Aujourd'hui", view! { <IconHome /> }.into_any())}
            {item(Page::Progress, "Progrès", view! { <IconChart /> }.into_any())}
            <button class="nav-add" aria-label="Ajouter (N)" on:click=add>
                <IconPlus />
            </button>
            {item(Page::Settings, "Réglages", view! { <IconSettings /> }.into_any())}
        </nav>
    }
}
