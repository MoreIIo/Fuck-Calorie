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
