//! # fc-service — les opérations de l'application
//!
//! Relie le cœur métier (`fc-core`) et la base locale (`fc-storage`) : c'est ce que
//! l'interface appelle, via les commandes Tauri. Chaque opération est une fonction simple,
//! testable sans interface.
//!
//! L'heure courante (`now_ms`) et la date du jour sont toujours passées par l'appelant :
//! c'est l'interface qui connaît le fuseau horaire de l'utilisatrice.

use std::path::Path;

use chrono::{Days, NaiveDate, NaiveTime};
use fc_core::activity::activity_kcal_on;
use fc_core::adaptive::{AdaptiveOutcome, DayRecord, WINDOW_DAYS, estimate_tdee, week_start};
use fc_core::api::{
    CustomFoodInput, EntryDetail, EntryView, FoodDetail, FoodHit, LogFoodInput, MealView,
    ProfileInput, ProgressView, QuickAddInput, SavedMealView, SettingsView, TodayView,
    UpdateEntryInput, WeekReport, WeightPoint, WeightView,
};
use fc_core::energy::initial_tdee_for;
use fc_core::food::{Food, FoodSource, Portion, QuantityError, is_barcode};
use fc_core::goal::{Goal, base_target, day_target_kcal, projected_goal_date};
use fc_core::hlc::{Hlc, HlcClock, SyncMeta};
use fc_core::journal::{
    DayStatus, LogEntry, MealSlot, SavedMeal, SavedMealItem, WeighIn, day_summary,
    default_meal_slots, slot_for_time, weigh_in_points,
};
use fc_core::macros::default_macro_targets;
use fc_core::nutrients::Nutrients;
use fc_core::profile::{Profile, ProfileError, ProfileRevision, profile_on};
use fc_core::progress::{ProgressPeriod, calorie_bars, period_means, weekly_buckets};
use fc_core::trend::{DEFAULT_ALPHA, TrendPoint, trend_change, trend_on, trend_series};
use fc_food_db::{ciqual, off};
use fc_storage::{Db, StorageError};
use uuid::Uuid;

/// Réglage : identifiant de cet appareil (pour l'horloge de synchronisation).
const DEVICE_ID: &str = "device_id";
/// Réglage : dernier horodatage HLC émis, pour reprendre l'horloge au redémarrage.
const LAST_HLC: &str = "last_hlc";
/// Réglage : historique des dépenses estimées par l'algorithme adaptatif, en JSON
/// `[[date d'effet, kcal/jour], …]`. Chaque estimation vaut à partir de sa date d'effet : les
/// jours passés gardent leur cible d'origine.
pub const TDEE_HISTORY: &str = "tdee_history";
/// Réglage : dernier bilan hebdomadaire calculé (JSON).
const WEEK_REPORT: &str = "week_report";
/// Réglage : la cible est figée à la main (§2.3).
const TARGET_FROZEN: &str = "target_frozen";
/// Réglage : édition de la table Ciqual importée dans la base (§3.1).
const CIQUAL_VERSION: &str = "ciqual_version";

/// Nombre maximal de résultats d'une recherche locale.
const SEARCH_LIMIT: usize = 40;
/// Nombre d'aliments de l'onglet *Récents*.
const RECENT_LIMIT: usize = 20;
/// Nombre maximal d'aliments des onglets *Favoris* et *Mes aliments*.
const LIST_LIMIT: usize = 200;
/// Quantité maximale acceptée pour une ligne du journal, en grammes.
const MAX_ENTRY_GRAMS: f64 = 10_000.0;

/// Noms des repas créés au premier lancement (§3.5).
const DEFAULT_MEALS: [&str; 4] = ["Petit-déjeuner", "Déjeuner", "Dîner", "Collation"];
/// Historique de pesées pris en compte pour le poids de tendance.
const WEIGHT_HISTORY_DAYS: u64 = 365;

/// Erreur renvoyée à l'interface. Le message est affichable tel quel.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("donnée interne illisible : {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Profile(#[from] ProfileError),
    #[error("le profil n'est pas encore renseigné")]
    NoProfile,
    #[error("{0}")]
    Quantity(#[from] QuantityError),
    #[error("{0}")]
    Invalid(&'static str),
    #[error("élément introuvable (il a peut-être déjà été supprimé)")]
    NotFound,
}

pub type Result<T> = std::result::Result<T, ServiceError>;

/// L'application, ouverte sur une base locale.
pub struct Service {
    db: Db,
    clock: HlcClock,
}

impl Service {
    /// Ouvre (ou crée) la base au chemin `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_db(Db::open(path)?)
    }

    /// Service sur une base en mémoire, pour les tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::from_db(Db::open_in_memory()?)
    }

    fn from_db(mut db: Db) -> Result<Self> {
        // Table Ciqual embarquée (§3.1) : importée au premier lancement, puis à chaque
        // nouvelle édition livrée avec l'application.
        if db.setting(CIQUAL_VERSION)?.as_deref() != Some(ciqual::VERSION) {
            db.import_reference_foods(&ciqual::foods())?;
            db.set_setting(CIQUAL_VERSION, ciqual::VERSION)?;
        }

        let device = match db
            .setting(DEVICE_ID)?
            .and_then(|id| Uuid::parse_str(&id).ok())
        {
            Some(id) => id,
            None => {
                let id = Uuid::now_v7();
                db.set_setting(DEVICE_ID, &id.to_string())?;
                id
            }
        };
        let clock = match db.setting(LAST_HLC)?.and_then(|h| h.parse::<Hlc>().ok()) {
            Some(last) if last.device == device => HlcClock::resume(last),
            _ => HlcClock::new(device),
        };
        let mut service = Self { db, clock };
        service.ensure_meal_slots()?;
        Ok(service)
    }

    /// Accès direct à la base (lectures avancées, tests).
    #[must_use]
    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Horodate une modification et mémorise l'horloge.
    fn tick(&mut self, now_ms: i64) -> Result<Hlc> {
        let hlc = self.clock.tick(u64::try_from(now_ms).unwrap_or(0));
        self.db.set_setting(LAST_HLC, &hlc.to_string())?;
        Ok(hlc)
    }

    fn ensure_meal_slots(&mut self) -> Result<()> {
        if self.db.meal_slots()?.is_empty() {
            // Création au premier lancement : l'horloge système suffit ici, l'ordre exact
            // n'a pas d'importance.
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
            let mut hlcs = Vec::new();
            for _ in DEFAULT_MEALS {
                hlcs.push(self.tick(now)?);
            }
            let mut hlcs = hlcs.into_iter();
            for slot in default_meal_slots(DEFAULT_MEALS, || hlcs.next().unwrap_or(Hlc::ZERO)) {
                self.db.upsert_meal_slot(&slot)?;
            }
        }
        Ok(())
    }

    // ----- Profil ---------------------------------------------------------------------------

    /// Profil en vigueur à la date `on`. Pour une date antérieure au premier profil, le
    /// premier profil s'applique.
    pub fn profile(&self, on: NaiveDate) -> Result<Option<Profile>> {
        let revisions = self.db.profile_revisions()?;
        Ok(profile_on(&revisions, on)
            .or_else(|| revisions.first().map(|r| &r.profile))
            .cloned())
    }

    /// Enregistre le profil à partir du jour `today` (§2.1 : historisé, les jours passés
    /// gardent leur cible). Le poids saisi est aussi enregistré comme pesée du jour s'il n'y
    /// en a pas encore.
    pub fn save_profile(
        &mut self,
        input: ProfileInput,
        today: NaiveDate,
        now_ms: i64,
    ) -> Result<Profile> {
        let previous = self.profile(today)?;
        let profile = Profile {
            sex: input.sex,
            birth_date: input.birth_date,
            height_cm: input.height_cm,
            weight_kg: input.weight_kg,
            goal_weight_kg: input.goal_weight_kg,
            goal: input.goal,
            activity_level: input.activity_level,
            week_start: input
                .week_start
                .or(previous.as_ref().map(|p| p.week_start))
                .unwrap_or(chrono::Weekday::Mon),
        };
        profile.validate(today)?;

        let hlc = self.tick(now_ms)?;
        // Une seule révision par jour : une deuxième modification le même jour la remplace.
        let same_day = self
            .db
            .profile_revisions()?
            .into_iter()
            .find(|r| r.effective_from == today);
        let revision = match same_day {
            Some(mut revision) => {
                revision.meta.touch(hlc);
                revision.profile = profile.clone();
                revision
            }
            None => ProfileRevision {
                meta: SyncMeta::new(hlc),
                effective_from: today,
                profile: profile.clone(),
            },
        };
        self.db.upsert_profile_revision(&revision)?;

        // Le poids saisi devient la pesée du jour au premier profil, ou s'il a été changé.
        // Une modification des réglages qui ne touche pas au poids n'ajoute pas de pesée.
        let weight_changed = previous
            .as_ref()
            .is_none_or(|p| (p.weight_kg - profile.weight_kg).abs() >= 0.05);
        if weight_changed && self.db.weigh_ins_between(today, today)?.is_empty() {
            self.add_weigh_in(today, profile.weight_kg, now_ms)?;
        }
        Ok(profile)
    }

    /// Écran *Réglages* : le profil en vigueur le jour `today` et les options de la cible.
    pub fn settings(&self, today: NaiveDate) -> Result<SettingsView> {
        Ok(SettingsView {
            profile: self.profile(today)?.ok_or(ServiceError::NoProfile)?,
            target_frozen: self.target_frozen()?,
        })
    }

    // ----- Journal --------------------------------------------------------------------------

    /// Ajout rapide (§3.6).
    pub fn quick_add(&mut self, input: QuickAddInput, now_ms: i64) -> Result<()> {
        let valid = |v: f64| v.is_finite() && (0.0..=20_000.0).contains(&v);
        if !valid(input.kcal) {
            return Err(ServiceError::Invalid(
                "les calories doivent être comprises entre 0 et 20 000",
            ));
        }
        let macros = [input.protein_g, input.carbs_g, input.fat_g];
        if macros
            .iter()
            .flatten()
            .any(|g| !g.is_finite() || !(0.0..=2_000.0).contains(g))
        {
            return Err(ServiceError::Invalid(
                "les macros doivent être comprises entre 0 et 2 000 g",
            ));
        }
        if !self
            .db
            .meal_slots()?
            .iter()
            .any(|s| s.meta.id == input.meal_slot_id)
        {
            return Err(ServiceError::NotFound);
        }
        let label = match input.label.trim() {
            "" => "Ajout rapide".to_owned(),
            label => label.chars().take(80).collect(),
        };
        let nutrients = Nutrients {
            kcal: input.kcal,
            protein_g: input.protein_g.unwrap_or(0.0),
            carbs_g: input.carbs_g.unwrap_or(0.0),
            fat_g: input.fat_g.unwrap_or(0.0),
            ..Nutrients::ZERO
        };
        let hlc = self.tick(now_ms)?;
        let entry = LogEntry::quick_add(
            SyncMeta::new(hlc),
            input.date,
            input.meal_slot_id,
            label,
            nutrients,
            now_ms,
        );
        self.db.upsert_log_entry(&entry)?;
        Ok(())
    }

    /// Ajoute un aliment au journal (§3.4). Les nutriments sont figés dans l'entrée :
    /// l'historique ne change pas si la fiche est corrigée plus tard.
    pub fn log_food(&mut self, input: LogFoodInput, now_ms: i64) -> Result<()> {
        let food = self.db.food(input.food_id)?.ok_or(ServiceError::NotFound)?;
        if !food.per_100.has_energy() {
            return Err(ServiceError::Invalid(
                "cette fiche n'indique pas les calories : passe par l'ajout rapide",
            ));
        }
        self.require_meal(input.meal_slot_id)?;
        if food.grams_for(input.quantity, &input.unit)? > MAX_ENTRY_GRAMS {
            return Err(ServiceError::Invalid(
                "la quantité ne peut pas dépasser 10 kg",
            ));
        }
        let hlc = self.tick(now_ms)?;
        let entry = LogEntry::from_food(
            SyncMeta::new(hlc),
            input.date,
            input.meal_slot_id,
            &food,
            input.quantity,
            input.unit,
            now_ms,
        )?;
        self.db.upsert_log_entry(&entry)?;
        Ok(())
    }

    fn require_meal(&self, meal_slot_id: Uuid) -> Result<()> {
        if self
            .db
            .meal_slots()?
            .iter()
            .any(|s| s.meta.id == meal_slot_id)
        {
            Ok(())
        } else {
            Err(ServiceError::NotFound)
        }
    }

    /// Une ligne du journal, ouverte pour modification (§3.5).
    pub fn entry_detail(&self, id: Uuid) -> Result<EntryDetail> {
        let entry = self.db.log_entry(id)?.ok_or(ServiceError::NotFound)?;
        let food = match entry.food_id.map(|food_id| self.db.food(food_id)) {
            Some(food) => food?,
            None => None,
        };
        let food = match food {
            Some(food) => {
                let mut detail = self.detail_of(food)?;
                // La quantité affichée est celle de la ligne, si elle est encore valable
                // pour la fiche (une portion peut avoir disparu depuis).
                if detail.food.grams_for(entry.quantity, &entry.unit).is_ok() {
                    detail.quantity = entry.quantity;
                    detail.unit = entry.unit.clone();
                }
                Some(detail)
            }
            None => None,
        };
        Ok(EntryDetail {
            id: entry.meta.id,
            meal_slot_id: entry.meal_slot_id,
            label: entry.label,
            kcal: entry.nutrients_snapshot.kcal,
            food,
        })
    }

    /// Modifie une ligne du journal (§3.5) : repas et, si elle est indiquée, quantité. Les
    /// nutriments sont alors recalculés à partir de la fiche actuelle de l'aliment.
    pub fn update_entry(&mut self, input: UpdateEntryInput, now_ms: i64) -> Result<()> {
        let mut entry = self.db.log_entry(input.id)?.ok_or(ServiceError::NotFound)?;
        self.require_meal(input.meal_slot_id)?;
        let quantity = input.quantity.zip(input.unit);
        let food = match (&quantity, entry.food_id) {
            (Some((amount, unit)), Some(food_id)) => {
                let food = self.db.food(food_id)?.ok_or(ServiceError::NotFound)?;
                if food.grams_for(*amount, unit)? > MAX_ENTRY_GRAMS {
                    return Err(ServiceError::Invalid(
                        "la quantité ne peut pas dépasser 10 kg",
                    ));
                }
                Some(food)
            }
            (Some(_), None) => {
                return Err(ServiceError::Invalid(
                    "un ajout rapide n'a pas de quantité : supprime-le et recrée-le",
                ));
            }
            (None, _) => None,
        };
        let hlc = self.tick(now_ms)?;
        match (food, quantity) {
            (Some(food), Some((amount, unit))) => entry.set_quantity(&food, amount, unit, hlc)?,
            _ => entry.meta.touch(hlc),
        }
        entry.meal_slot_id = input.meal_slot_id;
        self.db.upsert_log_entry(&entry)?;
        Ok(())
    }

    /// Copie les lignes d'un autre jour vers `to` (§3.6) : celles du repas `meal_slot_id`,
    /// ou toute la journée s'il est absent. Chaque ligne garde son repas. Renvoie le nombre
    /// de lignes copiées.
    pub fn copy_entries(
        &mut self,
        from: NaiveDate,
        meal_slot_id: Option<Uuid>,
        to: NaiveDate,
        now_ms: i64,
    ) -> Result<usize> {
        if from == to {
            return Err(ServiceError::Invalid(
                "la copie se fait depuis un autre jour",
            ));
        }
        let sources: Vec<LogEntry> = self
            .db
            .log_entries_on(from)?
            .into_iter()
            .filter(|e| meal_slot_id.is_none_or(|id| e.meal_slot_id == id))
            .collect();
        if sources.is_empty() {
            return Err(ServiceError::Invalid("il n'y a rien à copier ce jour-là"));
        }
        let mut copies = Vec::with_capacity(sources.len());
        for (index, source) in sources.iter().enumerate() {
            let hlc = self.tick(now_ms)?;
            // L'ordre de saisie est conservé.
            let created = now_ms.saturating_add(i64::try_from(index).unwrap_or(0));
            copies.push(source.copied_to(SyncMeta::new(hlc), to, source.meal_slot_id, created));
        }
        Ok(self.db.upsert_log_entries(&copies)?)
    }

    /// Supprime une ligne du journal (tombstone).
    pub fn delete_entry(&mut self, id: Uuid, now_ms: i64) -> Result<()> {
        let mut entry = self.db.log_entry(id)?.ok_or(ServiceError::NotFound)?;
        let hlc = self.tick(now_ms)?;
        entry.meta.delete(hlc);
        self.db.upsert_log_entry(&entry)?;
        Ok(())
    }

    /// Bouton *Journée terminée* (§3.5), réversible.
    pub fn set_day_complete(&mut self, date: NaiveDate, complete: bool, now_ms: i64) -> Result<()> {
        let note = self.db.day_status(date)?.and_then(|s| s.note);
        let hlc = self.tick(now_ms)?;
        self.db.set_day_status(&DayStatus {
            date,
            complete,
            note,
            hlc,
        })?;
        Ok(())
    }

    // ----- Aliments -------------------------------------------------------------------------

    /// Recherche locale (§3.3) : Ciqual, produits Open Food Facts déjà consultés, aliments
    /// personnels. Fonctionne hors ligne (§3.2).
    pub fn search_foods(&self, query: &str) -> Result<Vec<FoodHit>> {
        Ok(self
            .db
            .search_foods(query, SEARCH_LIMIT)?
            .iter()
            .map(|food| FoodHit::from_food(food, true))
            .collect())
    }

    /// Aliments récemment utilisés, du plus récent au plus ancien (onglet *Récents*, §3.3).
    pub fn recent_foods(&self) -> Result<Vec<FoodHit>> {
        Ok(self
            .db
            .recent_foods(RECENT_LIMIT)?
            .iter()
            .map(|food| FoodHit::from_food(food, true))
            .collect())
    }

    fn detail_of(&self, food: Food) -> Result<FoodDetail> {
        let last = self.db.last_quantity_for(food.meta.id)?;
        let (quantity, unit) = food.default_quantity(last);
        let units = food.units();
        let favorite = self.db.is_favorite(food.meta.id)?;
        Ok(FoodDetail {
            food,
            quantity,
            unit,
            units,
            favorite,
        })
    }

    /// Écran de quantité d'un aliment (§3.4) : fiche, quantité pré-remplie, unités.
    pub fn food_detail(&self, id: Uuid) -> Result<FoodDetail> {
        let food = self.db.food(id)?.ok_or(ServiceError::NotFound)?;
        self.detail_of(food)
    }

    /// Fiche locale d'un code-barres, si l'appareil en a une (§3.2), et son état :
    /// `true` si elle est à jour, `false` si c'est une fiche Open Food Facts de plus de
    /// 30 jours, à rafraîchir quand le réseau est là.
    pub fn local_product(&self, barcode: &str, now_ms: i64) -> Result<Option<(FoodDetail, bool)>> {
        let Some(food) = self.db.food_by_barcode(barcode)? else {
            return Ok(None);
        };
        let fresh = food.source != FoodSource::Off
            || now_ms.saturating_sub(food.updated_at_ms) < off::CACHE_MAX_AGE_MS;
        Ok(Some((self.detail_of(food)?, fresh)))
    }

    /// Copie en local une fiche reçue d'Open Food Facts (§3.2) et renvoie l'écran de
    /// quantité. Si un aliment personnel porte le même code-barres, c'est lui qui est
    /// renvoyé : il masque la fiche d'origine (§3.7).
    pub fn cache_off_product(&mut self, mut food: Food, now_ms: i64) -> Result<FoodDetail> {
        if food.source != FoodSource::Off {
            return Err(ServiceError::Invalid(
                "seules les fiches Open Food Facts vont dans le cache",
            ));
        }
        food.meta.touch(self.tick(now_ms)?);
        food.updated_at_ms = now_ms;
        self.db.upsert_food(&food)?;
        let shown = match &food.barcode {
            Some(barcode) => self.db.food_by_barcode(barcode)?.unwrap_or(food),
            None => food,
        };
        self.detail_of(shown)
    }

    /// Résultats d'une recherche Open Food Facts, prêts à afficher. Un produit que
    /// l'appareil connaît déjà est remplacé par sa fiche locale.
    pub fn off_hits(&self, foods: &[Food]) -> Result<Vec<FoodHit>> {
        foods
            .iter()
            .map(|food| {
                let local = match &food.barcode {
                    Some(barcode) => self.db.food_by_barcode(barcode)?,
                    None => None,
                };
                Ok(match local {
                    Some(local) => FoodHit::from_food(&local, true),
                    None => FoodHit::from_food(food, false),
                })
            })
            .collect()
    }

    /// Marque ou démarque un aliment comme favori (§3.6).
    pub fn set_favorite(&mut self, food_id: Uuid, favorite: bool, now_ms: i64) -> Result<()> {
        self.db.food(food_id)?.ok_or(ServiceError::NotFound)?;
        let hlc = self.tick(now_ms)?;
        self.db.set_favorite(food_id, favorite, hlc)?;
        Ok(())
    }

    /// Aliments favoris, par nom (onglet *Favoris*, §3.3).
    pub fn favorite_foods(&self) -> Result<Vec<FoodHit>> {
        Ok(self
            .db
            .favorite_foods(LIST_LIMIT)?
            .iter()
            .map(|food| FoodHit::from_food(food, true))
            .collect())
    }

    /// Aliments personnels, par nom (onglet *Mes aliments*, §3.3).
    pub fn custom_foods(&self) -> Result<Vec<FoodHit>> {
        Ok(self
            .db
            .foods_by_source(FoodSource::Custom, LIST_LIMIT)?
            .iter()
            .map(|food| FoodHit::from_food(food, true))
            .collect())
    }

    /// Crée un aliment personnel (§3.7) et renvoie son écran de quantité.
    pub fn create_custom_food(
        &mut self,
        input: CustomFoodInput,
        now_ms: i64,
    ) -> Result<FoodDetail> {
        let name = input.name.trim();
        if name.is_empty() || name.chars().count() > 120 {
            return Err(ServiceError::Invalid(
                "donne un nom à l'aliment (120 caractères au plus)",
            ));
        }
        let n = input.per_100;
        if !n.kcal.is_some_and(|kcal| (0.0..=1_000.0).contains(&kcal)) {
            return Err(ServiceError::Invalid(
                "indique les calories pour 100 g (entre 0 et 1 000)",
            ));
        }
        let grams = [
            n.protein_g,
            n.carbs_g,
            n.sugars_g,
            n.fat_g,
            n.sat_fat_g,
            n.fiber_g,
            n.salt_g,
        ];
        if grams.iter().flatten().any(|g| !(0.0..=100.0).contains(g)) {
            return Err(ServiceError::Invalid(
                "pour 100 g, chaque nutriment est compris entre 0 et 100 g",
            ));
        }
        let exceeds = |part: Option<f64>, whole: Option<f64>| matches!((part, whole), (Some(part), Some(whole)) if part > whole + 1e-9);
        if exceeds(n.sugars_g, n.carbs_g) || exceeds(n.sat_fat_g, n.fat_g) {
            return Err(ServiceError::Invalid(
                "les sucres ne peuvent pas dépasser les glucides, ni les saturés les lipides",
            ));
        }
        if input
            .portion_g
            .is_some_and(|g| !g.is_finite() || g <= 0.0 || g > 5_000.0)
        {
            return Err(ServiceError::Invalid(
                "le poids d'une portion est compris entre 0 et 5 000 g",
            ));
        }
        let barcode = input
            .barcode
            .as_deref()
            .map(|code| code.split_whitespace().collect::<String>())
            .filter(|code| !code.is_empty());
        if let Some(code) = &barcode {
            if !is_barcode(code) {
                return Err(ServiceError::Invalid(
                    "un code-barres compte de 8 à 14 chiffres",
                ));
            }
            let taken = self
                .db
                .food_by_barcode(code)?
                .is_some_and(|f| f.source == FoodSource::Custom);
            if taken {
                return Err(ServiceError::Invalid(
                    "un de tes aliments porte déjà ce code-barres",
                ));
            }
        }
        let brand = input.brand.trim();
        let hlc = self.tick(now_ms)?;
        let food = Food {
            meta: SyncMeta::new(hlc),
            source: FoodSource::Custom,
            source_ref: None,
            barcode,
            name: name.to_owned(),
            brand: (!brand.is_empty()).then(|| brand.chars().take(80).collect()),
            per_100: n,
            density: input.liquid.then_some(1.0),
            unit_weight_g: None,
            portions: input
                .portion_g
                .map(|grams| Portion {
                    label: "1 portion".to_owned(),
                    grams,
                })
                .into_iter()
                .collect(),
            updated_at_ms: now_ms,
        };
        self.db.upsert_food(&food)?;
        self.detail_of(food)
    }

    // ----- Repas enregistrés (§3.6) ---------------------------------------------------------

    /// Enregistre les aliments d'un repas du jour `date` sous le nom `name` (« Mon
    /// petit-déj »). Les ajouts rapides, qui n'ont pas de fiche, ne sont pas repris.
    pub fn save_meal(
        &mut self,
        date: NaiveDate,
        meal_slot_id: Uuid,
        name: &str,
        now_ms: i64,
    ) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 60 {
            return Err(ServiceError::Invalid(
                "donne un nom au repas (60 caractères au plus)",
            ));
        }
        let items: Vec<SavedMealItem> = self
            .db
            .log_entries_on(date)?
            .into_iter()
            .filter(|e| e.meal_slot_id == meal_slot_id)
            .filter_map(|e| {
                Some(SavedMealItem {
                    food_id: e.food_id?,
                    quantity: e.quantity,
                    unit: e.unit,
                })
            })
            .collect();
        if items.is_empty() {
            return Err(ServiceError::Invalid(
                "ce repas ne contient aucun aliment à enregistrer (les ajouts rapides ne sont pas repris)",
            ));
        }
        let hlc = self.tick(now_ms)?;
        self.db.upsert_saved_meal(&SavedMeal {
            meta: SyncMeta::new(hlc),
            name: name.to_owned(),
            items,
        })?;
        Ok(())
    }

    /// Les repas enregistrés, par nom.
    pub fn saved_meals(&self) -> Result<Vec<SavedMealView>> {
        let mut views = Vec::new();
        for meal in self.db.saved_meals()? {
            let mut kcal = 0.0;
            for item in &meal.items {
                if let Some(food) = self.db.food(item.food_id)? {
                    kcal += food
                        .nutrients_for(item.quantity, &item.unit)
                        .map_or(0.0, |n| n.kcal);
                }
            }
            views.push(SavedMealView {
                id: meal.meta.id,
                name: meal.name,
                kcal,
                items: meal.items.len(),
            });
        }
        Ok(views)
    }

    /// Ajoute un repas enregistré au journal, en une fois. Renvoie le nombre de lignes
    /// ajoutées (un aliment dont la fiche a disparu est ignoré).
    pub fn add_saved_meal(
        &mut self,
        id: Uuid,
        date: NaiveDate,
        meal_slot_id: Uuid,
        now_ms: i64,
    ) -> Result<usize> {
        let meal = self.db.saved_meal(id)?.ok_or(ServiceError::NotFound)?;
        self.require_meal(meal_slot_id)?;
        let mut entries = Vec::with_capacity(meal.items.len());
        for (index, item) in meal.items.iter().enumerate() {
            let Some(food) = self.db.food(item.food_id)? else {
                continue;
            };
            let hlc = self.tick(now_ms)?;
            let created = now_ms.saturating_add(i64::try_from(index).unwrap_or(0));
            if let Ok(entry) = LogEntry::from_food(
                SyncMeta::new(hlc),
                date,
                meal_slot_id,
                &food,
                item.quantity,
                item.unit.clone(),
                created,
            ) {
                entries.push(entry);
            }
        }
        if entries.is_empty() {
            return Err(ServiceError::Invalid(
                "les aliments de ce repas enregistré n'existent plus",
            ));
        }
        Ok(self.db.upsert_log_entries(&entries)?)
    }

    /// Supprime un repas enregistré (le journal n'est pas touché).
    pub fn delete_saved_meal(&mut self, id: Uuid, now_ms: i64) -> Result<()> {
        let mut meal = self.db.saved_meal(id)?.ok_or(ServiceError::NotFound)?;
        meal.meta.delete(self.tick(now_ms)?);
        self.db.upsert_saved_meal(&meal)?;
        Ok(())
    }

    // ----- Pesées ---------------------------------------------------------------------------

    /// Enregistre une pesée (§2.8).
    pub fn add_weigh_in(&mut self, date: NaiveDate, weight_kg: f64, now_ms: i64) -> Result<()> {
        if !weight_kg.is_finite() || !(30.0..=400.0).contains(&weight_kg) {
            return Err(ServiceError::Invalid(
                "le poids doit être compris entre 30 et 400 kg",
            ));
        }
        let hlc = self.tick(now_ms)?;
        let weigh_in = WeighIn {
            meta: SyncMeta::new(hlc),
            date,
            weight_kg: (weight_kg * 10.0).round() / 10.0,
            body_fat_pct: None,
        };
        self.db.upsert_weigh_in(&weigh_in)?;
        Ok(())
    }

    // ----- Recalcul hebdomadaire (§2.3) et bilan de la semaine (§5.4) ----------------------

    fn tdee_history(&self) -> Result<Vec<(NaiveDate, f64)>> {
        Ok(self
            .db
            .setting(TDEE_HISTORY)?
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default())
    }

    /// Dépense estimée par l'algorithme adaptatif en vigueur à la date `on`, s'il y en a une.
    fn adaptive_tdee_on(&self, on: NaiveDate) -> Result<Option<f64>> {
        Ok(self
            .tdee_history()?
            .into_iter()
            .filter(|(from, _)| *from <= on)
            .max_by_key(|(from, _)| *from)
            .map(|(_, tdee)| tdee))
    }

    /// La cible est-elle figée à la main ?
    pub fn target_frozen(&self) -> Result<bool> {
        Ok(self.db.setting(TARGET_FROZEN)?.as_deref() == Some("1"))
    }

    /// Fige ou libère la cible (§2.3). Figée, la dépense estimée ne change plus ; le bilan
    /// hebdomadaire continue d'indiquer ce que le recalcul aurait donné.
    pub fn set_target_frozen(&mut self, frozen: bool) -> Result<()> {
        if frozen {
            self.db.set_setting(TARGET_FROZEN, "1")?;
        } else {
            self.db.remove_setting(TARGET_FROZEN)?;
        }
        // Le bilan affiché reflète tout de suite le nouvel état.
        if let Some(mut report) = self.stored_week_report()? {
            report.frozen = frozen;
            self.db
                .set_setting(WEEK_REPORT, &serde_json::to_string(&report)?)?;
        }
        Ok(())
    }

    fn stored_week_report(&self) -> Result<Option<WeekReport>> {
        Ok(self
            .db
            .setting(WEEK_REPORT)?
            .and_then(|json| serde_json::from_str(&json).ok()))
    }

    /// Bilan de la semaine écoulée (§5.4). Au premier appel d'une nouvelle semaine, fait le
    /// **recalcul hebdomadaire** de la dépense (§2.3) sur les 21 jours qui précèdent, et
    /// applique la nouvelle estimation à partir du premier jour de la semaine. Les appels
    /// suivants de la même semaine renvoient le bilan déjà calculé.
    ///
    /// `None` tant que le profil n'existe pas ou que l'application a moins d'une semaine
    /// d'utilisation.
    pub fn weekly_review(&mut self, today: NaiveDate) -> Result<Option<WeekReport>> {
        let Some(profile) = self.profile(today)? else {
            return Ok(None);
        };
        let this_week = week_start(today, profile.week_start);
        let stored = self.stored_week_report()?;
        if stored.as_ref().is_some_and(|r| r.computed_for == this_week) {
            return Ok(stored);
        }
        let (Some(to), Some(from)) = (
            this_week.checked_sub_days(Days::new(1)),
            this_week.checked_sub_days(Days::new(7)),
        ) else {
            return Ok(None);
        };
        // Pas de bilan avant une première semaine complète d'utilisation.
        let started = self
            .db
            .profile_revisions()?
            .first()
            .map(|r| r.effective_from);
        if started.is_none_or(|start| start > from) {
            return Ok(None);
        }
        let window_start = to
            .checked_sub_days(Days::new(u64::from(WINDOW_DAYS - 1)))
            .unwrap_or(from);

        // Poids et dépense en vigueur à la fin de la semaine écoulée.
        let since = to
            .checked_sub_days(Days::new(WEIGHT_HISTORY_DAYS))
            .unwrap_or(to);
        let points = weigh_in_points(&self.db.weigh_ins_between(since, to)?);
        let series = trend_series(&points, to, DEFAULT_ALPHA);
        let weight_kg = series.last().map_or(profile.weight_kg, |p| p.trend_kg);
        let previous_tdee = match self.adaptive_tdee_on(to)? {
            Some(tdee) => tdee,
            None => initial_tdee_for(&profile, weight_kg, to),
        };
        let previous_target =
            base_target(previous_tdee, profile.goal, profile.sex, weight_kg, None);

        // Les 21 journées de la fenêtre.
        let totals = self.db.daily_totals_between(window_start, to)?;
        let activities = self.db.activities_between(window_start, to)?;
        let statuses = self.db.day_statuses_between(window_start, to)?;
        let records: Vec<DayRecord> = window_start
            .iter_days()
            .take_while(|date| *date <= to)
            .map(|date| {
                let activity_kcal = activity_kcal_on(&activities, date);
                DayRecord {
                    date,
                    eaten_kcal: totals
                        .iter()
                        .find(|(day, _)| *day == date)
                        .map_or(0.0, |(_, n)| n.kcal),
                    activity_kcal,
                    target_kcal: previous_target.kcal + activity_kcal,
                    marked_complete: statuses.iter().any(|s| s.date == date && s.complete),
                }
            })
            .collect();

        let outcome = estimate_tdee(to, &records, &points, previous_tdee);
        let frozen = self.target_frozen()?;
        let (tdee, computed, window_logged_days, window_weigh_in_days) = match outcome {
            AdaptiveOutcome::Estimated(e) => (
                if frozen { previous_tdee } else { e.tdee_base },
                Some(e.raw_tdee_base),
                e.logged_days,
                e.weigh_in_days,
            ),
            AdaptiveOutcome::InsufficientData {
                logged_days,
                weigh_in_days,
            } => (previous_tdee, None, logged_days, weigh_in_days),
        };
        if computed.is_some() && !frozen {
            let mut history = self.tdee_history()?;
            history.retain(|(date, _)| *date != this_week);
            history.push((this_week, tdee));
            history.sort_by_key(|(date, _)| *date);
            self.db
                .set_setting(TDEE_HISTORY, &serde_json::to_string(&history)?)?;
        }
        let target = base_target(tdee, profile.goal, profile.sex, weight_kg, None);

        // La semaine écoulée.
        let week: Vec<&DayRecord> = records
            .iter()
            .filter(|r| r.date >= from && r.counts_as_logged())
            .collect();
        let count = week.len() as f64;
        let mean = |value: fn(&DayRecord) -> f64| {
            (!week.is_empty()).then(|| week.iter().map(|r| value(r)).sum::<f64>() / count)
        };
        let report = WeekReport {
            computed_for: this_week,
            from,
            to,
            logged_days: u32::try_from(week.len()).unwrap_or(7),
            mean_eaten_kcal: mean(|r| r.eaten_kcal),
            mean_target_kcal: mean(|r| r.target_kcal),
            cumulative_gap_kcal: (!week.is_empty())
                .then(|| week.iter().map(|r| r.eaten_kcal - r.target_kcal).sum()),
            trend_change_kg: trend_on(&series, to)
                .zip(trend_on(&series, from))
                .map(|(end, start)| end - start),
            previous_tdee_kcal: previous_tdee,
            tdee_kcal: tdee,
            computed_tdee_kcal: computed,
            previous_target_kcal: previous_target.kcal,
            target_kcal: target.kcal,
            window_logged_days,
            window_weigh_in_days,
            frozen,
        };
        self.db
            .set_setting(WEEK_REPORT, &serde_json::to_string(&report)?)?;
        Ok(Some(report))
    }

    // ----- Écran « Progrès » (§5.3) ---------------------------------------------------------

    /// Graphiques de l'écran *Progrès* pour `period`, jusqu'à `today` inclus.
    ///
    /// La cible de chaque jour est recalculée comme sur l'écran *Aujourd'hui* : profil en
    /// vigueur ce jour-là (§2.1), dépense estimée à cette date (§2.3), poids de tendance du
    /// jour et activités enregistrées.
    pub fn progress(&self, period: ProgressPeriod, today: NaiveDate) -> Result<ProgressView> {
        let current = self.profile(today)?.ok_or(ServiceError::NoProfile)?;
        let revisions = self.db.profile_revisions()?;
        let first_use = revisions
            .iter()
            .map(|r| r.effective_from)
            .min()
            .unwrap_or(today);
        let from = period.start(today, first_use);
        let to = today;

        // Pesées : l'année qui précède la période sert à amorcer la tendance.
        let since = from
            .checked_sub_days(Days::new(WEIGHT_HISTORY_DAYS))
            .unwrap_or(from);
        let points = weigh_in_points(&self.db.weigh_ins_between(since, to)?);
        let series = trend_series(&points, to, DEFAULT_ALPHA);
        let in_period: Vec<&TrendPoint> = series.iter().filter(|p| p.date >= from).collect();
        let weigh_ins = in_period
            .iter()
            .filter_map(|p| p.raw_kg.map(|kg| WeightPoint { date: p.date, kg }))
            .collect();
        let trend: Vec<WeightPoint> = in_period
            .iter()
            .map(|p| WeightPoint {
                date: p.date,
                kg: p.trend_kg,
            })
            .collect();
        let trend_change_kg = match (trend.first(), trend.last()) {
            (Some(first), Some(last)) if trend.len() > 1 => Some(last.kg - first.kg),
            _ => None,
        };
        let today_weight = series.last().map_or(current.weight_kg, |p| p.trend_kg);

        // Les journées de la période.
        let totals = self.db.daily_totals_between(from, to)?;
        let activities = self.db.activities_between(from, to)?;
        let statuses = self.db.day_statuses_between(from, to)?;
        let history = self.tdee_history()?;
        let records: Vec<DayRecord> = from
            .iter_days()
            .take_while(|date| *date <= to)
            .map(|date| {
                // Avant le premier profil, le premier profil s'applique (comme `profile`).
                let profile = profile_on(&revisions, date)
                    .or_else(|| revisions.first().map(|r| &r.profile))
                    .unwrap_or(&current);
                let weight_kg = trend_on(&series, date).unwrap_or(profile.weight_kg);
                let tdee = history
                    .iter()
                    .filter(|(start, _)| *start <= date)
                    .max_by_key(|(start, _)| *start)
                    .map_or_else(|| initial_tdee_for(profile, weight_kg, date), |(_, t)| *t);
                let base = base_target(tdee, profile.goal, profile.sex, weight_kg, None);
                let activity_kcal = activity_kcal_on(&activities, date);
                DayRecord {
                    date,
                    eaten_kcal: totals
                        .iter()
                        .find(|(day, _)| *day == date)
                        .map_or(0.0, |(_, n)| n.kcal),
                    activity_kcal,
                    target_kcal: day_target_kcal(&base, activity_kcal, 1.0),
                    marked_complete: statuses.iter().any(|s| s.date == date && s.complete),
                }
            })
            .collect();
        let weekly = weekly_buckets(from, to);
        let means = period_means(&records);

        Ok(ProgressView {
            period,
            from,
            to,
            weigh_ins,
            trend,
            trend_change_kg,
            goal_weight_kg: current.goal_weight_kg,
            projected_goal_date: projection(&current, &series, today, today_weight),
            weekly,
            calories: calorie_bars(&records, weekly, current.week_start),
            mean_eaten_kcal: means.eaten_kcal,
            mean_target_kcal: means.target_kcal,
            logged_days: means.logged_days,
            days: u32::try_from(records.len()).unwrap_or(u32::MAX),
        })
    }

    // ----- Écran « Aujourd'hui » ------------------------------------------------------------

    /// Tout ce qu'affiche l'écran *Aujourd'hui* pour `date` (§5.2). `time` est l'heure locale
    /// actuelle, qui sert à pré-sélectionner le repas.
    pub fn today(&self, date: NaiveDate, time: NaiveTime) -> Result<TodayView> {
        let profile = self.profile(date)?.ok_or(ServiceError::NoProfile)?;

        // Poids : tendance des pesées jusqu'à ce jour, à défaut le poids du profil.
        let since = date
            .checked_sub_days(Days::new(WEIGHT_HISTORY_DAYS))
            .unwrap_or(date);
        let weigh_ins = self.db.weigh_ins_between(since, date)?;
        let points = weigh_in_points(&weigh_ins);
        let series = trend_series(&points, date, DEFAULT_ALPHA);
        let weight_kg = series.last().map_or(profile.weight_kg, |p| p.trend_kg);
        let weight = series.last().and_then(|last| {
            let latest = weigh_ins.iter().max_by_key(|w| (w.date, w.meta.hlc))?;
            Some(WeightView {
                trend_kg: last.trend_kg,
                last_kg: latest.weight_kg,
                last_date: latest.date,
                change_7d_kg: trend_change(&series, 7),
                weighed_today: latest.date == date,
            })
        });

        // Dépense : estimation adaptative si elle existe, sinon estimation initiale (§2.2).
        let adaptive = self.adaptive_tdee_on(date)?;
        let tdee = adaptive.unwrap_or_else(|| initial_tdee_for(&profile, weight_kg, date));
        let base = base_target(tdee, profile.goal, profile.sex, weight_kg, None);

        let entries = self.db.log_entries_on(date)?;
        let activities = self.db.activities_between(date, date)?;
        let summary = day_summary(date, &base, &entries, &activities, 1.0);

        let slots = self.db.meal_slots()?;
        let meals = meals_view(&slots, &entries);
        let complete = self.db.day_status(date)?.is_some_and(|s| s.complete);

        let projected_goal_date = projection(&profile, &series, date, weight_kg);

        Ok(TodayView {
            date,
            base_target_kcal: summary.base_target_kcal,
            activity_kcal: summary.activity_credit_kcal,
            target_kcal: summary.target_kcal,
            eaten: summary.eaten,
            remaining_kcal: summary.remaining_kcal,
            progress: summary.progress(),
            macros: default_macro_targets(summary.target_kcal, weight_kg, profile.goal),
            meals,
            suggested_meal_id: slot_for_time(&slots, time).map(|s| s.meta.id),
            weight,
            complete,
            tdee_kcal: tdee,
            tdee_adaptive: adaptive.is_some(),
            floor_applied: base.floor_applied,
            deficit_capped: base.deficit_capped,
            goal: profile.goal,
            goal_weight_kg: profile.goal_weight_kg,
            projected_goal_date,
        })
    }
}

/// Date estimée d'atteinte du poids objectif (§2.4) : rythme réel des 14 derniers jours, à
/// défaut le rythme choisi. `None` en maintien et en recomposition.
fn projection(
    profile: &Profile,
    series: &[TrendPoint],
    date: NaiveDate,
    weight_kg: f64,
) -> Option<NaiveDate> {
    match profile.goal {
        Goal::Lose { .. } | Goal::Gain { .. } => {
            let rate = trend_change(series, 14)
                .map(|change| change / 2.0)
                .or_else(|| profile.goal.target_kg_per_week());
            rate.and_then(|r| projected_goal_date(date, weight_kg, profile.goal_weight_kg, r))
        }
        Goal::Maintain | Goal::Recomp => None,
    }
}

/// Regroupe les entrées par repas. Une entrée dont le repas a été supprimé est rattachée au
/// dernier repas, pour ne jamais disparaître des totaux affichés.
fn meals_view(slots: &[MealSlot], entries: &[LogEntry]) -> Vec<MealView> {
    let mut meals: Vec<MealView> = slots
        .iter()
        .map(|s| MealView {
            id: s.meta.id,
            name: s.name.clone(),
            kcal: 0.0,
            entries: Vec::new(),
        })
        .collect();
    for entry in entries {
        let index = meals
            .iter()
            .position(|m| m.id == entry.meal_slot_id)
            .unwrap_or(meals.len().saturating_sub(1));
        let Some(meal) = meals.get_mut(index) else {
            continue;
        };
        let n = &entry.nutrients_snapshot;
        meal.kcal += n.kcal;
        meal.entries.push(EntryView {
            id: entry.meta.id,
            label: entry.label.clone(),
            kcal: n.kcal,
            protein_g: n.protein_g,
            carbs_g: n.carbs_g,
            fat_g: n.fat_g,
            quick_add: entry.is_quick_add(),
        });
    }
    meals
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_core::profile::{ActivityLevel, Sex};

    fn date(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    fn noon() -> NaiveTime {
        NaiveTime::from_hms_opt(12, 30, 0).unwrap()
    }

    fn profile_input() -> ProfileInput {
        ProfileInput {
            sex: Sex::Female,
            birth_date: NaiveDate::from_ymd_opt(1990, 6, 15).unwrap(),
            height_cm: 170.0,
            weight_kg: 70.0,
            goal_weight_kg: 62.0,
            goal: Goal::Lose { kg_per_week: 0.5 },
            activity_level: ActivityLevel::Sedentary,
            week_start: None,
        }
    }

    fn ready() -> Service {
        let mut service = Service::open_in_memory().unwrap();
        service
            .save_profile(profile_input(), date(15), 1_000)
            .unwrap();
        service
    }

    #[test]
    fn first_launch_creates_meals_and_requires_profile() {
        let service = Service::open_in_memory().unwrap();
        let names: Vec<String> = service
            .db()
            .meal_slots()
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, DEFAULT_MEALS);
        assert!(service.profile(date(15)).unwrap().is_none());
        assert!(matches!(
            service.today(date(15), noon()),
            Err(ServiceError::NoProfile)
        ));
    }

    #[test]
    fn reopening_keeps_device_clock_and_meals() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fc.db");
        let first_hlc = {
            let mut service = Service::open(&path).unwrap();
            service
                .save_profile(profile_input(), date(15), 5_000)
                .unwrap();
            service.db().setting(LAST_HLC).unwrap().unwrap()
        };
        let mut service = Service::open(&path).unwrap();
        assert_eq!(
            service.db().meal_slots().unwrap().len(),
            4,
            "pas de doublon de repas"
        );
        // L'horloge reprend après le dernier horodatage, même si l'heure système recule.
        let hlc = service.tick(10).unwrap();
        assert!(hlc > first_hlc.parse::<Hlc>().unwrap());
    }

    #[test]
    fn initial_target_from_profile() {
        let service = ready();
        let view = service.today(date(15), noon()).unwrap();
        // Femme, 36 ans, 170 cm, 70 kg, sédentaire : BMR 1 421,5 × 1,2 = 1 705,8 ;
        // −550 kcal pour 0,5 kg/semaine → 1 155,8, remonté au plancher de 1 200.
        assert!((view.tdee_kcal - 1_705.8).abs() < 1e-9);
        assert!(!view.tdee_adaptive);
        assert!(view.floor_applied);
        assert_eq!(view.target_kcal, 1_200.0);
        assert_eq!(view.remaining_kcal, 1_200.0);
        assert_eq!(view.meals.len(), 4);
        // Le poids du profil a été enregistré comme première pesée.
        let weight = view.weight.unwrap();
        assert_eq!(
            (weight.trend_kg, weight.last_kg, weight.weighed_today),
            (70.0, 70.0, true)
        );
        // 8 kg à 0,5 kg/semaine → 16 semaines.
        assert_eq!(
            view.projected_goal_date,
            NaiveDate::from_ymd_opt(2027, 3, 7)
        );
        // Repas proposé à 12 h 30 : le déjeuner.
        let lunch = view.meals.iter().find(|m| m.name == "Déjeuner").unwrap();
        assert_eq!(view.suggested_meal_id, Some(lunch.id));
    }

    #[test]
    fn invalid_profile_is_refused() {
        let mut service = Service::open_in_memory().unwrap();
        let input = ProfileInput {
            height_cm: 17.0,
            ..profile_input()
        };
        let err = service.save_profile(input, date(15), 1).unwrap_err();
        assert!(err.to_string().contains("taille"));
        assert!(service.profile(date(15)).unwrap().is_none());
    }

    #[test]
    fn profile_changes_are_dated() {
        let mut service = ready();
        let maintain = ProfileInput {
            goal: Goal::Maintain,
            ..profile_input()
        };
        service
            .save_profile(maintain.clone(), date(20), 2_000)
            .unwrap();
        service.save_profile(maintain, date(20), 3_000).unwrap(); // même jour : remplacée
        assert_eq!(service.db().profile_revisions().unwrap().len(), 2);
        assert_eq!(
            service.profile(date(18)).unwrap().unwrap().goal,
            Goal::Lose { kg_per_week: 0.5 }
        );
        assert_eq!(
            service.profile(date(21)).unwrap().unwrap().goal,
            Goal::Maintain
        );
        // Avant le premier profil : le premier profil s'applique.
        assert!(service.profile(date(1)).unwrap().is_some());
    }

    #[test]
    fn quick_add_and_delete() {
        let mut service = ready();
        let view = service.today(date(15), noon()).unwrap();
        let lunch = view.suggested_meal_id.unwrap();
        let add = |label: &str, kcal| QuickAddInput {
            date: date(15),
            meal_slot_id: lunch,
            label: label.to_owned(),
            kcal,
            protein_g: Some(30.0),
            carbs_g: None,
            fat_g: None,
        };
        service
            .quick_add(add("Resto italien", 850.0), 2_000)
            .unwrap();
        service.quick_add(add("   ", 150.0), 3_000).unwrap();

        let view = service.today(date(15), noon()).unwrap();
        assert_eq!(view.eaten.kcal, 1_000.0);
        assert_eq!(view.eaten.protein_g, 60.0);
        assert_eq!(view.remaining_kcal, 200.0);
        let meal = view.meals.iter().find(|m| m.id == lunch).unwrap();
        assert_eq!(meal.kcal, 1_000.0);
        let labels: Vec<&str> = meal.entries.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, ["Resto italien", "Ajout rapide"]);
        assert!(meal.entries.iter().all(|e| e.quick_add));

        service.delete_entry(meal.entries[0].id, 4_000).unwrap();
        assert!(matches!(
            service.delete_entry(meal.entries[0].id, 5_000),
            Err(ServiceError::NotFound)
        ));
        let view = service.today(date(15), noon()).unwrap();
        assert_eq!(view.eaten.kcal, 150.0);
        // Un autre jour n'est pas touché.
        assert_eq!(service.today(date(16), noon()).unwrap().eaten.kcal, 0.0);
    }

    #[test]
    fn quick_add_validation() {
        let mut service = ready();
        let lunch = service
            .today(date(15), noon())
            .unwrap()
            .suggested_meal_id
            .unwrap();
        let input = QuickAddInput {
            date: date(15),
            meal_slot_id: lunch,
            label: String::new(),
            kcal: -5.0,
            protein_g: None,
            carbs_g: None,
            fat_g: None,
        };
        assert!(matches!(
            service.quick_add(input.clone(), 1),
            Err(ServiceError::Invalid(_))
        ));
        let nan_macro = QuickAddInput {
            kcal: 100.0,
            fat_g: Some(f64::NAN),
            ..input.clone()
        };
        assert!(matches!(
            service.quick_add(nan_macro, 1),
            Err(ServiceError::Invalid(_))
        ));
        let unknown_meal = QuickAddInput {
            kcal: 100.0,
            meal_slot_id: Uuid::nil(),
            ..input
        };
        assert!(matches!(
            service.quick_add(unknown_meal, 1),
            Err(ServiceError::NotFound)
        ));
    }

    #[test]
    fn weigh_ins_drive_the_trend() {
        let mut service = ready();
        assert!(service.add_weigh_in(date(16), 700.0, 2_000).is_err());
        service.add_weigh_in(date(16), 69.04, 2_000).unwrap(); // arrondi à 0,1 kg
        let view = service.today(date(16), noon()).unwrap();
        let weight = view.weight.unwrap();
        assert_eq!(weight.last_kg, 69.0);
        assert!((weight.trend_kg - 69.9).abs() < 1e-9); // 70 + 0,1 × (69 − 70)
        assert!(weight.weighed_today);
        assert!(
            !service
                .today(date(17), noon())
                .unwrap()
                .weight
                .unwrap()
                .weighed_today
        );
    }

    #[test]
    fn day_complete_toggle() {
        let mut service = ready();
        assert!(!service.today(date(15), noon()).unwrap().complete);
        service.set_day_complete(date(15), true, 2_000).unwrap();
        assert!(service.today(date(15), noon()).unwrap().complete);
        service.set_day_complete(date(15), false, 3_000).unwrap();
        assert!(!service.today(date(15), noon()).unwrap().complete);
    }

    #[test]
    fn adaptive_estimate_replaces_initial_one() {
        let service = ready();
        service
            .db()
            .set_setting(TDEE_HISTORY, r#"[["2026-11-09",2300.0]]"#)
            .unwrap();
        // Avant la date d'effet, l'estimation initiale s'applique encore.
        assert!(!service.today(date(8), noon()).unwrap().tdee_adaptive);
        let view = service.today(date(15), noon()).unwrap();
        assert!(view.tdee_adaptive);
        assert_eq!(view.target_kcal, 1_750.0);
        assert!(!view.floor_applied);
    }

    // ----- Aliments -------------------------------------------------------------------------

    use fc_core::food::Unit;

    fn off_product(barcode: &str, name: &str) -> Food {
        let json = format!(
            r#"{{"code":"{barcode}","product_name":"{name}","brands":"Marque",
                "serving_quantity":125,"serving_quantity_unit":"g",
                "nutriments":{{"energy-kcal_100g":61,"proteins_100g":3.8,
                               "carbohydrates_100g":5.3,"fat_100g":2.7}}}}"#
        );
        off::food_from_product(&serde_json::from_str(&json).unwrap(), 0).unwrap()
    }

    #[test]
    fn ciqual_is_available_from_first_launch() {
        let service = ready();
        let hits = service.search_foods("huile d'olive vierge").unwrap();
        assert_eq!(hits[0].name, "Huile d'olive vierge extra");
        assert_eq!(hits[0].source, FoodSource::Ciqual);
        assert_eq!(hits[0].kcal_100, Some(899.0));
        assert!(hits[0].id.is_some());
        // Faute de frappe tolérée (§3.3).
        let hits = service.search_foods("camenbert").unwrap();
        assert!(!hits.is_empty());
        assert!(
            hits.iter()
                .all(|h| h.name.to_lowercase().contains("camembert"))
        );
        assert!(service.search_foods("a").unwrap().is_empty());
    }

    #[test]
    fn ciqual_is_imported_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fc.db");
        let id = {
            let service = Service::open(&path).unwrap();
            service.search_foods("pastis").unwrap()[0].id.unwrap()
        };
        // Une fiche modifiée dans la base survit à la réouverture : pas de réimport tant que
        // l'édition de la table n'a pas changé.
        {
            let mut service = Service::open(&path).unwrap();
            let mut food = service.db.food(id).unwrap().unwrap();
            food.meta.touch(service.tick(1_000).unwrap());
            food.name = "Pastis modifié".into();
            service.db.upsert_food(&food).unwrap();
        }
        let service = Service::open(&path).unwrap();
        assert_eq!(service.db.food(id).unwrap().unwrap().name, "Pastis modifié");
    }

    #[test]
    fn log_a_food_and_prefill_next_time() {
        let mut service = ready();
        let lunch = service
            .today(date(15), noon())
            .unwrap()
            .suggested_meal_id
            .unwrap();
        let oil = service.search_foods("huile d'olive vierge").unwrap()[0]
            .id
            .unwrap();

        // Première fois : 100 g proposés, en grammes seulement (pas de portion connue).
        let detail = service.food_detail(oil).unwrap();
        assert_eq!((detail.quantity, detail.unit.clone()), (100.0, Unit::Gram));
        assert_eq!(detail.units, [Unit::Gram]);

        // 10 g d'huile à 899 kcal / 100 g = 89,9 kcal, 9,99 g de lipides.
        service
            .log_food(
                LogFoodInput {
                    date: date(15),
                    meal_slot_id: lunch,
                    food_id: oil,
                    quantity: 10.0,
                    unit: Unit::Gram,
                },
                1_000,
            )
            .unwrap();
        let view = service.today(date(15), noon()).unwrap();
        assert!((view.eaten.kcal - 89.9).abs() < 1e-9);
        assert!((view.eaten.fat_g - 9.99).abs() < 1e-9);
        let meal = view.meals.iter().find(|m| m.id == lunch).unwrap();
        assert_eq!(meal.entries[0].label, "Huile d'olive vierge extra");
        assert!(!meal.entries[0].quick_add);

        // La fois suivante : la dernière quantité est pré-remplie, et l'aliment est dans
        // les récents.
        assert_eq!(service.food_detail(oil).unwrap().quantity, 10.0);
        assert_eq!(service.recent_foods().unwrap()[0].id, Some(oil));
    }

    #[test]
    fn log_food_validation() {
        let mut service = ready();
        let lunch = service
            .today(date(15), noon())
            .unwrap()
            .suggested_meal_id
            .unwrap();
        let oil = service.search_foods("huile d'olive vierge").unwrap()[0]
            .id
            .unwrap();
        let input = |food_id, meal_slot_id, quantity, unit| LogFoodInput {
            date: date(15),
            meal_slot_id,
            food_id,
            quantity,
            unit,
        };
        let cases = [
            input(oil, lunch, 0.0, Unit::Gram),
            input(oil, lunch, f64::NAN, Unit::Gram),
            input(oil, lunch, 10_001.0, Unit::Gram),
            input(oil, lunch, 1.0, Unit::Piece),
            input(oil, lunch, 1.0, Unit::Portion("1 bol".into())),
            input(Uuid::from_u128(1), lunch, 10.0, Unit::Gram),
            input(oil, Uuid::from_u128(1), 10.0, Unit::Gram),
        ];
        for case in cases {
            assert!(service.log_food(case.clone(), 1_000).is_err(), "{case:?}");
        }
        assert_eq!(service.today(date(15), noon()).unwrap().eaten.kcal, 0.0);
    }

    #[test]
    fn off_products_are_cached_then_found_offline() {
        let mut service = ready();
        let barcode = "3033490004743";
        assert_eq!(service.local_product(barcode, 0).unwrap(), None);
        let remote = [off_product(barcode, "Yaourt nature")];

        // Avant consultation : résultat distant, sans fiche locale.
        let hits = service.off_hits(&remote).unwrap();
        assert_eq!(hits[0].id, None);
        assert_eq!(hits[0].barcode.as_deref(), Some(barcode));
        assert_eq!(hits[0].portion, Some(("1 portion".to_owned(), 125.0)));

        // Consultation : la fiche est copiée en local (§3.2), portion par défaut proposée.
        let detail = service.cache_off_product(remote[0].clone(), 5_000).unwrap();
        assert_eq!(detail.food.updated_at_ms, 5_000);
        assert_eq!(
            (detail.quantity, detail.unit.clone()),
            (1.0, Unit::Portion("1 portion".into()))
        );
        // Hors ligne : la recherche locale et le code-barres la retrouvent.
        assert_eq!(
            service.search_foods("yaourt nature marque").unwrap()[0].id,
            Some(detail.food.meta.id)
        );
        assert_eq!(
            service.off_hits(&remote).unwrap()[0].id,
            Some(detail.food.meta.id)
        );
        let (cached, fresh) = service.local_product(barcode, 6_000).unwrap().unwrap();
        assert_eq!(cached.food, detail.food);
        assert!(fresh);
        // Après 30 jours, la fiche est à rafraîchir (§3.2).
        let later = 5_000 + off::CACHE_MAX_AGE_MS;
        assert!(!service.local_product(barcode, later).unwrap().unwrap().1);

        // Rafraîchissement : même identifiant, données et date mises à jour.
        let mut updated = off_product(barcode, "Yaourt nature bio");
        updated.per_100.kcal = Some(65.0);
        let refreshed = service.cache_off_product(updated, later).unwrap();
        assert_eq!(refreshed.food.meta.id, detail.food.meta.id);
        assert_eq!(refreshed.food.name, "Yaourt nature bio");
        assert_eq!(refreshed.food.per_100.kcal, Some(65.0));
        assert!(service.local_product(barcode, later).unwrap().unwrap().1);
    }

    #[test]
    fn foods_without_energy_cannot_be_logged() {
        let mut service = ready();
        let lunch = service
            .today(date(15), noon())
            .unwrap()
            .suggested_meal_id
            .unwrap();
        let mut food = off_product("12345678", "Mystère");
        food.per_100.kcal = None;
        let detail = service.cache_off_product(food, 1_000).unwrap();
        let error = service
            .log_food(
                LogFoodInput {
                    date: date(15),
                    meal_slot_id: lunch,
                    food_id: detail.food.meta.id,
                    quantity: 100.0,
                    unit: Unit::Gram,
                },
                2_000,
            )
            .unwrap_err();
        assert!(error.to_string().contains("ajout rapide"));
    }

    // ----- Journal complet ------------------------------------------------------------------

    use fc_core::nutrients::NutrientFacts;

    /// Service prêt, avec le repas de midi, celui du soir et l'identifiant de l'huile d'olive.
    fn kitchen() -> (Service, Uuid, Uuid, Uuid) {
        let service = ready();
        let view = service.today(date(15), noon()).unwrap();
        let lunch = view.suggested_meal_id.unwrap();
        let dinner = view.meals.iter().find(|m| m.name == "Dîner").unwrap().id;
        let oil = service.search_foods("huile d'olive vierge").unwrap()[0]
            .id
            .unwrap();
        (service, lunch, dinner, oil)
    }

    fn log(service: &mut Service, day: u32, meal: Uuid, food: Uuid, grams: f64, now_ms: i64) {
        service
            .log_food(
                LogFoodInput {
                    date: date(day),
                    meal_slot_id: meal,
                    food_id: food,
                    quantity: grams,
                    unit: Unit::Gram,
                },
                now_ms,
            )
            .unwrap();
    }

    fn entries_of(service: &Service, day: u32, meal: Uuid) -> Vec<EntryView> {
        let view = service.today(date(day), noon()).unwrap();
        view.meals
            .into_iter()
            .find(|m| m.id == meal)
            .unwrap()
            .entries
    }

    #[test]
    fn edit_an_entry() {
        let (mut service, lunch, dinner, oil) = kitchen();
        log(&mut service, 15, lunch, oil, 10.0, 1_000);
        let id = entries_of(&service, 15, lunch)[0].id;

        // L'écran de modification reprend la quantité de la ligne.
        let detail = service.entry_detail(id).unwrap();
        assert_eq!(detail.meal_slot_id, lunch);
        assert_eq!(detail.food.as_ref().unwrap().quantity, 10.0);

        // 20 g au lieu de 10 g, déplacés au dîner : 179,8 kcal.
        service
            .update_entry(
                UpdateEntryInput {
                    id,
                    meal_slot_id: dinner,
                    quantity: Some(20.0),
                    unit: Some(Unit::Gram),
                },
                2_000,
            )
            .unwrap();
        assert!(entries_of(&service, 15, lunch).is_empty());
        let moved = entries_of(&service, 15, dinner);
        assert_eq!(moved[0].id, id);
        assert!((moved[0].kcal - 179.8).abs() < 1e-9);

        // Déplacement seul : la quantité ne bouge pas.
        service
            .update_entry(
                UpdateEntryInput {
                    id,
                    meal_slot_id: lunch,
                    quantity: None,
                    unit: None,
                },
                3_000,
            )
            .unwrap();
        assert!((entries_of(&service, 15, lunch)[0].kcal - 179.8).abs() < 1e-9);

        // Quantité invalide ou repas inconnu : refusés, rien ne change.
        for (meal, quantity) in [(lunch, 0.0), (lunch, 20_000.0), (Uuid::from_u128(9), 5.0)] {
            let input = UpdateEntryInput {
                id,
                meal_slot_id: meal,
                quantity: Some(quantity),
                unit: Some(Unit::Gram),
            };
            assert!(service.update_entry(input, 4_000).is_err());
        }
        assert!((entries_of(&service, 15, lunch)[0].kcal - 179.8).abs() < 1e-9);
    }

    #[test]
    fn quick_adds_can_move_but_have_no_quantity() {
        let (mut service, lunch, dinner, _) = kitchen();
        service
            .quick_add(
                QuickAddInput {
                    date: date(15),
                    meal_slot_id: lunch,
                    label: "Resto".into(),
                    kcal: 800.0,
                    protein_g: None,
                    carbs_g: None,
                    fat_g: None,
                },
                1_000,
            )
            .unwrap();
        let id = entries_of(&service, 15, lunch)[0].id;
        assert_eq!(service.entry_detail(id).unwrap().food, None);
        let mut input = UpdateEntryInput {
            id,
            meal_slot_id: dinner,
            quantity: Some(2.0),
            unit: Some(Unit::Gram),
        };
        assert!(service.update_entry(input.clone(), 2_000).is_err());
        input.quantity = None;
        input.unit = None;
        service.update_entry(input, 2_000).unwrap();
        assert_eq!(entries_of(&service, 15, dinner)[0].kcal, 800.0);
    }

    #[test]
    fn copy_a_meal_or_a_whole_day() {
        let (mut service, lunch, dinner, oil) = kitchen();
        log(&mut service, 14, lunch, oil, 10.0, 1_000);
        log(&mut service, 14, lunch, oil, 5.0, 2_000);
        log(&mut service, 14, dinner, oil, 20.0, 3_000);

        // Un seul repas : les deux lignes du déjeuner d'hier, dans le même ordre.
        assert_eq!(
            service
                .copy_entries(date(14), Some(lunch), date(15), 10_000)
                .unwrap(),
            2
        );
        let copied = entries_of(&service, 15, lunch);
        assert!((copied[0].kcal - 89.9).abs() < 1e-9);
        assert!((copied[1].kcal - 44.95).abs() < 1e-9);
        assert!(entries_of(&service, 15, dinner).is_empty());
        // La journée d'origine est intacte, et les copies sont indépendantes.
        service.delete_entry(copied[0].id, 11_000).unwrap();
        assert_eq!(entries_of(&service, 14, lunch).len(), 2);

        // Toute la journée, chaque ligne dans son repas.
        assert_eq!(
            service
                .copy_entries(date(14), None, date(16), 12_000)
                .unwrap(),
            3
        );
        assert_eq!(entries_of(&service, 16, lunch).len(), 2);
        assert_eq!(entries_of(&service, 16, dinner).len(), 1);

        // Rien à copier, ou même jour : refusé.
        assert!(
            service
                .copy_entries(date(10), None, date(15), 13_000)
                .is_err()
        );
        assert!(
            service
                .copy_entries(date(14), None, date(14), 13_000)
                .is_err()
        );
    }

    #[test]
    fn favorites() {
        let (mut service, _, _, oil) = kitchen();
        assert!(service.favorite_foods().unwrap().is_empty());
        assert!(!service.food_detail(oil).unwrap().favorite);
        service.set_favorite(oil, true, 1_000).unwrap();
        assert!(service.food_detail(oil).unwrap().favorite);
        assert_eq!(service.favorite_foods().unwrap()[0].id, Some(oil));
        service.set_favorite(oil, false, 2_000).unwrap();
        assert!(service.favorite_foods().unwrap().is_empty());
        assert!(
            service
                .set_favorite(Uuid::from_u128(1), true, 3_000)
                .is_err()
        );
    }

    fn granola() -> CustomFoodInput {
        CustomFoodInput {
            name: "  Granola maison ".into(),
            brand: String::new(),
            per_100: NutrientFacts {
                kcal: Some(450.0),
                protein_g: Some(10.0),
                carbs_g: Some(55.0),
                sugars_g: Some(18.0),
                fat_g: Some(20.0),
                ..Default::default()
            },
            portion_g: Some(40.0),
            liquid: false,
            barcode: None,
        }
    }

    #[test]
    fn custom_foods() {
        let (mut service, lunch, _, _) = kitchen();
        let detail = service.create_custom_food(granola(), 1_000).unwrap();
        assert_eq!(detail.food.name, "Granola maison");
        assert_eq!(detail.food.source, FoodSource::Custom);
        assert_eq!(detail.food.brand, None);
        // La portion est proposée d'emblée : 40 g = 180 kcal.
        assert_eq!(
            (detail.quantity, detail.unit.clone()),
            (1.0, Unit::Portion("1 portion".into()))
        );
        service
            .log_food(
                LogFoodInput {
                    date: date(15),
                    meal_slot_id: lunch,
                    food_id: detail.food.meta.id,
                    quantity: detail.quantity,
                    unit: detail.unit,
                },
                2_000,
            )
            .unwrap();
        assert!((entries_of(&service, 15, lunch)[0].kcal - 180.0).abs() < 1e-9);

        // Il apparaît dans *Mes aliments* et en tête de la recherche (avant Ciqual).
        assert_eq!(service.custom_foods().unwrap().len(), 1);
        assert_eq!(
            service.search_foods("granola").unwrap()[0].id,
            Some(detail.food.meta.id)
        );
    }

    #[test]
    fn custom_food_validation() {
        let (mut service, ..) = kitchen();
        let mut cases = Vec::new();
        let mut case = |edit: &dyn Fn(&mut CustomFoodInput)| {
            let mut input = granola();
            edit(&mut input);
            cases.push(input);
        };
        case(&|i| i.name = "   ".into());
        case(&|i| i.per_100.kcal = None);
        case(&|i| i.per_100.kcal = Some(1_200.0));
        case(&|i| i.per_100.protein_g = Some(120.0));
        case(&|i| i.per_100.fat_g = Some(-1.0));
        case(&|i| i.per_100.sugars_g = Some(60.0)); // plus que les glucides
        case(&|i| i.per_100.sat_fat_g = Some(25.0)); // plus que les lipides
        case(&|i| i.portion_g = Some(0.0));
        for input in cases {
            assert!(
                service.create_custom_food(input.clone(), 1_000).is_err(),
                "{input:?}"
            );
        }
        assert!(service.custom_foods().unwrap().is_empty());
    }

    #[test]
    fn saved_meals() {
        let (mut service, lunch, dinner, oil) = kitchen();
        // Un repas vide, ou fait seulement d'ajouts rapides, ne s'enregistre pas.
        assert!(service.save_meal(date(15), lunch, "Vide", 500).is_err());
        log(&mut service, 15, lunch, oil, 10.0, 1_000);
        log(&mut service, 15, lunch, oil, 5.0, 2_000);
        assert!(service.save_meal(date(15), lunch, "  ", 3_000).is_err());
        service
            .save_meal(date(15), lunch, "Salade du midi", 3_000)
            .unwrap();

        // 15 g d'huile = 134,85 kcal.
        let saved = service.saved_meals().unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(
            (saved[0].name.as_str(), saved[0].items),
            ("Salade du midi", 2)
        );
        assert!((saved[0].kcal - 134.85).abs() < 1e-9);

        // Ajout en une fois, à un autre repas d'un autre jour.
        assert_eq!(
            service
                .add_saved_meal(saved[0].id, date(16), dinner, 4_000)
                .unwrap(),
            2
        );
        let added = entries_of(&service, 16, dinner);
        assert!((added[0].kcal - 89.9).abs() < 1e-9);
        assert!((added[1].kcal - 44.95).abs() < 1e-9);

        // Suppression : la liste se vide, le journal ne bouge pas.
        service.delete_saved_meal(saved[0].id, 5_000).unwrap();
        assert!(service.saved_meals().unwrap().is_empty());
        assert_eq!(entries_of(&service, 16, dinner).len(), 2);
        assert!(
            service
                .add_saved_meal(saved[0].id, date(16), dinner, 6_000)
                .is_err()
        );
    }

    // ----- Recalcul hebdomadaire et bilan de la semaine -------------------------------------

    fn october(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
    }

    /// Service dont le profil date du lundi 12 octobre 2026, avec `days` jours de suivi :
    /// 1 500 kcal mangées par jour, journée marquée terminée, pesée quotidienne d'une
    /// personne dont la vraie dépense est de 2 000 kcal (elle perd 500 ÷ 7 700 kg par jour).
    fn tracked(days: u64) -> Service {
        let mut service = Service::open_in_memory().unwrap();
        let start = october(12);
        service.save_profile(profile_input(), start, 0).unwrap();
        let lunch = service
            .today(start, noon())
            .unwrap()
            .suggested_meal_id
            .unwrap();
        for i in 0..days {
            let day = start + Days::new(i);
            let now = 1_000 * (i64::try_from(i).unwrap() + 1);
            service
                .quick_add(
                    QuickAddInput {
                        date: day,
                        meal_slot_id: lunch,
                        label: String::new(),
                        kcal: 1_500.0,
                        protein_g: None,
                        carbs_g: None,
                        fat_g: None,
                    },
                    now,
                )
                .unwrap();
            service.set_day_complete(day, true, now).unwrap();
            let weight = 70.0 - 500.0 / 7_700.0 * i as f64;
            service.add_weigh_in(day, weight, now + 1).unwrap();
        }
        service
    }

    #[test]
    fn weekly_review_recalculates_once_per_week() {
        // 4 semaines de suivi, du lundi 12 octobre au dimanche 8 novembre.
        let mut service = tracked(28);
        let monday = date(9);
        // Dépense de départ, telle qu'estimée le dernier jour de la semaine écoulée.
        let before = service.today(date(8), noon()).unwrap();
        assert!(!before.tdee_adaptive);

        let report = service.weekly_review(monday).unwrap().unwrap();
        assert_eq!(report.computed_for, monday);
        assert_eq!((report.from, report.to), (date(2), date(8)));
        // Semaine écoulée : 7 jours journalisés à 1 500 kcal, pour une cible au plancher de
        // 1 200 kcal : 300 kcal de trop par jour, 2 100 sur la semaine.
        assert_eq!(report.logged_days, 7);
        assert_eq!(report.mean_eaten_kcal, Some(1_500.0));
        assert_eq!(report.mean_target_kcal, Some(1_200.0));
        assert_eq!(report.cumulative_gap_kcal, Some(2_100.0));
        assert!(report.trend_change_kg.unwrap() < -0.3);
        // Fenêtre de 21 jours complète ; la dépense calculée retrouve les 2 000 kcal réels
        // (aux arrondis des pesées près), mais ne monte que de 100 kcal par semaine.
        assert_eq!(
            (report.window_logged_days, report.window_weigh_in_days),
            (21, 21)
        );
        let computed = report.computed_tdee_kcal.unwrap();
        assert!((computed - 2_000.0).abs() < 60.0, "{computed}");
        assert_eq!(report.previous_tdee_kcal, before.tdee_kcal);
        assert!((report.tdee_kcal - report.previous_tdee_kcal - 100.0).abs() < 1e-9);
        assert!(!report.frozen);

        // La nouvelle dépense vaut à partir du lundi ; la veille garde l'ancienne.
        let after = service.today(monday, noon()).unwrap();
        assert!(after.tdee_adaptive);
        assert_eq!(after.tdee_kcal, report.tdee_kcal);
        assert_eq!(after.base_target_kcal, report.target_kcal);
        assert!(!service.today(date(8), noon()).unwrap().tdee_adaptive);

        // Le même bilan toute la semaine, sans second recalcul.
        assert_eq!(service.weekly_review(date(11)).unwrap().unwrap(), report);
        assert_eq!(
            service.today(date(11), noon()).unwrap().tdee_kcal,
            report.tdee_kcal
        );

        // Semaine suivante : nouveau recalcul, encore + 100 kcal.
        let next = service.weekly_review(date(16)).unwrap().unwrap();
        assert_eq!(next.computed_for, date(16));
        assert_eq!(next.previous_tdee_kcal, report.tdee_kcal);
        assert!((next.tdee_kcal - report.tdee_kcal - 100.0).abs() < 1e-9);
        // Semaine du 9 au 15 novembre : rien n'a été journalisé.
        assert_eq!(next.logged_days, 0);
        assert_eq!(next.mean_eaten_kcal, None);
        assert_eq!(next.cumulative_gap_kcal, None);
    }

    #[test]
    fn weekly_review_without_enough_data_keeps_the_estimate() {
        // 10 jours de suivi seulement dans la fenêtre : il en faut 14.
        let mut service = tracked(10);
        let report = service.weekly_review(date(2)).unwrap().unwrap();
        assert_eq!(report.computed_tdee_kcal, None);
        assert_eq!(report.tdee_kcal, report.previous_tdee_kcal);
        assert_eq!(report.target_kcal, report.previous_target_kcal);
        assert_eq!(report.window_logged_days, 10);
        assert!(!service.today(date(2), noon()).unwrap().tdee_adaptive);
    }

    #[test]
    fn no_weekly_review_during_the_first_week() {
        let mut service = Service::open_in_memory().unwrap();
        assert_eq!(service.weekly_review(date(9)).unwrap(), None); // pas de profil
        // Profil créé le jeudi 5 novembre : pas de semaine complète avant le lundi 9.
        service.save_profile(profile_input(), date(5), 0).unwrap();
        assert_eq!(service.weekly_review(date(9)).unwrap(), None);
        // Le lundi suivant, la semaine du 9 au 15 est complète.
        assert!(service.weekly_review(date(16)).unwrap().is_some());
    }

    #[test]
    fn frozen_target_is_not_changed_by_the_review() {
        let mut service = tracked(28);
        service.set_target_frozen(true).unwrap();
        let report = service.weekly_review(date(9)).unwrap().unwrap();
        // Le recalcul est montré, pas appliqué.
        assert!(report.frozen);
        assert!(report.computed_tdee_kcal.is_some());
        assert_eq!(report.tdee_kcal, report.previous_tdee_kcal);
        assert!(!service.today(date(9), noon()).unwrap().tdee_adaptive);

        // Cible libérée : le bilan affiché suit, et le recalcul reprend la semaine suivante.
        service.set_target_frozen(false).unwrap();
        assert!(!service.weekly_review(date(9)).unwrap().unwrap().frozen);
        let next = service.weekly_review(date(16)).unwrap().unwrap();
        assert!((next.tdee_kcal - next.previous_tdee_kcal - 100.0).abs() < 1e-9);
    }

    fn quick(service: &mut Service, day: NaiveDate, kcal: f64) {
        let meal = service.db().meal_slots().unwrap()[0].meta.id;
        let input = QuickAddInput {
            date: day,
            meal_slot_id: meal,
            label: String::new(),
            kcal,
            protein_g: None,
            carbs_g: None,
            fat_g: None,
        };
        service.quick_add(input, 10_000).unwrap();
    }

    #[test]
    fn settings_edit_keeps_weight_and_week_start() {
        let mut service = ready();
        let settings = service.settings(date(15)).unwrap();
        assert_eq!(settings.profile.week_start, chrono::Weekday::Mon);
        assert!(!settings.target_frozen);

        // Réglages : taille et premier jour de la semaine changés, poids inchangé.
        let edit = ProfileInput {
            height_cm: 168.0,
            week_start: Some(chrono::Weekday::Sun),
            ..profile_input()
        };
        service.save_profile(edit.clone(), date(17), 2_000).unwrap();
        assert!(
            service
                .db()
                .weigh_ins_between(date(16), date(30))
                .unwrap()
                .is_empty(),
            "pas de pesée ajoutée quand le poids ne change pas"
        );
        let profile = service.settings(date(17)).unwrap().profile;
        assert_eq!(
            (profile.height_cm, profile.week_start),
            (168.0, chrono::Weekday::Sun)
        );
        // Les jours passés gardent l'ancien profil (§2.1).
        assert_eq!(service.profile(date(16)).unwrap().unwrap().height_cm, 170.0);

        // Sans premier jour indiqué, il est conservé.
        let again = ProfileInput {
            week_start: None,
            ..edit
        };
        service
            .save_profile(again.clone(), date(18), 3_000)
            .unwrap();
        assert_eq!(
            service.profile(date(18)).unwrap().unwrap().week_start,
            chrono::Weekday::Sun
        );

        // Un nouveau poids devient la pesée du jour.
        let heavier = ProfileInput {
            weight_kg: 68.0,
            ..again
        };
        service.save_profile(heavier, date(19), 4_000).unwrap();
        let weigh_ins = service.db().weigh_ins_between(date(19), date(19)).unwrap();
        assert_eq!(weigh_ins.len(), 1);
        assert_eq!(weigh_ins[0].weight_kg, 68.0);

        service.set_target_frozen(true).unwrap();
        assert!(service.settings(date(19)).unwrap().target_frozen);
        assert!(matches!(
            Service::open_in_memory().unwrap().settings(date(15)),
            Err(ServiceError::NoProfile)
        ));
    }

    #[test]
    fn progress_follows_profile_history() {
        let mut service = ready();
        quick(&mut service, date(15), 1_500.0);
        quick(&mut service, date(16), 1_500.0);
        quick(&mut service, date(17), 300.0);
        let maintain = ProfileInput {
            goal: Goal::Maintain,
            ..profile_input()
        };
        service.save_profile(maintain, date(17), 2_000).unwrap();

        let view = service.progress(ProgressPeriod::Week, date(17)).unwrap();
        assert_eq!((view.from, view.to, view.days), (date(11), date(17), 7));
        assert!(!view.weekly);
        assert_eq!(view.calories.len(), 7);
        // Avant le premier jour : rien de mangé, cible du premier profil (plancher).
        assert_eq!(view.calories[0].eaten_kcal, None);
        assert_eq!(view.calories[0].target_kcal, 1_200.0);
        // Le 15 et le 16 : perte de poids, cible au plancher de 1 200 kcal.
        assert_eq!(view.calories[4].eaten_kcal, Some(1_500.0));
        assert_eq!(view.calories[4].target_kcal, 1_200.0);
        // Le 17 : maintien (dépense de 1 705,8 kcal), journée commencée seulement.
        let last = view.calories[6];
        assert!((last.target_kcal - 1_705.8).abs() < 1e-9);
        assert_eq!(last.eaten_kcal, Some(300.0));
        assert!(last.partial);
        // Moyennes : seuls le 15 et le 16 comptent.
        assert_eq!(view.logged_days, 2);
        assert_eq!(view.mean_eaten_kcal, Some(1_500.0));
        assert_eq!(view.mean_target_kcal, Some(1_200.0));

        // Poids : la pesée du profil, et la tendance du 15 au 17.
        assert_eq!(
            view.weigh_ins,
            vec![WeightPoint {
                date: date(15),
                kg: 70.0
            }]
        );
        assert_eq!(view.trend.len(), 3);
        assert_eq!(view.trend_change_kg, Some(0.0));
        assert_eq!(view.goal_weight_kg, 62.0);
        // Maintien : pas de projection.
        assert_eq!(view.projected_goal_date, None);
    }

    #[test]
    fn progress_periods() {
        let mut service = ready();
        service.add_weigh_in(date(16), 69.6, 2_000).unwrap();
        let today = date(17);

        let all = service.progress(ProgressPeriod::All, today).unwrap();
        assert_eq!(all.from, date(11), "au moins une semaine");

        let year = service.progress(ProgressPeriod::Year, today).unwrap();
        assert!(year.weekly);
        assert_eq!(year.days, 365);
        assert_eq!(year.calories.first().unwrap().from, year.from);
        assert_eq!(year.calories.last().unwrap().to, today);
        // Semaines du lundi : le 16 novembre 2026 est un lundi.
        assert_eq!(year.calories.last().unwrap().from, date(16));
        // α = 0,1 : 70 → 69,96 le 16 → 69,924 le 17 (le dernier poids, 69,6, est conservé).
        let change = year.trend_change_kg.unwrap();
        assert!((change + 0.076).abs() < 1e-9, "{change}");
        // Perte de poids : la projection existe.
        assert!(year.projected_goal_date.is_some_and(|d| d > today));

        let quarter = service.progress(ProgressPeriod::Quarter, today).unwrap();
        assert!(!quarter.weekly);
        assert_eq!(quarter.calories.len(), 90);
    }

    #[test]
    fn custom_food_barcode() {
        let (mut service, _, _, _) = kitchen();
        let with_code = |code: &str| CustomFoodInput {
            barcode: Some(code.to_owned()),
            ..granola()
        };
        // Espaces retirés ; le scan de ce code ouvre ensuite l'aliment, à jour.
        let detail = service
            .create_custom_food(with_code(" 3017 6204 22003 "), 1_000)
            .unwrap();
        assert_eq!(detail.food.barcode.as_deref(), Some("3017620422003"));
        let (found, fresh) = service
            .local_product("3017620422003", 1_000_000_000_000)
            .unwrap()
            .unwrap();
        assert_eq!(found.food.meta.id, detail.food.meta.id);
        assert!(fresh, "un aliment personnel n'expire pas");

        // Un seul aliment personnel par code-barres ; un code mal formé est refusé.
        assert!(
            service
                .create_custom_food(with_code("3017620422003"), 2_000)
                .is_err()
        );
        assert!(
            service
                .create_custom_food(with_code("12345"), 2_000)
                .is_err()
        );
        assert!(
            service
                .create_custom_food(with_code("30176204220ab"), 2_000)
                .is_err()
        );
        // Vide : pas de code-barres.
        let blank = service.create_custom_food(with_code("  "), 2_000).unwrap();
        assert_eq!(blank.food.barcode, None);
        assert_eq!(service.custom_foods().unwrap().len(), 2);
    }
}
