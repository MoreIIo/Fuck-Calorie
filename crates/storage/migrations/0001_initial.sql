-- Schéma initial de Fuck Calorie (version 1).
--
-- Conventions :
--   * les tables synchronisées commencent par `id` (UUID v7), `hlc` (horloge logique
--     hybride, texte triable) et `deleted` (tombstone) ;
--   * les dates sont au format ISO `AAAA-MM-JJ`, les heures `HH:MM:SS` ;
--   * les valeurs composées (portions, unité, profil) sont en JSON.
--
-- Une migration appliquée ne se modifie plus : tout changement passe par un nouveau fichier.

CREATE TABLE foods (
    id            TEXT PRIMARY KEY NOT NULL,
    hlc           TEXT NOT NULL,
    deleted       INTEGER NOT NULL DEFAULT 0,
    source        TEXT NOT NULL,          -- ciqual | off | custom | recipe
    source_ref    TEXT,
    barcode       TEXT,
    name          TEXT NOT NULL,
    brand         TEXT,
    search_text   TEXT NOT NULL,          -- nom + marque, sans accents ni majuscules
    per_100       TEXT NOT NULL,          -- JSON : les 8 nutriments, chacun facultatif
    density       REAL,
    unit_weight_g REAL,
    portions      TEXT NOT NULL,          -- JSON : [{label, grams}]
    updated_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX foods_barcode ON foods (barcode) WHERE barcode IS NOT NULL;
CREATE INDEX foods_source_ref ON foods (source, source_ref) WHERE source_ref IS NOT NULL;

CREATE TABLE meal_slots (
    id         TEXT PRIMARY KEY NOT NULL,
    hlc        TEXT NOT NULL,
    deleted    INTEGER NOT NULL DEFAULT 0,
    name       TEXT NOT NULL,
    sort_order INTEGER NOT NULL,
    time_start TEXT,
    time_end   TEXT
) STRICT;

CREATE TABLE log_entries (
    id            TEXT PRIMARY KEY NOT NULL,
    hlc           TEXT NOT NULL,
    deleted       INTEGER NOT NULL DEFAULT 0,
    date          TEXT NOT NULL,
    meal_slot_id  TEXT NOT NULL,
    food_id       TEXT,                   -- NULL = ajout rapide
    label         TEXT NOT NULL,
    quantity      REAL NOT NULL,
    unit          TEXT NOT NULL,          -- JSON
    grams         REAL NOT NULL,
    -- Instantané des nutriments : l'historique ne change pas si la fiche change.
    kcal          REAL NOT NULL,
    protein_g     REAL NOT NULL,
    carbs_g       REAL NOT NULL,
    sugars_g      REAL NOT NULL,
    fat_g         REAL NOT NULL,
    sat_fat_g     REAL NOT NULL,
    fiber_g       REAL NOT NULL,
    salt_g        REAL NOT NULL,
    created_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX log_entries_date ON log_entries (date);
CREATE INDEX log_entries_food ON log_entries (food_id, created_at_ms) WHERE food_id IS NOT NULL;

CREATE TABLE saved_meals (
    id      TEXT PRIMARY KEY NOT NULL,
    hlc     TEXT NOT NULL,
    deleted INTEGER NOT NULL DEFAULT 0,
    name    TEXT NOT NULL,
    items   TEXT NOT NULL                 -- JSON : [{food_id, quantity, unit}]
) STRICT;

CREATE TABLE weigh_ins (
    id           TEXT PRIMARY KEY NOT NULL,
    hlc          TEXT NOT NULL,
    deleted      INTEGER NOT NULL DEFAULT 0,
    date         TEXT NOT NULL,
    weight_kg    REAL NOT NULL,
    body_fat_pct REAL
) STRICT;
CREATE INDEX weigh_ins_date ON weigh_ins (date);

CREATE TABLE activities (
    id           TEXT PRIMARY KEY NOT NULL,
    hlc          TEXT NOT NULL,
    deleted      INTEGER NOT NULL DEFAULT 0,
    date         TEXT NOT NULL,
    source       TEXT NOT NULL,           -- manual | strava
    kind         TEXT NOT NULL,
    duration_min REAL NOT NULL,
    kcal         REAL NOT NULL,
    external_id  TEXT
) STRICT;
CREATE INDEX activities_date ON activities (date);
CREATE INDEX activities_external ON activities (source, external_id) WHERE external_id IS NOT NULL;

-- Une ligne par date : bouton « Journée terminée » et note libre.
CREATE TABLE day_status (
    date     TEXT PRIMARY KEY NOT NULL,
    hlc      TEXT NOT NULL,
    complete INTEGER NOT NULL DEFAULT 0,
    note     TEXT
) STRICT;

-- Profil historisé : une révision par date d'effet.
CREATE TABLE profile_revisions (
    id             TEXT PRIMARY KEY NOT NULL,
    hlc            TEXT NOT NULL,
    deleted        INTEGER NOT NULL DEFAULT 0,
    effective_from TEXT NOT NULL,
    profile        TEXT NOT NULL          -- JSON
) STRICT;

-- Réglages propres à cet appareil (non synchronisés). Jamais de jeton ni de clé API ici :
-- ils vont dans le coffre sécurisé de la plateforme.
CREATE TABLE settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) STRICT;
