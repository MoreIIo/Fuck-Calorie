# Journal des modifications

Format inspiré de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/) ; le projet suit
[SemVer](https://semver.org/lang/fr/).

## [Non publié]

### Ajouté
- Recalcul hebdomadaire de la dépense (algorithme adaptatif) à la première ouverture de la
  semaine, carte « Bilan de la semaine », cible figeable.
- Journal : modifier la quantité ou le repas d'une ligne, copier un repas ou toute la journée
  de la veille, favoris, aliments personnels, repas enregistrés (créer depuis un repas,
  ajouter en un clic, supprimer). Onglets *Récents · Favoris · Mes aliments · Repas* sous la
  recherche. Schéma de la base en version 2.
- Recherche d'aliments : table Ciqual 2025 embarquée (3 341 aliments, disponible hors ligne),
  recherche en ligne sur Open Food Facts avec copie locale des produits consultés, tolérance
  aux fautes de frappe, aliments récents.
- Écran de quantité : grammes, millilitres ou portion, quantité pré-remplie avec la dernière
  utilisée, raccourcis, aperçu des calories et des macros avant l'ajout.
- `fc-food-db` : table Ciqual, lecture des réponses d'Open Food Facts, client HTTP avec
  limites de débit.
- Application de bureau (Tauri 2 + Leptos) : profil de départ, écran *Aujourd'hui* (kcal
  restantes, macros, repas, ajout rapide, suppression, pesée, journée terminée, navigation
  par jour, raccourcis clavier), design system (bleu Klein, verre, angles droits, Urbanist,
  thèmes clair et sombre).
- `fc-service` : opérations de l'application sur la base locale.
- Workspace Cargo, licence MPL-2.0, intégration continue (format, clippy, tests sous Linux et
  Windows).
- `fc-core` : nutriments, profil historisé, BMR Mifflin-St Jeor et TDEE initial, modes
  d'objectif, cible calorique avec garde-fous santé, macros par défaut, poids de tendance,
  algorithme adaptatif, activités (MET, Strava), aliments et conversion des quantités,
  journal et bilan du jour, horloge logique hybride.
- `fc-storage` : base SQLite, migrations versionnées, écritures idempotentes, recherche
  d'aliments insensible aux accents, journal, pesées, activités, profil, réglages.

### Modifié
- Algorithme adaptatif : la variation du poids est mesurée par régression sur les pesées, ce
  qui supprime la sous-estimation de la dépense pendant les premières semaines.
