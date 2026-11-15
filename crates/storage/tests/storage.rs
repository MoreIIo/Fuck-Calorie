//! Tests d'intégration du stockage : chaque test part d'une base vide en mémoire.

use chrono::{Days, NaiveDate, Weekday};
use fc_core::activity::{Activity, ActivitySource};
use fc_core::adaptive::{AdaptiveOutcome, DayRecord, estimate_tdee};
use fc_core::food::{Food, FoodSource, Portion, Unit};
use fc_core::goal::Goal;
use fc_core::hlc::{HlcClock, SyncMeta};
use fc_core::journal::{
    DayStatus, LogEntry, SavedMeal, SavedMealItem, WeighIn, default_meal_slots, weigh_in_points,
};
use fc_core::nutrients::{NutrientFacts, Nutrients};
use fc_core::profile::{ActivityLevel, Profile, ProfileRevision, Sex, profile_on};
use fc_storage::Db;
use uuid::Uuid;

fn day(n: u64) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 1).unwrap() + Days::new(n)
}

struct Fixture {
    db: Db,
    clock: HlcClock,
    now: u64,
}

impl Fixture {
    fn new() -> Self {
        Self {
            db: Db::open_in_memory().unwrap(),
            clock: HlcClock::new(Uuid::from_u128(1)),
            now: 0,
        }
    }

    fn meta(&mut self) -> SyncMeta {
        self.now += 1;
        SyncMeta::new(self.clock.tick(self.now))
    }

    fn food(&mut self, source: FoodSource, name: &str, brand: Option<&str>, kcal: f64) -> Food {
        Food {
            meta: self.meta(),
            source,
            source_ref: None,
            barcode: None,
            name: name.to_owned(),
            brand: brand.map(str::to_owned),
            per_100: NutrientFacts {
                kcal: Some(kcal),
                protein_g: Some(4.0),
                ..Default::default()
            },
            density: None,
            unit_weight_g: None,
            portions: vec![Portion {
                label: "1 pot".into(),
                grams: 125.0,
            }],
            updated_at_ms: 1_000,
        }
    }

    fn quick_add(&mut self, date: NaiveDate, slot: Uuid, kcal: f64) -> LogEntry {
        let meta = self.meta();
        let created = i64::try_from(self.now).unwrap();
        LogEntry::quick_add(
            meta,
            date,
            slot,
            "Ajout",
            Nutrients {
                kcal,
                ..Nutrients::ZERO
            },
            created,
        )
    }
}

#[test]
fn food_round_trip_and_tombstone() {
    let mut fx = Fixture::new();
    let mut food = fx.food(FoodSource::Custom, "Yaourt nature", Some("Marque"), 61.0);
    food.density = Some(1.03);
    food.unit_weight_g = Some(125.0);
    assert!(fx.db.upsert_food(&food).unwrap());
    assert_eq!(fx.db.food(food.meta.id).unwrap(), Some(food.clone()));
    assert_eq!(fx.db.food(Uuid::from_u128(999)).unwrap(), None);

    // Suppression : la fiche disparaît des lectures.
    fx.now += 1;
    food.meta.delete(fx.clock.tick(fx.now));
    assert!(fx.db.upsert_food(&food).unwrap());
    assert_eq!(fx.db.food(food.meta.id).unwrap(), None);
    assert!(fx.db.search_foods("yaourt", 10).unwrap().is_empty());
}

#[test]
fn writes_are_idempotent_and_never_go_backwards() {
    let mut fx = Fixture::new();
    let old = fx.food(FoodSource::Custom, "Ancien nom", None, 61.0);
    let mut new = old.clone();
    new.name = "Nouveau nom".into();
    fx.now += 1;
    new.meta.touch(fx.clock.tick(fx.now));

    assert!(fx.db.upsert_food(&new).unwrap());
    // Rejouer la même écriture ne change rien.
    assert!(!fx.db.upsert_food(&new).unwrap());
    // Une version plus ancienne reçue après coup (synchronisation) est ignorée.
    assert!(!fx.db.upsert_food(&old).unwrap());
    assert_eq!(
        fx.db.food(new.meta.id).unwrap().unwrap().name,
        "Nouveau nom"
    );

    // Dans l'autre ordre, le résultat est le même : convergence.
    let other = Fixture::new();
    assert!(other.db.upsert_food(&old).unwrap());
    assert!(other.db.upsert_food(&new).unwrap());
    assert_eq!(
        other.db.food(new.meta.id).unwrap(),
        fx.db.food(new.meta.id).unwrap()
    );
}

#[test]
fn search_is_accent_and_case_insensitive() {
    let mut fx = Fixture::new();
    let foods = [
        fx.food(FoodSource::Ciqual, "Crème fraîche épaisse", None, 300.0),
        fx.food(
            FoodSource::Off,
            "Crème dessert chocolat",
            Some("Danette"),
            130.0,
        ),
        fx.food(FoodSource::Custom, "Ma crème de marrons", None, 250.0),
        fx.food(FoodSource::Ciqual, "Œuf dur", None, 134.0),
        fx.food(FoodSource::Ciqual, "Lait 100% végétal", None, 40.0),
    ];
    for food in &foods {
        fx.db.upsert_food(food).unwrap();
    }
    let names = |query: &str| -> Vec<String> {
        fx.db
            .search_foods(query, 10)
            .unwrap()
            .into_iter()
            .map(|f| f.name)
            .collect()
    };

    // Moins de 2 caractères : pas de recherche.
    assert!(names("c").is_empty());
    assert!(names("  ").is_empty());
    // Accents et casse ignorés ; ordre : perso, puis Ciqual, puis Open Food Facts.
    assert_eq!(
        names("CREME"),
        [
            "Ma crème de marrons",
            "Crème fraîche épaisse",
            "Crème dessert chocolat"
        ]
    );
    assert_eq!(names("oeuf"), ["Œuf dur"]);
    // Plusieurs mots, dans n'importe quel ordre ; la marque compte.
    assert_eq!(names("chocolat crème"), ["Crème dessert chocolat"]);
    assert_eq!(names("danette"), ["Crème dessert chocolat"]);
    // La ponctuation de la requête est ignorée (et ne sert jamais de joker SQL).
    assert_eq!(names("100%"), ["Lait 100% végétal"]);
    assert_eq!(names("%%"), Vec::<String>::new());
    // Limite respectée.
    assert_eq!(fx.db.search_foods("creme", 2).unwrap().len(), 2);
}

#[test]
fn custom_food_masks_off_product_with_same_barcode() {
    let mut fx = Fixture::new();
    let mut off = fx.food(FoodSource::Off, "Biscuits (fiche OFF)", None, 480.0);
    off.barcode = Some("3017620422003".into());
    off.updated_at_ms = 9_999;
    fx.db.upsert_food(&off).unwrap();
    assert_eq!(
        fx.db
            .food_by_barcode("3017620422003")
            .unwrap()
            .unwrap()
            .name,
        off.name
    );

    let mut custom = fx.food(FoodSource::Custom, "Biscuits (corrigé)", None, 455.0);
    custom.barcode = off.barcode.clone();
    fx.db.upsert_food(&custom).unwrap();
    assert_eq!(
        fx.db
            .food_by_barcode("3017620422003")
            .unwrap()
            .unwrap()
            .name,
        custom.name
    );
    assert_eq!(fx.db.food_by_barcode("0000000000000").unwrap(), None);
}

#[test]
fn journal_entries_recents_and_last_quantity() {
    let mut fx = Fixture::new();
    let mut clock = fx.clock.clone();
    let slots = default_meal_slots(
        ["Petit-déjeuner", "Déjeuner", "Dîner", "Collation"],
        || clock.tick(1),
    );
    for slot in &slots {
        assert!(fx.db.upsert_meal_slot(slot).unwrap());
    }
    assert_eq!(fx.db.meal_slots().unwrap(), slots);

    let yaourt = fx.food(FoodSource::Off, "Yaourt nature", None, 61.0);
    let pain = fx.food(FoodSource::Ciqual, "Pain complet", None, 250.0);
    fx.db.upsert_food(&yaourt).unwrap();
    fx.db.upsert_food(&pain).unwrap();
    assert_eq!(fx.db.last_quantity_for(yaourt.meta.id).unwrap(), None);
    assert!(fx.db.recent_foods(10).unwrap().is_empty());

    let breakfast = slots[0].meta.id;
    let pot = Unit::Portion("1 pot".into());
    let e1 =
        LogEntry::from_food(fx.meta(), day(0), breakfast, &yaourt, 1.0, pot.clone(), 10).unwrap();
    let e2 =
        LogEntry::from_food(fx.meta(), day(0), breakfast, &pain, 60.0, Unit::Gram, 20).unwrap();
    let mut e3 =
        LogEntry::from_food(fx.meta(), day(1), breakfast, &yaourt, 180.0, Unit::Gram, 30).unwrap();
    for entry in [&e1, &e2, &e3] {
        assert!(fx.db.upsert_log_entry(entry).unwrap());
    }

    assert_eq!(
        fx.db.log_entries_on(day(0)).unwrap(),
        vec![e1.clone(), e2.clone()]
    );
    assert_eq!(fx.db.log_entries_between(day(0), day(1)).unwrap().len(), 3);
    assert!(fx.db.log_entries_on(day(5)).unwrap().is_empty());

    // Récents : du plus récemment utilisé au plus ancien.
    let recents: Vec<String> = fx
        .db
        .recent_foods(10)
        .unwrap()
        .into_iter()
        .map(|f| f.name)
        .collect();
    assert_eq!(recents, ["Yaourt nature", "Pain complet"]);
    assert_eq!(fx.db.recent_foods(1).unwrap().len(), 1);

    // Dernière quantité utilisée, pour pré-remplir la saisie.
    assert_eq!(
        fx.db.last_quantity_for(yaourt.meta.id).unwrap(),
        Some((180.0, Unit::Gram))
    );
    fx.now += 1;
    e3.meta.delete(fx.clock.tick(fx.now));
    fx.db.upsert_log_entry(&e3).unwrap();
    assert_eq!(
        fx.db.last_quantity_for(yaourt.meta.id).unwrap(),
        Some((1.0, pot))
    );
    assert_eq!(fx.db.log_entries_on(day(1)).unwrap(), vec![]);

    // Totaux par jour : 76,25 + 150 kcal le premier jour, plus rien le second.
    let totals = fx.db.daily_totals_between(day(0), day(1)).unwrap();
    assert_eq!(totals.len(), 1);
    assert_eq!(totals[0].0, day(0));
    assert!((totals[0].1.kcal - 226.25).abs() < 1e-9);
    assert!((totals[0].1.protein_g - (5.0 + 2.4)).abs() < 1e-9);
}

#[test]
fn batch_write_is_all_or_nothing() {
    let mut fx = Fixture::new();
    let slot = Uuid::from_u128(7);
    let good = fx.quick_add(day(0), slot, 500.0);
    // SQLite refuse NaN dans une colonne NOT NULL : la seconde entrée fait échouer le lot.
    let bad = fx.quick_add(day(0), slot, f64::NAN);
    assert!(fx.db.upsert_log_entries(&[good.clone(), bad]).is_err());
    assert!(
        fx.db.log_entries_on(day(0)).unwrap().is_empty(),
        "rien n'a été écrit"
    );

    // Copie d'une journée : tout le lot est écrit, et le rejouer n'écrit rien de plus.
    let batch = [good.clone(), fx.quick_add(day(0), slot, 300.0)];
    assert_eq!(fx.db.upsert_log_entries(&batch).unwrap(), 2);
    assert_eq!(fx.db.upsert_log_entries(&batch).unwrap(), 0);
    assert_eq!(fx.db.log_entries_on(day(0)).unwrap().len(), 2);
}

#[test]
fn day_status_is_one_row_per_date() {
    let mut fx = Fixture::new();
    assert_eq!(fx.db.day_status(day(0)).unwrap(), None);
    let first = DayStatus {
        date: day(0),
        complete: true,
        note: None,
        hlc: fx.meta().hlc,
    };
    let second = DayStatus {
        date: day(0),
        complete: false,
        note: Some("Resto le soir".into()),
        hlc: fx.meta().hlc,
    };
    assert!(fx.db.set_day_status(&first).unwrap());
    assert!(fx.db.set_day_status(&second).unwrap());
    assert!(
        !fx.db.set_day_status(&first).unwrap(),
        "version plus ancienne ignorée"
    );
    assert_eq!(fx.db.day_status(day(0)).unwrap(), Some(second.clone()));
    assert_eq!(
        fx.db.day_statuses_between(day(0), day(6)).unwrap(),
        vec![second]
    );
}

#[test]
fn saved_meals_round_trip() {
    let mut fx = Fixture::new();
    let meal = SavedMeal {
        meta: fx.meta(),
        name: "Mon petit-déj".into(),
        items: vec![
            SavedMealItem {
                food_id: Uuid::from_u128(1),
                quantity: 1.0,
                unit: Unit::Piece,
            },
            SavedMealItem {
                food_id: Uuid::from_u128(2),
                quantity: 2.0,
                unit: Unit::Portion("1 tranche".into()),
            },
        ],
    };
    assert!(fx.db.upsert_saved_meal(&meal).unwrap());
    assert_eq!(fx.db.saved_meals().unwrap(), vec![meal.clone()]);
    assert_eq!(fx.db.saved_meal(meal.meta.id).unwrap(), Some(meal));
    assert_eq!(fx.db.saved_meal(Uuid::from_u128(5)).unwrap(), None);
}

#[test]
fn weigh_ins_and_activities() {
    let mut fx = Fixture::new();
    let w1 = WeighIn {
        meta: fx.meta(),
        date: day(0),
        weight_kg: 70.4,
        body_fat_pct: None,
    };
    let w2 = WeighIn {
        meta: fx.meta(),
        date: day(3),
        weight_kg: 70.1,
        body_fat_pct: Some(24.5),
    };
    fx.db.upsert_weigh_in(&w1).unwrap();
    fx.db.upsert_weigh_in(&w2).unwrap();
    assert_eq!(
        fx.db.weigh_ins_between(day(0), day(3)).unwrap(),
        vec![w1.clone(), w2]
    );
    assert_eq!(fx.db.weigh_ins_between(day(0), day(2)).unwrap(), vec![w1]);

    let mut ride = Activity {
        meta: fx.meta(),
        date: day(1),
        source: ActivitySource::Strava,
        kind: "Ride".into(),
        duration_min: 95.0,
        kcal: 850.0,
        external_id: Some("1234567890".into()),
    };
    let walk = Activity {
        meta: fx.meta(),
        date: day(1),
        source: ActivitySource::Manual,
        kind: "walking".into(),
        duration_min: 30.0,
        kcal: 120.0,
        external_id: None,
    };
    fx.db.upsert_activity(&ride).unwrap();
    fx.db.upsert_activity(&walk).unwrap();
    assert_eq!(fx.db.activities_between(day(0), day(6)).unwrap().len(), 2);
    let found = fx
        .db
        .activity_by_external_id(ActivitySource::Strava, "1234567890")
        .unwrap();
    assert_eq!(found, Some(ride.clone()));
    assert_eq!(
        fx.db
            .activity_by_external_id(ActivitySource::Manual, "1234567890")
            .unwrap(),
        None
    );

    // Activité Strava supprimée par l'utilisatrice : elle disparaît des listes, mais reste
    // connue pour ne pas être réimportée.
    fx.now += 1;
    ride.meta.delete(fx.clock.tick(fx.now));
    fx.db.upsert_activity(&ride).unwrap();
    assert_eq!(
        fx.db.activities_between(day(0), day(6)).unwrap(),
        vec![walk]
    );
    let found = fx
        .db
        .activity_by_external_id(ActivitySource::Strava, "1234567890")
        .unwrap();
    assert!(found.unwrap().meta.deleted);
}

#[test]
fn profile_history() {
    let mut fx = Fixture::new();
    let profile = Profile {
        sex: Sex::Unspecified,
        birth_date: NaiveDate::from_ymd_opt(1990, 6, 15).unwrap(),
        height_cm: 170.0,
        weight_kg: 75.0,
        goal_weight_kg: 68.0,
        goal: Goal::Lose { kg_per_week: 0.5 },
        activity_level: ActivityLevel::LightlyActive,
        week_start: Weekday::Mon,
    };
    let r1 = ProfileRevision {
        meta: fx.meta(),
        effective_from: day(0),
        profile: profile.clone(),
    };
    let r2 = ProfileRevision {
        meta: fx.meta(),
        effective_from: day(10),
        profile: Profile {
            goal: Goal::Maintain,
            weight_kg: 68.0,
            ..profile
        },
    };
    // Écrites dans le désordre, relues par date d'effet.
    fx.db.upsert_profile_revision(&r2).unwrap();
    fx.db.upsert_profile_revision(&r1).unwrap();
    let revisions = fx.db.profile_revisions().unwrap();
    assert_eq!(revisions, vec![r1, r2]);
    // Les anciens jours gardent leur objectif d'origine.
    assert_eq!(
        profile_on(&revisions, day(5)).unwrap().goal,
        Goal::Lose { kg_per_week: 0.5 }
    );
    assert_eq!(
        profile_on(&revisions, day(12)).unwrap().goal,
        Goal::Maintain
    );
}

/// De bout en bout : trois semaines de journal et de pesées relues depuis la base
/// alimentent l'algorithme adaptatif.
#[test]
fn adaptive_estimate_from_stored_data() {
    let mut fx = Fixture::new();
    let slot = Uuid::from_u128(7);
    let mut weight = 70.0;
    for n in 0..21 {
        let weigh_in = WeighIn {
            meta: fx.meta(),
            date: day(n),
            weight_kg: weight,
            body_fat_pct: None,
        };
        fx.db.upsert_weigh_in(&weigh_in).unwrap();
        // 1 200 + 800 kcal mangées, vraie dépense 2 000 kcal : poids stable.
        let entries = [
            fx.quick_add(day(n), slot, 1_200.0),
            fx.quick_add(day(n), slot, 800.0),
        ];
        fx.db.upsert_log_entries(&entries).unwrap();
        let status = DayStatus {
            date: day(n),
            complete: true,
            note: None,
            hlc: fx.meta().hlc,
        };
        fx.db.set_day_status(&status).unwrap();
        weight += 0.0;
    }

    let statuses = fx.db.day_statuses_between(day(0), day(20)).unwrap();
    let days: Vec<DayRecord> = fx
        .db
        .daily_totals_between(day(0), day(20))
        .unwrap()
        .into_iter()
        .map(|(date, total)| DayRecord {
            date,
            eaten_kcal: total.kcal,
            activity_kcal: 0.0,
            target_kcal: 1_800.0,
            marked_complete: statuses.iter().any(|s| s.date == date && s.complete),
        })
        .collect();
    let weigh_ins = weigh_in_points(&fx.db.weigh_ins_between(day(0), day(20)).unwrap());

    match estimate_tdee(day(20), &days, &weigh_ins, 1_950.0, 0.1) {
        AdaptiveOutcome::Estimated(e) => {
            assert_eq!((e.logged_days, e.weigh_in_days), (21, 21));
            assert!((e.tdee_base - 2_000.0).abs() < 1e-6);
        }
        other => panic!("estimation attendue, obtenu {other:?}"),
    }
}
