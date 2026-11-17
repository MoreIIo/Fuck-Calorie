//! Icônes au trait fin (style Phosphor *Light*, §6.7), dessinées en SVG.

use leptos::prelude::*;

#[component]
pub fn IconPlus() -> impl IntoView {
    view! { <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 4v16M4 12h16" /></svg> }
}

#[component]
pub fn IconLeft() -> impl IntoView {
    view! { <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M15 5l-7 7 7 7" /></svg> }
}

#[component]
pub fn IconRight() -> impl IntoView {
    view! { <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 5l7 7-7 7" /></svg> }
}

#[component]
pub fn IconClose() -> impl IntoView {
    view! { <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18" /></svg> }
}

#[component]
pub fn IconHome() -> impl IntoView {
    view! {
        <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M4 10.5L12 4l8 6.5V20h-5.5v-5.5h-5V20H4z" />
        </svg>
    }
}

#[component]
pub fn IconChart() -> impl IntoView {
    view! {
        <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M4 4v16h16M8 16v-4M12 16V8M16 16v-6" />
        </svg>
    }
}

#[component]
pub fn IconSettings() -> impl IntoView {
    view! {
        <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M4 7h10M18 7h2M4 17h2M10 17h10M14 4.5v5M6 14.5v5" />
            <rect x="14" y="4.5" width="4" height="5" />
            <rect x="6" y="14.5" width="4" height="5" />
        </svg>
    }
}

/// Code-barres : traits verticaux dans un cadre ouvert.
#[component]
pub fn IconBarcode() -> impl IntoView {
    view! {
        <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M3 8V4h4M17 4h4v4M21 16v4h-4M7 20H3v-4M7 8v8M10 8v8M13 8v8M17 8v8" />
        </svg>
    }
}
