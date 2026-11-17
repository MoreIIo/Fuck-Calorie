//! Types échangés entre l'interface (Leptos) et l'application (Tauri, `fc-service`).
//!
//! Uniquement des données sérialisables, sans logique : l'interface affiche, l'application
//! calcule. Les champs sont en `snake_case` des deux côtés.

use chrono::{NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::food::{Food, FoodSource, Unit};
use crate::goal::Goal;
use crate::macros::MacroTargets;
use crate::nutrients::{NutrientFacts, Nutrients};
use crate::profile::{ActivityLevel, Sex};
use crate::progress::{CalorieBar, ProgressPeriod};

/// Saisie du profil (écran de démarrage et réglages, §2.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileInput {
    pub sex: Sex,
    pub birth_date: NaiveDate,
    pub height_cm: f64,
    pub weight_kg: f64,
    pub goal_weight_kg: f64,
    pub goal: Goal,
    pub activity_level: ActivityLevel,
    /// Premier jour de la semaine (§2.1), réglable dans *Réglages*. `None` : inchangé
    /// (lundi pour un nouveau profil).
    #[serde(default)]
    pub week_start: Option<Weekday>,
}

/// Écran *Réglages* : le profil en vigueur et les options de la cible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingsView {
    pub profile: crate::profile::Profile,
    /// La cible est figée à la main (§2.3).
    pub target_frozen: bool,
}

/// Ajout rapide (§3.6) : des calories et, facultativement, les macros.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickAddInput {
    pub date: NaiveDate,
    pub meal_slot_id: Uuid,
    /// Libellé facultatif (« Resto italien »). Vide = « Ajout rapide ».
    pub label: String,
    pub kcal: f64,
    pub protein_g: Option<f64>,
    pub carbs_g: Option<f64>,
    pub fat_g: Option<f64>,
}

/// Un résultat de recherche d'aliment (§3.3) : nom, marque, kcal / 100 g, portion par
/// défaut et source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoodHit {
    /// Identifiant de la fiche locale. `None` pour un produit Open Food Facts qui n'est pas
    /// encore dans le cache de l'appareil : il s'ouvre alors par son code-barres.
    pub id: Option<Uuid>,
    pub barcode: Option<String>,
    pub source: FoodSource,
    pub name: String,
    pub brand: Option<String>,
    /// `None` : fiche sans calories, inutilisable telle quelle pour le suivi.
    pub kcal_100: Option<f64>,
    /// Portion par défaut (« 1 pot ») et son poids en grammes.
    pub portion: Option<(String, f64)>,
}

impl FoodHit {
    /// Résultat correspondant à une fiche. `local` : la fiche est-elle enregistrée sur
    /// l'appareil ?
    #[must_use]
    pub fn from_food(food: &Food, local: bool) -> Self {
        Self {
            id: local.then_some(food.meta.id),
            barcode: food.barcode.clone(),
            source: food.source,
            name: food.name.clone(),
            brand: food.brand.clone(),
            kcal_100: food.per_100.kcal,
            portion: food.portions.first().map(|p| (p.label.clone(), p.grams)),
        }
    }
}

/// Écran de quantité (§3.4) : la fiche, la quantité pré-remplie et les unités proposées.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoodDetail {
    pub food: Food,
    /// Quantité pré-remplie : la dernière utilisée, sinon la portion par défaut, sinon 100 g.
    pub quantity: f64,
    pub unit: Unit,
    /// Unités utilisables pour cet aliment, dans l'ordre d'affichage.
    pub units: Vec<Unit>,
    /// L'aliment est-il un favori (§3.6) ?
    pub favorite: bool,
}

/// Une ligne du journal ouverte pour modification (§3.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryDetail {
    pub id: Uuid,
    pub meal_slot_id: Uuid,
    pub label: String,
    pub kcal: f64,
    /// Fiche de l'aliment, avec la quantité de la ligne. `None` pour un ajout rapide (ou si
    /// la fiche n'existe plus) : seul le repas peut alors être changé.
    pub food: Option<FoodDetail>,
}

/// Modification d'une ligne du journal (§3.5) : la déplacer vers un autre repas et,
/// facultativement, changer sa quantité.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateEntryInput {
    pub id: Uuid,
    pub meal_slot_id: Uuid,
    /// Nouvelle quantité, avec `unit`. `None` : quantité inchangée.
    pub quantity: Option<f64>,
    pub unit: Option<Unit>,
}

/// Création d'un aliment personnel (§3.7) : valeurs pour 100 g (ou 100 ml).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomFoodInput {
    pub name: String,
    /// Marque, facultative (vide = aucune).
    pub brand: String,
    /// Les calories sont obligatoires, le reste est facultatif.
    pub per_100: NutrientFacts,
    /// Poids d'une portion en grammes, facultatif.
    pub portion_g: Option<f64>,
    /// Liquide : les valeurs sont pour 100 ml.
    pub liquid: bool,
    /// Code-barres, facultatif (§3.7) : scanner ce produit ouvre alors cet aliment.
    #[serde(default)]
    pub barcode: Option<String>,
}

/// Un repas enregistré (§3.6), tel qu'affiché dans la liste.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedMealView {
    pub id: Uuid,
    pub name: String,
    /// Total d'après les fiches actuelles des aliments.
    pub kcal: f64,
    /// Nombre d'aliments.
    pub items: usize,
}

/// Ajout d'un aliment au journal (§3.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogFoodInput {
    pub date: NaiveDate,
    pub meal_slot_id: Uuid,
    pub food_id: Uuid,
    pub quantity: f64,
    pub unit: Unit,
}

/// Une ligne du journal, telle qu'affichée.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryView {
    pub id: Uuid,
    pub label: String,
    pub kcal: f64,
    pub protein_g: f64,
    pub carbs_g: f64,
    pub fat_g: f64,
    /// Ajout rapide (pas de fiche aliment).
    pub quick_add: bool,
}

/// Un repas de la journée, avec son total et ses lignes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MealView {
    pub id: Uuid,
    pub name: String,
    pub kcal: f64,
    pub entries: Vec<EntryView>,
}

/// Mini-carte poids (§5.2) : la tendance d'abord, le poids brut ensuite (§2.8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeightView {
    pub trend_kg: f64,
    pub last_kg: f64,
    pub last_date: NaiveDate,
    /// Variation du poids de tendance sur 7 jours (négatif = perte).
    pub change_7d_kg: Option<f64>,
    /// Une pesée a-t-elle déjà été faite ce jour-là ?
    pub weighed_today: bool,
}

/// Tout ce qu'affiche l'écran *Aujourd'hui* (§5.2) pour une date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TodayView {
    pub date: NaiveDate,
    /// Cible de base (hors activités).
    pub base_target_kcal: f64,
    /// Calories d'activité ajoutées au budget.
    pub activity_kcal: f64,
    /// Cible du jour = cible de base + activités.
    pub target_kcal: f64,
    pub eaten: Nutrients,
    /// Calories restantes (négatif = dépassement).
    pub remaining_kcal: f64,
    /// Part de la cible consommée, entre 0 et 1.
    pub progress: f64,
    pub macros: MacroTargets,
    pub meals: Vec<MealView>,
    /// Repas pré-sélectionné pour un ajout, d'après l'heure (§3.5).
    pub suggested_meal_id: Option<Uuid>,
    pub weight: Option<WeightView>,
    /// Bouton *Journée terminée* (§3.5).
    pub complete: bool,
    /// Dépense quotidienne utilisée pour la cible.
    pub tdee_kcal: f64,
    /// `true` si la dépense vient de l'algorithme adaptatif, `false` si c'est l'estimation
    /// initiale (§2.2).
    pub tdee_adaptive: bool,
    /// La cible a été remontée au plancher calorique (§2.7).
    pub floor_applied: bool,
    /// Le déficit a été limité à 1 % du poids par semaine (§2.7).
    pub deficit_capped: bool,
    pub goal: Goal,
    pub goal_weight_kg: f64,
    /// Date estimée d'atteinte du poids objectif (§2.4).
    pub projected_goal_date: Option<NaiveDate>,
}

/// Carte « Bilan de la semaine » (§5.4) : la semaine écoulée et le recalcul hebdomadaire de
/// la dépense (§2.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeekReport {
    /// Premier jour de la semaine en cours : le jour du recalcul.
    pub computed_for: NaiveDate,
    /// Semaine écoulée (7 jours, bornes incluses).
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Jours journalisés sur les 7.
    pub logged_days: u32,
    /// Calories moyennes des jours journalisés. `None` si aucun jour ne l'est.
    pub mean_eaten_kcal: Option<f64>,
    /// Cible moyenne de ces mêmes jours (activités comprises).
    pub mean_target_kcal: Option<f64>,
    /// Écart cumulé sur ces jours : mangé − cible (négatif = sous la cible).
    pub cumulative_gap_kcal: Option<f64>,
    /// Variation du poids de tendance sur la semaine (négatif = perte).
    pub trend_change_kg: Option<f64>,
    /// Dépense estimée avant le recalcul.
    pub previous_tdee_kcal: f64,
    /// Dépense estimée retenue après le recalcul (inchangée s'il n'y a pas assez de données
    /// ou si la cible est figée).
    pub tdee_kcal: f64,
    /// Dépense calculée cette semaine, avant amortissement. `None` : pas assez de données.
    pub computed_tdee_kcal: Option<f64>,
    /// Cible de base avant et après le recalcul.
    pub previous_target_kcal: f64,
    pub target_kcal: f64,
    /// Données de la fenêtre de 21 jours : jours journalisés (14 requis) et jours avec pesée
    /// (6 requis, étalés sur au moins une semaine).
    pub window_logged_days: usize,
    pub window_weigh_in_days: usize,
    /// La cible est figée à la main (§2.3) : le recalcul est calculé mais pas appliqué.
    pub frozen: bool,
}

/// Un point d'un graphique de poids.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeightPoint {
    pub date: NaiveDate,
    pub kg: f64,
}

/// Écran *Progrès* (§5.3) pour une période.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressView {
    pub period: ProgressPeriod,
    /// Premier et dernier jour affichés (le dernier est aujourd'hui).
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Pesées de la période (moyenne du jour s'il y en a plusieurs).
    pub weigh_ins: Vec<WeightPoint>,
    /// Poids de tendance, un point par jour de la période couvert par les pesées.
    pub trend: Vec<WeightPoint>,
    /// Variation du poids de tendance sur la période (négatif = perte).
    pub trend_change_kg: Option<f64>,
    pub goal_weight_kg: f64,
    /// Date estimée d'atteinte du poids objectif (§2.4), pour la projection.
    pub projected_goal_date: Option<NaiveDate>,
    /// Les barres de calories sont-elles des semaines (au-delà de 90 jours) ?
    pub weekly: bool,
    pub calories: Vec<CalorieBar>,
    /// Moyennes des jours journalisés de la période.
    pub mean_eaten_kcal: Option<f64>,
    pub mean_target_kcal: Option<f64>,
    pub logged_days: u32,
    /// Nombre de jours de la période.
    pub days: u32,
}
