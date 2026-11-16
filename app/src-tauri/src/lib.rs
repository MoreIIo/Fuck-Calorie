//! Coque native de Fuck Calorie (Tauri 2).
//!
//! Ouvre la base locale dans le dossier de données de l'application et expose à
//! l'interface les opérations de `fc-service` sous forme de commandes. Aucune donnée ne
//! quitte l'appareil (§8.1), à une exception près : la recherche de produits interroge Open
//! Food Facts, à qui seuls le texte recherché ou le code-barres sont envoyés (§3.1).

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{NaiveDate, NaiveTime};
use fc_core::api::{
    CustomFoodInput, EntryDetail, FoodDetail, FoodHit, LogFoodInput, ProfileInput, QuickAddInput,
    SavedMealView, TodayView, UpdateEntryInput, WeekReport,
};
use fc_core::profile::Profile;
use fc_food_db::client::OffClient;
use fc_service::{Service, ServiceError};
use tauri::{Manager, State};
use uuid::Uuid;

/// Nom du fichier de la base, dans le dossier de données de l'application
/// (sous Windows : `%APPDATA%\io.github.moreiio.fuckcalorie\`).
const DB_FILE: &str = "fuck-calorie.db";

type AppState = Mutex<Service>;
/// Client Open Food Facts ; `None` s'il n'a pas pu être créé (l'application reste alors
/// utilisable, sans recherche en ligne : tout le reste fonctionne hors ligne, §8.2).
type OffState = Option<OffClient>;
type CmdResult<T> = Result<T, String>;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Exécute une opération sur le service. Les erreurs deviennent des messages affichables.
fn with_service<T>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&mut Service) -> fc_service::Result<T>,
) -> CmdResult<T> {
    let mut service = state
        .lock()
        .map_err(|_| "la base locale est indisponible".to_owned())?;
    f(&mut service).map_err(|e| e.to_string())
}

/// Écran *Aujourd'hui*. `None` tant que le profil n'est pas renseigné.
#[tauri::command]
fn today(
    state: State<'_, AppState>,
    date: NaiveDate,
    time: NaiveTime,
) -> CmdResult<Option<TodayView>> {
    with_service(&state, |s| match s.today(date, time) {
        Ok(view) => Ok(Some(view)),
        Err(ServiceError::NoProfile) => Ok(None),
        Err(e) => Err(e),
    })
}

#[tauri::command]
fn save_profile(
    state: State<'_, AppState>,
    input: ProfileInput,
    today: NaiveDate,
) -> CmdResult<Profile> {
    with_service(&state, |s| s.save_profile(input, today, now_ms()))
}

#[tauri::command]
fn quick_add(state: State<'_, AppState>, input: QuickAddInput) -> CmdResult<()> {
    with_service(&state, |s| s.quick_add(input, now_ms()))
}

#[tauri::command]
fn delete_entry(state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    with_service(&state, |s| s.delete_entry(id, now_ms()))
}

#[tauri::command]
fn add_weigh_in(state: State<'_, AppState>, date: NaiveDate, weight: f64) -> CmdResult<()> {
    with_service(&state, |s| s.add_weigh_in(date, weight, now_ms()))
}

#[tauri::command]
fn set_day_complete(state: State<'_, AppState>, date: NaiveDate, complete: bool) -> CmdResult<()> {
    with_service(&state, |s| s.set_day_complete(date, complete, now_ms()))
}

/// Bilan de la semaine écoulée (§5.4). Le premier appel d'une nouvelle semaine fait le
/// recalcul hebdomadaire de la dépense (§2.3) : l'interface l'appelle avant `today`.
#[tauri::command]
fn week_report(state: State<'_, AppState>, today: NaiveDate) -> CmdResult<Option<WeekReport>> {
    with_service(&state, |s| s.weekly_review(today))
}

/// Fige ou libère la cible calorique (§2.3).
#[tauri::command]
fn set_target_frozen(state: State<'_, AppState>, frozen: bool) -> CmdResult<()> {
    with_service(&state, |s| s.set_target_frozen(frozen))
}

// ----- Aliments ---------------------------------------------------------------------------

/// Recherche locale (Ciqual, produits déjà consultés, aliments personnels) : immédiate et
/// disponible hors ligne.
#[tauri::command]
fn search_foods(state: State<'_, AppState>, query: String) -> CmdResult<Vec<FoodHit>> {
    with_service(&state, |s| s.search_foods(&query))
}

#[tauri::command]
fn recent_foods(state: State<'_, AppState>) -> CmdResult<Vec<FoodHit>> {
    with_service(&state, |s| s.recent_foods())
}

#[tauri::command]
fn food_detail(state: State<'_, AppState>, id: Uuid) -> CmdResult<FoodDetail> {
    with_service(&state, |s| s.food_detail(id))
}

#[tauri::command]
fn log_food(state: State<'_, AppState>, input: LogFoodInput) -> CmdResult<()> {
    with_service(&state, |s| s.log_food(input, now_ms()))
}

// Les arguments des commandes portent des noms d'un seul mot : Tauri attend sinon du
// camelCase côté interface.

#[tauri::command]
fn entry_detail(state: State<'_, AppState>, id: Uuid) -> CmdResult<EntryDetail> {
    with_service(&state, |s| s.entry_detail(id))
}

#[tauri::command]
fn update_entry(state: State<'_, AppState>, input: UpdateEntryInput) -> CmdResult<()> {
    with_service(&state, |s| s.update_entry(input, now_ms()))
}

/// Copie un repas (`meal`) ou toute une journée (`meal` absent) du jour `from` vers `to`.
#[tauri::command]
fn copy_entries(
    state: State<'_, AppState>,
    from: NaiveDate,
    meal: Option<Uuid>,
    to: NaiveDate,
) -> CmdResult<usize> {
    with_service(&state, |s| s.copy_entries(from, meal, to, now_ms()))
}

#[tauri::command]
fn set_favorite(state: State<'_, AppState>, id: Uuid, favorite: bool) -> CmdResult<()> {
    with_service(&state, |s| s.set_favorite(id, favorite, now_ms()))
}

#[tauri::command]
fn favorite_foods(state: State<'_, AppState>) -> CmdResult<Vec<FoodHit>> {
    with_service(&state, |s| s.favorite_foods())
}

#[tauri::command]
fn custom_foods(state: State<'_, AppState>) -> CmdResult<Vec<FoodHit>> {
    with_service(&state, |s| s.custom_foods())
}

#[tauri::command]
fn create_custom_food(state: State<'_, AppState>, input: CustomFoodInput) -> CmdResult<FoodDetail> {
    with_service(&state, |s| s.create_custom_food(input, now_ms()))
}

#[tauri::command]
fn save_meal(
    state: State<'_, AppState>,
    date: NaiveDate,
    meal: Uuid,
    name: String,
) -> CmdResult<()> {
    with_service(&state, |s| s.save_meal(date, meal, &name, now_ms()))
}

#[tauri::command]
fn saved_meals(state: State<'_, AppState>) -> CmdResult<Vec<SavedMealView>> {
    with_service(&state, |s| s.saved_meals())
}

#[tauri::command]
fn add_saved_meal(
    state: State<'_, AppState>,
    id: Uuid,
    date: NaiveDate,
    meal: Uuid,
) -> CmdResult<usize> {
    with_service(&state, |s| s.add_saved_meal(id, date, meal, now_ms()))
}

#[tauri::command]
fn delete_saved_meal(state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    with_service(&state, |s| s.delete_saved_meal(id, now_ms()))
}

const OFFLINE_ONLY: &str = "la recherche en ligne est indisponible sur cet appareil";

fn online<'a>(off: &'a State<'_, OffState>) -> CmdResult<&'a OffClient> {
    off.as_ref().ok_or_else(|| OFFLINE_ONLY.to_owned())
}

/// Recherche en ligne sur Open Food Facts, dès 3 caractères. La base locale n'est jamais
/// verrouillée pendant l'attente du réseau.
#[tauri::command]
async fn search_off(
    state: State<'_, AppState>,
    off: State<'_, OffState>,
    query: String,
) -> CmdResult<Vec<FoodHit>> {
    if query.trim().chars().count() < 3 {
        return Ok(Vec::new());
    }
    let foods = online(&off)?
        .search(&query, now_ms())
        .await
        .map_err(|e| e.to_string())?;
    with_service(&state, |s| s.off_hits(&foods))
}

/// Ouvre un produit par son code-barres : fiche locale si elle est à jour, sinon Open Food
/// Facts (la fiche est alors copiée en local, §3.2). Sans réseau, une fiche locale ancienne
/// est utilisée telle quelle.
#[tauri::command]
async fn off_product(
    state: State<'_, AppState>,
    off: State<'_, OffState>,
    barcode: String,
) -> CmdResult<FoodDetail> {
    let local = with_service(&state, |s| s.local_product(&barcode, now_ms()))?;
    let stale = match local {
        Some((detail, true)) => return Ok(detail),
        Some((detail, false)) => Some(detail),
        None => None,
    };
    let fetched = match off.as_ref() {
        Some(client) => client
            .product(&barcode, now_ms())
            .await
            .map_err(|e| e.to_string()),
        None => Err(OFFLINE_ONLY.to_owned()),
    };
    match (fetched, stale) {
        (Ok(Some(food)), _) => with_service(&state, |s| s.cache_off_product(food, now_ms())),
        (_, Some(detail)) => Ok(detail),
        (Ok(None), None) => Err("produit introuvable sur Open Food Facts".to_owned()),
        (Err(error), None) => Err(error),
    }
}

/// Point d'entrée (bureau et, plus tard, mobile).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let service = Service::open(dir.join(DB_FILE))?;
            app.manage::<AppState>(Mutex::new(service));
            app.manage::<OffState>(OffClient::new(env!("CARGO_PKG_VERSION")).ok());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            today,
            save_profile,
            quick_add,
            delete_entry,
            add_weigh_in,
            set_day_complete,
            search_foods,
            recent_foods,
            food_detail,
            log_food,
            search_off,
            off_product,
            entry_detail,
            update_entry,
            copy_entries,
            set_favorite,
            favorite_foods,
            custom_foods,
            create_custom_food,
            save_meal,
            saved_meals,
            add_saved_meal,
            delete_saved_meal,
            week_report,
            set_target_frozen,
        ])
        .run(tauri::generate_context!())
        .expect("impossible de démarrer Fuck Calorie");
}
