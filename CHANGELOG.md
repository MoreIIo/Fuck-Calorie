# Journal des modifications

Format inspiré de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/) ; le projet suit
[SemVer](https://semver.org/lang/fr/).

## [Non publié]

### Ajouté
- Workflow GitHub qui construit une IPA iPhone de test, non signée (pré-version
  « ipa-test »), à installer avec Sideloadly ; description d'accès à la caméra pour iOS ;
  marges de l'encoche de l'iPhone.
- Scan de code-barres sur Android et iOS (plugin officiel `barcode-scanner` de Tauri) :
  bouton dans la recherche et sur l'accueil, viseur, vibration au scan réussi ; produit
  inconnu → création d'un aliment personnel avec ce code-barres. Sur tous les appareils, un
  code-barres tapé dans la recherche s'ouvre directement.
- Aliments personnels : code-barres facultatif (un scan ouvre ensuite cet aliment).
- Navigation : barre en bas sur mobile, rail à gauche sur PC (*Aujourd'hui · Progrès · + ·
  Réglages*) ; le « + » ouvre la feuille d'ajout depuis n'importe quel écran.
- Écran *Progrès* : périodes 7 j · 30 j · 90 j · 1 an · Tout, graphique du poids (pesées,
  tendance, objectif, projection), graphique « Calories vs cible » (par semaine au-delà de
  90 jours, moyenne de la période, un clic ouvre le journal du jour), bilan de la semaine.
- Écran *Réglages* : modifier le profil (historisé), choisir le premier jour de la semaine,
  figer la cible, à propos (sources des données, confidentialité, licence).
- `lancer.cmd` : lance l'application sur Windows en installant ce qui manque.
- Workflow GitHub qui construit un APK Android de test (pré-version « apk-test »).
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
- Modifier le profil sans changer le poids n'ajoute plus de pesée.
- Algorithme adaptatif : la variation du poids est mesurée par régression sur les pesées, ce
  qui supprime la sous-estimation de la dépense pendant les premières semaines.
