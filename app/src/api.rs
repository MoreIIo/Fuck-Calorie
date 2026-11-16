//! Appels à l'application (commandes Tauri, cf. `app/src-tauri/src/lib.rs`).

use chrono::{NaiveDate, NaiveTime};
use fc_core::api::{
    CustomFoodInput, EntryDetail, FoodDetail, FoodHit, LogFoodInput, ProfileInput, QuickAddInput,
    SavedMealView, TodayView, UpdateEntryInput, WeekReport,
};
use fc_core::profile::Profile;
use serde::Serialize;
use serde::de::DeserializeOwned;
use uuid::Uuid;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// `window.__TAURI__.core.invoke`, exposé par Tauri (`app.withGlobalTauri`).
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

/// L'interface tourne-t-elle dans l'application Tauri (et pas dans un simple navigateur) ?
pub fn in_tauri() -> bool {
    js_sys::Reflect::has(&js_sys::global(), &JsValue::from_str("__TAURI__")).unwrap_or(false)
}

async fn call<A: Serialize, T: DeserializeOwned>(cmd: &str, args: &A) -> Result<T, String> {
    // Objets JavaScript simples (et non des `Map`), comme Tauri les attend.
    let serializer = serde_wasm_bindgen::Serializer::json_compatible();
    let args = args.serialize(&serializer).map_err(|e| e.to_string())?;
    let value = tauri_invoke(cmd, args)
        .await
        .map_err(|e| e.as_string().unwrap_or_else(|| format!("{e:?}")))?;
    serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct DateTimeArgs {
    date: NaiveDate,
    time: NaiveTime,
}

/// Écran *Aujourd'hui* pour `date` ; `None` si le profil n'est pas encore renseigné.
pub async fn today(date: NaiveDate, time: NaiveTime) -> Result<Option<TodayView>, String> {
    call("today", &DateTimeArgs { date, time }).await
}

#[derive(Serialize)]
struct SaveProfileArgs {
    input: ProfileInput,
    today: NaiveDate,
}

pub async fn save_profile(input: ProfileInput, today: NaiveDate) -> Result<Profile, String> {
    call("save_profile", &SaveProfileArgs { input, today }).await
}

#[derive(Serialize)]
struct InputArgs<T> {
    input: T,
}

pub async fn quick_add(input: QuickAddInput) -> Result<(), String> {
    call("quick_add", &InputArgs { input }).await
}

#[derive(Serialize)]
struct IdArgs {
    id: Uuid,
}

pub async fn delete_entry(id: Uuid) -> Result<(), String> {
    call("delete_entry", &IdArgs { id }).await
}

#[derive(Serialize)]
struct WeighInArgs {
    date: NaiveDate,
    weight: f64,
}

pub async fn add_weigh_in(date: NaiveDate, weight: f64) -> Result<(), String> {
    call("add_weigh_in", &WeighInArgs { date, weight }).await
}

#[derive(Serialize)]
struct CompleteArgs {
    date: NaiveDate,
    complete: bool,
}

pub async fn set_day_complete(date: NaiveDate, complete: bool) -> Result<(), String> {
    call("set_day_complete", &CompleteArgs { date, complete }).await
}

#[derive(Serialize)]
struct QueryArgs {
    query: String,
}

/// Recherche locale (Ciqual, produits déjà consultés), disponible hors ligne (§3.3).
pub async fn search_foods(query: String) -> Result<Vec<FoodHit>, String> {
    call("search_foods", &QueryArgs { query }).await
}

/// Recherche en ligne sur Open Food Facts.
pub async fn search_off(query: String) -> Result<Vec<FoodHit>, String> {
    call("search_off", &QueryArgs { query }).await
}

#[derive(Serialize)]
struct NoArgs {}

/// Aliments récemment utilisés.
pub async fn recent_foods() -> Result<Vec<FoodHit>, String> {
    call("recent_foods", &NoArgs {}).await
}

/// Écran de quantité d'un aliment enregistré sur l'appareil.
pub async fn food_detail(id: Uuid) -> Result<FoodDetail, String> {
    call("food_detail", &IdArgs { id }).await
}

#[derive(Serialize)]
struct BarcodeArgs {
    barcode: String,
}

/// Écran de quantité d'un produit Open Food Facts, par code-barres.
pub async fn off_product(barcode: String) -> Result<FoodDetail, String> {
    call("off_product", &BarcodeArgs { barcode }).await
}

pub async fn log_food(input: LogFoodInput) -> Result<(), String> {
    call("log_food", &InputArgs { input }).await
}

/// Une ligne du journal, ouverte pour modification.
pub async fn entry_detail(id: Uuid) -> Result<EntryDetail, String> {
    call("entry_detail", &IdArgs { id }).await
}

pub async fn update_entry(input: UpdateEntryInput) -> Result<(), String> {
    call("update_entry", &InputArgs { input }).await
}

#[derive(Serialize)]
struct CopyArgs {
    from: NaiveDate,
    meal: Option<Uuid>,
    to: NaiveDate,
}

/// Copie un repas (ou toute la journée si `meal` est absent) du jour `from` vers `to`.
pub async fn copy_entries(
    from: NaiveDate,
    meal: Option<Uuid>,
    to: NaiveDate,
) -> Result<usize, String> {
    call("copy_entries", &CopyArgs { from, meal, to }).await
}

#[derive(Serialize)]
struct FavoriteArgs {
    id: Uuid,
    favorite: bool,
}

pub async fn set_favorite(id: Uuid, favorite: bool) -> Result<(), String> {
    call("set_favorite", &FavoriteArgs { id, favorite }).await
}

pub async fn favorite_foods() -> Result<Vec<FoodHit>, String> {
    call("favorite_foods", &NoArgs {}).await
}

/// Aliments personnels.
pub async fn custom_foods() -> Result<Vec<FoodHit>, String> {
    call("custom_foods", &NoArgs {}).await
}

pub async fn create_custom_food(input: CustomFoodInput) -> Result<FoodDetail, String> {
    call("create_custom_food", &InputArgs { input }).await
}

#[derive(Serialize)]
struct SaveMealArgs {
    date: NaiveDate,
    meal: Uuid,
    name: String,
}

/// Enregistre les aliments d'un repas du jour sous un nom.
pub async fn save_meal(date: NaiveDate, meal: Uuid, name: String) -> Result<(), String> {
    call("save_meal", &SaveMealArgs { date, meal, name }).await
}

pub async fn saved_meals() -> Result<Vec<SavedMealView>, String> {
    call("saved_meals", &NoArgs {}).await
}

#[derive(Serialize)]
struct AddSavedMealArgs {
    id: Uuid,
    date: NaiveDate,
    meal: Uuid,
}

pub async fn add_saved_meal(id: Uuid, date: NaiveDate, meal: Uuid) -> Result<usize, String> {
    call("add_saved_meal", &AddSavedMealArgs { id, date, meal }).await
}

pub async fn delete_saved_meal(id: Uuid) -> Result<(), String> {
    call("delete_saved_meal", &IdArgs { id }).await
}

#[derive(Serialize)]
struct TodayArgs {
    today: NaiveDate,
}

/// Bilan de la semaine écoulée. Le premier appel d'une nouvelle semaine recalcule la
/// dépense : à appeler avant [`today`].
pub async fn week_report(today: NaiveDate) -> Result<Option<WeekReport>, String> {
    call("week_report", &TodayArgs { today }).await
}

#[derive(Serialize)]
struct FrozenArgs {
    frozen: bool,
}

pub async fn set_target_frozen(frozen: bool) -> Result<(), String> {
    call("set_target_frozen", &FrozenArgs { frozen }).await
}

/// Date du jour, dans le fuseau horaire de l'appareil.
pub fn local_today() -> NaiveDate {
    let now = js_sys::Date::new_0();
    NaiveDate::from_ymd_opt(
        now.get_full_year() as i32,
        now.get_month() + 1,
        now.get_date(),
    )
    .unwrap_or_default()
}

/// Heure locale actuelle.
pub fn local_time() -> NaiveTime {
    let now = js_sys::Date::new_0();
    NaiveTime::from_hms_opt(now.get_hours(), now.get_minutes(), 0).unwrap_or_default()
}
