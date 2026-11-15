# Fuck Calorie — Cahier des charges

> Tracker de calories open source et gratuit — iOS, Android, PC, Web.
> Dépôt : https://github.com/MoreIIo/Fuck-Calorie
> Version du document : 0.8 (brouillon, étapes 1–8/9) — 2026-11-15
> Auteure : Maëlle Leynaud

Légende des priorités : **[MVP]** indispensable à la première version utilisable · **[V1]** version publique · **[Later]** plus tard.
Légende des statuts : ✅ décidé · ❓ en attente de réponse · 💡 proposition à valider.

---

## Sommaire

1. Vision et périmètre
2. Profil utilisateur et stratégie nutritionnelle
3. Journal alimentaire et base de produits
4. Scan (code-barres et étiquette nutritionnelle)
5. Dashboards et suivi
6. Design system (luxe, verre liquide)
7. Architecture technique et synchronisation
8. Exigences non fonctionnelles
9. Roadmap

---

## 1. Vision et périmètre

### 1.1 Objectif
Une application qui aide à **suivre ses apports caloriques et à perdre du poids**, au moins aussi complète que MyFitnessPal, mais :
- **gratuite et open source**, sans publicité, sans abonnement, sans fonctionnalité payante ;
- **propriétaire de ses données** : aucune donnée ne quitte les appareils et le cloud personnel de l'utilisateur ;
- **la plus clean et la plus simple possible** : saisir un repas en moins de 10 secondes.

### 1.2 Public et distribution ✅
| Phase | Public | Distribution |
|---|---|---|
| MVP | Usage personnel (Maëlle) | Build Windows, APK Android, build iOS perso (TestFlight / sideload), web |
| V1 | Public | GitHub Releases, Google Play / F-Droid, App Store, version web hébergée |

Conséquence : dès le MVP, l'architecture ne doit rien contenir de « mono-utilisateur en dur » (pas de clé ou de compte codés en dur), pour que le passage au public ne demande pas de réécriture.

### 1.3 Plateformes cibles ✅
- **Android** (≥ 10) et **iOS** (≥ 16)
- **PC** : Windows 10/11 en priorité, macOS et Linux ensuite (gratuit avec Tauri)
- **Web** : navigateurs récents (Chrome, Firefox, Safari, Edge)

### 1.4 Hors périmètre (pour l'instant)
- Réseau social, amis, défis publics
- Coaching par un humain, plans de repas payants
- Montres connectées (à rediscuter à l'étape 5)

---

## 2. Profil utilisateur et stratégie nutritionnelle

### 2.1 Données du profil ✅
| Champ | Obligatoire | Usage | Priorité |
|---|---|---|---|
| Sexe pour le calcul (Homme / Femme / Non précisé = moyenne des deux constantes) | Oui | Formule BMR | MVP |
| Date de naissance (l'âge est calculé) | Oui | Formule BMR | MVP |
| Taille (cm) | Oui | BMR, IMC | MVP |
| Poids actuel (kg, 0,1 près) | Oui | BMR, suivi | MVP |
| Poids objectif | Oui | Projection de date d'atteinte | MVP |
| Objectif (mode + rythme, cf. 2.4) | Oui | Calcul de la cible | MVP |
| Niveau d'activité de base | Oui | Estimation initiale | MVP |
| Taux de masse grasse (%) | Non | Info, recomposition | V1 |
| Mensurations (taille, hanches, cou, poitrine, bras, cuisse) | Non | Suivi hors balance | V1 |
| Unités (métrique par défaut ; impérial en option) | — | Affichage | V1 |
| Premier jour de la semaine (lundi par défaut) | — | Bilans hebdo | MVP |

Toute modification du profil est **historisée** (date d'effet), pour que les anciens jours gardent leur cible d'origine.

### 2.2 Estimation initiale de la dépense ✅
- **BMR** : Mifflin-St Jeor
  - Homme : `10 × poids + 6,25 × taille − 5 × âge + 5`
  - Femme : `10 × poids + 6,25 × taille − 5 × âge − 161`
  - Non précisé : constante = −78 (moyenne)
- **TDEE initial** = BMR × multiplicateur d'activité :

| Niveau | Multiplicateur |
|---|---|
| Sédentaire | 1,2 |
| Légèrement actif | 1,375 |
| Modérément actif | 1,55 |
| Très actif | 1,725 |
| Extrêmement actif | 1,9 |

Cette estimation ne sert que pendant les **14 premiers jours**, ou tant que l'algorithme adaptatif n'a pas assez de données.

### 2.3 Algorithme adaptatif (TDEE réel) ✅
Principe : la vraie dépense se déduit de ce que l'on mange et de l'évolution réelle du poids.

1. **Poids de tendance** : moyenne mobile exponentielle des pesées (α = 0,1, valeur par défaut ajustable), pour lisser l'eau, le sel et les variations hormonales. Les jours sans pesée sont interpolés.
2. **Fenêtre de calcul** : les 21 derniers jours (glissants).
3. **Énergie stockée/perdue** : `Δ poids de tendance × 7 700 kcal/kg`.
4. **Dépense de base estimée** (hors activités enregistrées) :
   `TDEE_base = (Σ calories mangées − Σ calories d'activités enregistrées − Δtendance × 7 700) / nombre de jours`
5. **Cible du jour** = `TDEE_base + activités enregistrées du jour × facteur de récupération − déficit de l'objectif`.
   Les jours de grosse sortie vélo ont donc automatiquement plus de budget, sans double comptage.

Règles de fiabilité :
- Calcul activé seulement si **≥ 14 jours journalisés sur 21** et **≥ 6 pesées**.
- Un jour est « complet » si l'utilisatrice le marque comme tel (bouton *Journée terminée*) ; les jours non marqués et manifestement incomplets (< 50 % de la cible) sont exclus.
- Le TDEE ne bouge pas de plus de **±100 kcal par semaine** (amortissement).
- Recalcul **une fois par semaine** (jour configurable), avec une notification qui explique le changement : « Ta dépense estimée passe de 2 310 à 2 380 kcal ».
- Possibilité de **figer** la cible manuellement.

### 2.4 Modes d'objectif ✅
| Mode | Calcul de la cible | Rythmes proposés |
|---|---|---|
| Perte de poids | TDEE − déficit | 0,25 / 0,5 / 0,75 / 1 kg/sem → −275 / −550 / −825 / −1 100 kcal/j |
| Maintien | TDEE | Plage de tolérance ± 1 kg autour du poids cible |
| Prise de masse | TDEE + surplus | +0,25 / +0,5 kg/sem → +275 / +550 kcal/j |
| Recomposition | TDEE − 10 % | Protéines élevées (2,0 g/kg par défaut) |

- **Projection** : date estimée d'atteinte du poids objectif, recalculée chaque semaine à partir de la tendance réelle.
- Passage automatique proposé (pas imposé) de *Perte* à *Maintien* quand l'objectif est atteint.

### 2.5 Macros et nutriments suivis ✅
Nutriments : **calories, protéines, glucides, lipides, fibres, sucres, graisses saturées, sel** (= étiquette nutritionnelle européenne).

Répartition par défaut (modifiable en grammes ou en %) :
| Nutriment | Défaut | Type |
|---|---|---|
| Protéines | 1,8 g/kg de poids actuel (2,0 en recomposition) | Cible |
| Lipides | 30 % des calories, minimum 0,8 g/kg | Cible |
| Glucides | Le reste | Cible |
| Fibres | ≥ 30 g | Minimum |
| Sucres | ≤ 50 g | Maximum indicatif |
| Graisses saturées | ≤ 10 % des calories | Maximum |
| Sel | ≤ 5 g | Maximum (OMS) |

Préréglages proposés : *Équilibré* (défaut), *Riche en protéines*, *Low carb*, *Personnalisé*. [V1]

### 2.6 Activité physique ✅
Trois sources, combinables :
1. **Niveau d'activité fixe** : sert à l'estimation initiale. Si Strava ou la saisie manuelle sont utilisés, l'app conseille de choisir *Sédentaire/Légèrement actif* pour éviter le double comptage.
2. **Saisie manuelle** [MVP] : type, durée, calories (ou estimation par MET × poids × durée à partir d'une liste d'activités).
3. **Import Strava** [V1] : connexion OAuth, import des activités (vélo, course, etc.) avec calories ; synchro à l'ouverture de l'app.
   - Si Strava fournit les **kilojoules** (capteur de puissance), calories = kJ × 1 (équivalence usuelle en vélo) ; sinon, calories Strava.
   - Les activités importées restent modifiables et supprimables.

**Facteur de récupération** des calories d'activité : 100 % par défaut en mode adaptatif (l'algorithme corrige les erreurs) ; réglable (50–100 %).

### 2.7 Garde-fous santé ✅
- **Plancher calorique** : la cible ne descend jamais sous 1 200 kcal (Femme) / 1 500 kcal (Homme) / 1 350 kcal (Non précisé). Modifiable seulement dans les paramètres avancés, avec un avertissement.
- **Déficit maximum** : 1 % du poids corporel par semaine.
- Avertissement si le poids objectif correspond à un **IMC < 18,5**.
- Alerte si la perte réelle dépasse le rythme choisi pendant 3 semaines de suite.
- Ton **neutre et non culpabilisant** : pas de rouge agressif ni de message « tu as échoué » en cas de dépassement.

### 2.8 Pesées ✅
- Saisie en 2 taps depuis le dashboard ; rappel de pesée optionnel (heure configurable).
- Le dashboard affiche **la tendance** en premier, le poids brut en second.
- Import des pesées depuis Health Connect / Apple Santé. [Later]

## 3. Journal alimentaire et base de produits

### 3.1 Sources de données ✅
| Source | Contenu | Accès | Priorité |
|---|---|---|---|
| **Ciqual (ANSES)** | ~3 200 aliments génériques français (bruts, cuits, plats types) | Embarqué dans l'app (SQLite, ~2 Mo), mis à jour à chaque release | MVP |
| **Open Food Facts** | Produits de supermarché avec code-barres | API en ligne + cache local | MVP |
| **Aliments personnels** | Produits créés par l'utilisatrice, recettes | Local + sync | MVP |

Contraintes Open Food Facts :
- En-tête `User-Agent` obligatoire : `FuckCalorie/<version> (<email de contact>)`.
- Respect des limites : ~100 requêtes/min pour la lecture produit, ~10/min pour la recherche → **anti-rebond de 300 ms** sur la recherche et **cache local** systématique.
- Licence ODbL : mention « Données Open Food Facts » dans l'écran produit et dans *À propos*.
- Ciqual : licence Etalab, mention « Source : Table Ciqual, ANSES ».

### 3.2 Hors ligne et cache ✅
- Ciqual toujours disponible hors ligne.
- Chaque produit OFF consulté, scanné ou mangé est **copié en local** (fiche complète + image miniature).
- Hors ligne : la recherche porte sur Ciqual + cache + aliments perso ; les produits OFF jamais vus sont indiqués « nécessite une connexion ».
- Rafraîchissement d'une fiche OFF en cache si elle a plus de 30 jours (en arrière-plan, quand le réseau est là).
- Le cache produits **n'est pas synchronisé** (il se reconstruit) ; seules les entrées du journal le sont, et elles contiennent les valeurs nutritionnelles (cf. 3.6).

### 3.3 Recherche ✅
- Une seule barre de recherche, résultats **dès 2 caractères**, insensible aux accents et à la casse, tolérante aux fautes de frappe (recherche floue).
- Ordre des résultats :
  1. Aliments récents et favoris de l'utilisatrice
  2. Aliments personnels et recettes
  3. Ciqual
  4. Open Food Facts (triés par popularité, produits vendus en France en priorité)
- Chaque résultat affiche : nom, marque, kcal / 100 g, portion par défaut, icône de source.
- Onglets rapides sous la barre : **Récents · Favoris · Mes aliments · Recettes · Repas**.
- Filtre pour masquer les produits OFF incomplets (sans calories).

### 3.4 Saisie des quantités ✅
Unités possibles pour un aliment :
| Unité | Exemple | Origine |
|---|---|---|
| Grammes | 125 g | Toujours |
| Millilitres | 250 ml | Liquides ; conversion avec densité (1 par défaut, modifiable par produit) |
| Portion | 1 portion (30 g) | `serving_size` d'OFF ou portion définie par l'utilisatrice |
| Pièce / unité | 2 œufs, 1 banane | Table de poids moyens intégrée (œuf ≈ 50 g, banane ≈ 120 g…) + poids par pièce personnalisable |
| Cuillère, tasse, tranche… | 1 c. à soupe d'huile | Portions définies par l'utilisatrice [V1] |

Ergonomie :
- **Quantité pré-remplie automatiquement** avec la dernière quantité utilisée pour cet aliment (sinon la portion par défaut, sinon 100 g).
- Pavé numérique direct + boutons de raccourci (×½, ×1, ×2, +10 g).
- Aperçu en temps réel des kcal et macros pendant la saisie, et de l'impact sur le budget du jour.
- Validation en **1 tap** ; l'aliment est ajouté au repas en cours (déterminé par l'heure).

### 3.5 Structure du journal ✅
- **Repas configurables** : par défaut *Petit-déjeuner, Déjeuner, Dîner, Collation*. Renommer, réordonner, ajouter, supprimer ; plage horaire par repas (pour pré-sélectionner le bon repas à l'ajout).
- Navigation par jour (balayage gauche/droite, sélecteur de date).
- Par repas : total kcal + macros ; par jour : total, restant, barre de progression des macros.
- Chaque entrée : modifier la quantité, déplacer vers un autre repas, dupliquer, supprimer (balayage).
- Bouton **Journée terminée** (sert à l'algorithme adaptatif, cf. 2.3).
- Note libre par jour. [V1]

### 3.6 Fonctions de saisie rapide ✅
| Fonction | Description | Priorité |
|---|---|---|
| **Ajout rapide** | Saisir kcal (et facultativement P/G/L) avec un libellé optionnel (« Resto italien ») | MVP |
| **Copier** | Copier un repas ou une journée entière depuis une autre date, ou vers une autre date | MVP |
| **Repas enregistrés** | Sauvegarder un groupe d'aliments (« Mon petit-déj ») ; ajout en un tap, avec possibilité de décocher des éléments | MVP |
| **Recettes maison** | Ingrédients + quantités, nombre de portions **ou** poids final cuit (pour peser sa part) ; valeurs calculées automatiquement | V1 |
| **Favoris** | Étoile sur un aliment | MVP |

### 3.7 Aliments personnels ✅
- Création manuelle : nom, marque, code-barres optionnel, valeurs pour 100 g/ml (les 8 nutriments de 2.5), portions.
- Création depuis l'étiquette scannée (cf. étape 4).
- Correction d'un produit OFF : la version corrigée devient un aliment personnel qui **masque** l'original.

### 3.8 Modèle de données (aperçu)
```text
Food        { id, source: Ciqual|OFF|Custom|Recipe, source_ref, barcode?, name, brand?,
              per_100: Nutrients, density?, unit_weight_g?, portions[], updated_at }
Nutrients   { kcal, protein_g, carbs_g, sugars_g, fat_g, sat_fat_g, fiber_g, salt_g }
Portion     { label, grams }
MealSlot    { id, name, order, time_start?, time_end? }
LogEntry    { id, date, meal_slot_id, food_id?, label, quantity, unit, grams,
              nutrients_snapshot: Nutrients, created_at, hlc }   // snapshot = l'historique ne change pas si la fiche change
QuickAdd    = LogEntry sans food_id
SavedMeal   { id, name, items: [ { food_id, quantity, unit } ] }
Recipe      { id, name, ingredients: [ { food_id, grams } ], servings?, cooked_weight_g? }
Activity    { id, date, source: Manual|Strava, type, duration_min, kcal, external_id? }
WeighIn     { id, date, weight_kg, body_fat_pct? }
```
Tous les objets synchronisés ont un `id` UUID v7, un `hlc` et un indicateur `deleted` (tombstone).

## 4. Scan (code-barres et étiquette nutritionnelle)

### 4.1 Plateformes ✅
Le scan est **réservé au mobile** (Android, iOS). Sur PC et web : recherche et saisie manuelle uniquement ; le bouton de scan n'y apparaît pas.

### 4.2 Scan de code-barres [MVP] ✅
- Plugin officiel **`tauri-plugin-barcode-scanner`** ; formats EAN-13, EAN-8, UPC-A, UPC-E.
- Accès : bouton de scan visible sur le dashboard et dans l'écran d'ajout d'aliment (1 tap depuis n'importe quel repas).
- Parcours :
  1. Caméra plein écran, viseur rectangulaire (angles droits), lampe torche activable.
  2. Code détecté → vibration courte → recherche **cache local → Open Food Facts**.
  3. Produit trouvé → écran de quantité (cf. 3.4) avec la photo du produit.
  4. Produit trouvé mais **valeurs incomplètes** → proposer de scanner l'étiquette pour compléter.
  5. Produit inconnu → parcours 4.4.
- **Mode rafale** [V1] : scanner plusieurs produits à la suite (ex. en déballant les courses) pour les ajouter au repas ou aux favoris.
- Hors ligne : recherche dans le cache ; sinon, le code est mis en file d'attente et résolu au retour du réseau.

### 4.3 Scan de l'étiquette nutritionnelle [V1] ✅
**Moteur : OCR sur l'appareil, IA de vision en option.**

1. **OCR local (par défaut, gratuit, hors ligne)**
   - iOS : Apple Vision (`VNRecognizeTextRequest`, langue fr + en).
   - Android : Google ML Kit Text Recognition v2 (modèle latin embarqué, sans Google Play Services obligatoire).
   - Exposé via un **plugin Tauri maison** `tauri-plugin-label-ocr` (Swift + Kotlin) qui renvoie les blocs de texte avec leurs coordonnées.
2. **Analyse du tableau en Rust** (crate `core`, testable sur un corpus de photos) :
   - Repère les libellés FR/EN (et DE, ES, IT en V1) : *Énergie, Matières grasses, dont acides gras saturés, Glucides, dont sucres, Fibres alimentaires, Protéines, Sel*.
   - Repère les colonnes **« pour 100 g / 100 ml »** et **« par portion »** ; privilégie 100 g et récupère la taille de portion si elle est indiquée.
   - Gère : virgule décimale, `kJ` et `kcal` sur la même ligne, `< 0,5 g`, `traces`, sodium → sel (× 2,5).
   - **Contrôles de cohérence** : `kcal ≈ 4×P + 4×G + 9×L + 2×Fibres` (± 15 %) ; `kJ ≈ kcal × 4,184` ; sous-catégories ≤ catégories parentes (sucres ≤ glucides, saturés ≤ lipides).
3. **IA de vision (optionnelle)** : si la lecture locale échoue ou est incohérente, bouton *Analyser avec l'IA*.
   - L'utilisatrice fournit **sa propre clé API** (fournisseur au choix : Anthropic, OpenAI, Google), stockée dans le coffre sécurisé de l'appareil (Keychain / Android Keystore), jamais synchronisée.
   - Réponse demandée au format JSON strict (les 8 nutriments + portion), puis mêmes contrôles de cohérence.
   - Coût et envoi de la photo affichés avant le premier usage.
4. **Écran de validation (toujours)** : valeurs pré-remplies et modifiables, champs douteux surlignés, photo de l'étiquette consultable en zoom. Rien n'est enregistré sans validation.

### 4.4 Produit inconnu et contribution à Open Food Facts [V1] ✅
1. Code-barres inconnu → « Produit introuvable. Le créer ? »
2. Photo de face (nom, marque) + scan de l'étiquette (4.3) → validation → enregistrement comme **aliment personnel** lié au code-barres.
3. Proposition **facultative** : « Partager ce produit avec Open Food Facts ? »
   - Envoi : code-barres, nom, marque, quantité, valeurs nutritionnelles, photos face et étiquette.
   - Avec le compte OFF de l'utilisatrice (renseigné dans les paramètres) ; sinon, contribution anonyme via le compte de l'application, si OFF l'autorise.
   - Option *Toujours proposer / Ne plus demander*.
   - Envoi mis en file d'attente si hors ligne.

### 4.5 Photo de plat avec estimation par IA [Later] ✅
Hors MVP et V1. À réévaluer après la V1 (même principe de clé API personnelle et de validation obligatoire).

## 5. Dashboards et suivi

### 5.1 Navigation ✅
| Mobile (barre en bas) | PC / Web (rail latéral gauche) |
|---|---|
| **Aujourd'hui** · **Progrès** · [ **+** ] · **Aliments** · **Réglages** | Même ordre, en colonne ; le contenu s'étale sur 2 à 3 colonnes |

- Le bouton central **+** ouvre une feuille d'action : *Rechercher un aliment · Scanner (mobile) · Ajout rapide · Repas enregistré · Pesée · Activité*.
- Toute action courante est faisable en **3 taps maximum** depuis l'accueil.

### 5.2 Accueil « Aujourd'hui » ✅
De haut en bas (mobile) :
1. **Sélecteur de date** (balayage pour changer de jour).
2. **Carte héro** : grand chiffre **kcal restantes** ; dessous, en petit : `cible − mangé + activité = restant`. Jauge linéaire (angles droits) de la progression.
3. **Jauges macros** : protéines, glucides, lipides (grammes mangés / cible) ; fibres, sucres, sel accessibles d'un tap (« Plus de nutriments »).
4. **Repas du jour** : une carte par repas avec total kcal, liste des aliments, bouton « + » propre au repas.
5. **Activités du jour** (si présentes) : source (Strava / manuel), kcal ajoutées au budget.
6. **Mini-carte poids** : poids de tendance, variation sur 7 jours, bouton de pesée rapide.
7. Bouton **Journée terminée** en bas.

Sur PC / web : carte héro + macros à gauche, repas au centre, poids et activités à droite.

### 5.3 Écran « Progrès » ✅
Sélecteur de période commun : **7 j · 30 j · 90 j · 1 an · Tout**.

| Graphique | Contenu | Priorité |
|---|---|---|
| **Poids** | Points des pesées brutes, ligne de tendance, ligne d'objectif, projection en pointillés jusqu'à la date estimée | MVP |
| **Calories vs cible** | Barres par jour (ou par semaine au-delà de 90 j), ligne de cible, moyenne de la période | MVP |
| **Macros et nutriments** | Répartition P/G/L en % (barres empilées), moyennes fibres, sucres, saturés, sel vs repères | V1 |
| **TDEE estimé** | Évolution de la dépense calculée, avec les changements de cible marqués | V1 |
| **Mensurations** | Une courbe par mesure | V1 |

- Interaction : toucher ou survoler un point affiche la valeur exacte et la date ; toucher un jour ouvre le journal de ce jour.
- Graphiques dessinés en **SVG par des composants Leptos maison** (pas de librairie JS) pour maîtriser le style (angles droits, verre, accent unique).

### 5.4 Budget et bilan hebdomadaire ✅
- La cible est **quotidienne** ; les écarts ne sont **pas reportés** sur les jours suivants.
- **Carte « Bilan de la semaine »** (dans *Progrès*, et en tête de l'accueil le jour du recalcul) :
  - calories moyennes vs cible moyenne, écart cumulé ;
  - jours journalisés / 7 ;
  - variation du poids de tendance ;
  - nouvelle cible adaptative et explication du changement (cf. 2.3).

### 5.5 Notifications ✅
**Toutes désactivées par défaut.** Activables dans *Réglages → Rappels* (notifications locales via `tauri-plugin-notification`, aucun serveur) :
| Rappel | Réglage |
|---|---|
| Pesée | Heure, jours de la semaine |
| Repas oublié | Par repas, déclenché après la fin de sa plage horaire |
| Bilan hebdomadaire | Jour et heure |

Le changement de cible adaptative est toujours signalé **dans l'app** (carte), même sans notification.

### 5.6 Export des données [V1]
- Export CSV (journal, pesées, activités) et JSON (sauvegarde complète réimportable).
- Import JSON pour restaurer ou migrer.

## 6. Design system

### 6.1 Principes ✅
1. **Luxe calme** : beaucoup d'espace, peu d'éléments par écran, une information principale par carte.
2. **Verre liquide** : surfaces translucides floutées posées sur un fond neutre animé de lumières douces.
3. **Angles droits partout** : `border-radius: 0` sans exception (cartes, boutons, champs, jauges, modales, viseur de scan, avatars, tooltips).
4. **Un seul accent** : le **bleu Klein**, utilisé avec parcimonie (chiffre clé, action principale, progression).
5. **Les chiffres sont les héros** : grands, fins, tabulaires.
6. Références visuelles : `docs/design/references/` (les 4 images fournies).

### 6.2 Thèmes ✅
Sombre et clair, bascule manuelle (*Système · Clair · Sombre*) ; défaut : *Système*.

### 6.3 Couleurs (tokens) ✅
| Token | Clair | Sombre | Usage |
|---|---|---|---|
| `bg` | `#F2F0EB` (gris-crème) | `#0A0A0C` (noir profond) | Fond de l'app |
| `text-primary` | `#0E0E10` | `#F4F4F2` | Texte principal |
| `text-secondary` | `#0E0E10` à 60 % | `#F4F4F2` à 60 % | Libellés, unités |
| `glass-fill` | blanc à 55 % | blanc à 7 % | Fond des panneaux de verre |
| `glass-stroke` | blanc à 70 % | blanc à 14 % | Filet de 1 px des panneaux |
| `accent` | `#002FA7` (**bleu Klein**) | `#4D74FF` (bleu Klein éclairci) | Chiffre clé, bouton principal, jauges |
| `accent-on` | `#FFFFFF` | `#FFFFFF` | Texte sur fond d'accent |
| `accent-tint-1/2/3` | accent à 100 / 60 / 30 % | idem | Protéines / glucides / lipides |
| `over` | `#B4541E` | `#E08A52` | Dépassement de cible (orange sourd, jamais rouge vif) |

Contrastes vérifiés (WCAG) :
- Clair : `#002FA7` sur `#F2F0EB` = **9,4:1** ; blanc sur `#002FA7` = **10,7:1** (AAA).
- Sombre : le bleu Klein pur (`#002FA7`) ne fait que 1,9:1 sur noir, il est donc **réservé aux grandes surfaces de remplissage** ; texte et traits utilisent `#4D74FF` = **5,0:1** (AA).

Macros : trois **teintes du même bleu** (100 / 60 / 30 %) + libellé texte, pas de code couleur multicolore. Fibres, sucres, sel : neutres (`text-secondary`).

### 6.4 Typographie ✅
- Police : **Urbanist** (licence OFL), embarquée dans l'app (aucun chargement réseau). Alternative si les chiffres tabulaires manquent : **Outfit**.
- Chiffres : `font-variant-numeric: tabular-nums` (les colonnes ne bougent pas quand les valeurs changent).

| Style | Taille / graisse | Usage |
|---|---|---|
| `display` | 72 px / 200 | Kcal restantes (carte héro) |
| `h1` | 34 px / 300 | Titres d'écran |
| `h2` | 22 px / 400 | Titres de carte |
| `metric` | 28 px / 300 | Chiffres secondaires (poids, macros) |
| `body` | 16 px / 400 | Texte courant |
| `label` | 12 px / 500, majuscules, interlettrage +8 % | Libellés de carte (« MÉTABOLISME », « GLUCIDES ») |

Respect de la taille de texte système (Dynamic Type / échelle Android) jusqu'à 200 %.

### 6.5 Verre liquide (recette) ✅
```css
.glass {
  background: var(--glass-fill);
  backdrop-filter: blur(24px) saturate(160%);
  -webkit-backdrop-filter: blur(24px) saturate(160%);
  border: 1px solid var(--glass-stroke);
  border-radius: 0;
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,.35),   /* arête lumineuse en haut */
    0 20px 40px -20px rgba(0,0,0,.35);      /* ombre portée douce */
}
```
- **Fond** : `bg` uni + 3 à 4 **taches de lumière floues** (une bleu Klein, les autres neutres) qui dérivent très lentement + **grain** léger (texture bruit à 3–4 %).
- **Reflet spéculaire** : dégradé linéaire semi-transparent sur chaque panneau, dont l'angle suit l'**inclinaison du téléphone** (gyroscope) ou la **souris** sur PC/web.
- Lisibilité : tout texte sur verre doit garder un contraste ≥ 4,5:1 ; sinon l'opacité du verre augmente localement.
- **Performance** : 4 couches de verre superposées au maximum ; mode **« Effets réduits »** automatique sur appareils lents ou batterie faible (verre remplacé par des surfaces opaques), réglable à la main.

### 6.6 Animations (niveau riche) ✅
| Élément | Animation |
|---|---|
| Kcal restantes | Chiffres qui défilent (roulement) à chaque ajout |
| Jauges | Remplissage avec léger rebond (spring) |
| Navigation | Glissement + flou progressif entre écrans |
| Feuille d'action « + » | Monte depuis le bas, le fond se floute davantage |
| Reflets | Suivent le gyroscope / la souris (cf. 6.5) |
| Profondeur | Léger effet parallaxe entre le fond lumineux et les panneaux |
| Retour haptique | Ajout d'aliment, scan réussi, objectif atteint (`tauri-plugin-haptics`) |

- Durées : 180–320 ms ; courbes `cubic-bezier(.2,.8,.2,1)` et ressorts.
- Si le système demande de **réduire les animations** : tout passe en fondu simple, plus de parallaxe ni de gyroscope.
- Toutes les animations à 60 fps minimum (transform/opacity uniquement, pas d'animation de layout).

### 6.7 Composants et grille ✅
- Grille de **4 px** ; marges d'écran 20 px (mobile), 32 px (PC).
- Icônes : **Phosphor**, graisse *Light* (trait fin, licence MIT).
- Boutons : principal = fond accent plein ; secondaire = verre + filet ; tertiaire = texte seul. Hauteur 48 px (zone tactile ≥ 44 px).
- Champs : soulignés ou encadrés d'un filet de 1 px, jamais arrondis.
- Jauges : barres linéaires fines (4 px) ou arcs de cercle ; segments à angles droits.
- Composants à maquetter en priorité : carte héro, carte repas, ligne d'aliment, écran de quantité, feuille « + », viseur de scan, carte poids, graphiques.

## 7. Architecture technique et synchronisation

### 7.1 Stack ✅
| Couche | Choix | Pourquoi |
|---|---|---|
| Shell multiplateforme | **Tauri 2** | Un seul code pour Windows, macOS, Linux, Android, iOS ; mobile stable ; plugins officiels (caméra, code-barres, système de fichiers) |
| Interface | **Leptos** (Rust → WebAssembly) | 100 % Rust ; rendu en HTML/CSS, donc effet verre liquide natif via CSS ; la même UI sert la version web |
| Cœur métier | Crate Rust `core` partagée | Calculs nutritionnels, modèles de données, logique de sync — testable sans UI |
| Stockage local | **SQLite** (`rusqlite` ou `sqlx`) côté natif ; **IndexedDB / SQLite-wasm (OPFS)** côté web | Fonctionnement hors ligne complet |

Organisation du dépôt envisagée (workspace Cargo) :
```
Fuck-Calorie/
├─ crates/
│  ├─ core/        # modèles, calculs (BMR, TDEE, macros), règles métier
│  ├─ storage/     # SQLite / IndexedDB, migrations
│  ├─ sync/        # journal d'événements, fusion, connecteurs cloud
│  └─ food-db/     # client Open Food Facts, cache local
├─ app/            # Leptos (UI) + src-tauri (shell natif)
└─ docs/           # ce cahier des charges, maquettes
```

### 7.2 Synchronisation : sans serveur, via fichier cloud ✅
Principe : **local-first**. L'application fonctionne entièrement hors ligne ; la synchronisation se fait par le cloud personnel de l'utilisateur, sans serveur à héberger.

**Fournisseur** ✅ : **Google Drive**, dossier caché `appDataFolder` (invisible pour l'utilisateur, gratuit, accessible par API depuis Android, iOS, PC **et web**).
- iCloud est écarté comme fournisseur principal : inaccessible depuis Android et difficile depuis le web.
- Abstraction `SyncProvider` dans la crate `sync`, pour ajouter plus tard WebDAV (Nextcloud), Dropbox ou OneDrive. [Later]

**Modèle de données de sync** ✅ (règle la fragilité habituelle de la sync par fichier) :
- Chaque appareil écrit **uniquement son propre fichier** de journal d'événements (`events-<device_id>.jsonl`, append-only, compacté en snapshots). Deux appareils n'écrivent donc jamais dans le même fichier : aucun conflit d'écriture.
- Chaque modification est un événement horodaté par **horloge logique hybride (HLC)**.
- À la sync, chaque appareil lit les journaux des autres et fusionne : **dernier écrivain gagnant par champ** ; les suppressions sont des « tombstones ».
- Sync déclenchée : à l'ouverture, après chaque modification (avec un délai de quelques secondes), et périodiquement en arrière-plan.
- **Pas de chiffrement de bout en bout** ✅ : les fichiers sont dans le dossier caché `appDataFolder` du Drive de l'utilisatrice, accessible uniquement par l'app et par elle. Le format reste prêt à recevoir un chiffrement plus tard (champ `encryption: none` dans l'en-tête des fichiers).

**Limites acceptées** : délai de sync de quelques secondes à quelques minutes ; un compte Google est nécessaire pour la sync (l'app reste utilisable sans).

### 7.3 Authentification et intégrations ✅
**Pas de compte Fuck Calorie** : l'app fonctionne sans inscription. Seuls des services tiers optionnels demandent une connexion.

| Service | Mécanisme | Stockage des jetons | Priorité |
|---|---|---|---|
| Google Drive | OAuth 2.0 + PKCE (mobile, PC) ; Google Identity Services (web) ; scope **`drive.appdata` uniquement** | Coffre sécurisé de l'appareil (Keychain / Keystore / Credential Manager) ; `sessionStorage` sur le web | MVP |
| Strava | OAuth 2.0 avec **les identifiants de l'app Strava de l'utilisatrice** (cf. ci-dessous) | Coffre sécurisé | V1 |
| Open Food Facts (écriture) | Identifiant + mot de passe OFF de l'utilisatrice | Coffre sécurisé | V1 |
| IA de vision | Clé API de l'utilisatrice | Coffre sécurisé, jamais synchronisée | V1 |

**Google** : un projet Google Cloud au nom de Fuck Calorie fournit l'identifiant client (public, sans secret grâce à PKCE). En MVP, mode *test* (100 comptes max, suffisant pour l'usage perso) ; avant la V1 publique, **vérification de l'app par Google** (écran de consentement, politique de confidentialité publiée).

**Strava — clé de chaque utilisatrice** :
1. *Réglages → Strava* affiche un guide pas à pas : créer une app sur `strava.com/settings/api`, renseigner `localhost` comme *Authorization Callback Domain*, copier **Client ID** et **Client Secret** dans Fuck Calorie.
2. Redirection : serveur local de rappel sur `http://localhost:<port>` (PC) ; schéma d'URL `fuckcalorie://localhost/strava` (mobile) — **à valider en prototype**.
3. Web : l'échange de jetons Strava depuis un navigateur peut être bloqué (CORS) → **Strava non garanti sur la version web** ; les activités importées depuis un autre appareil y apparaissent quand même grâce à la sync.
4. Import : activités des 30 derniers jours à la connexion, puis nouvelles activités à chaque ouverture (limite Strava : 100 requêtes / 15 min par app, largement suffisant).
5. Les données Strava restent privées et ne sont jamais envoyées ailleurs (conformité aux conditions de l'API Strava).

### 7.4 Plugins et crates principales 💡
| Besoin | Choix |
|---|---|
| UI | `leptos` (mode CSR), `leptos_router` |
| Shell natif | `tauri` 2, `tauri-plugin-barcode-scanner`, `tauri-plugin-notification`, `tauri-plugin-haptics`, `tauri-plugin-deep-link`, `tauri-plugin-http`, `tauri-plugin-store` |
| Plugin maison | `tauri-plugin-label-ocr` (Swift / Kotlin) |
| Stockage | `rusqlite` (natif), `sqlite-wasm` via OPFS ou `indexed_db` (web) |
| HTTP | `reqwest` (natif), `gloo-net` (web) derrière un trait commun |
| Dates / IDs | `time` ou `chrono`, `uuid` (v7) |
| Recherche floue | `nucleo` ou `strsim` + index FTS5 SQLite |
| Coffre sécurisé | `keyring` (PC) + `tauri-plugin-stronghold` ou API natives (mobile) |
| Tests | `cargo test` (core), `wasm-bindgen-test` (web), corpus de photos d'étiquettes pour l'OCR |

### 7.5 Hébergement et livraison ✅
- **Web** : Leptos compilé en WebAssembly (build `trunk`), **site statique sur GitHub Pages**, déployé automatiquement par GitHub Actions à chaque tag de version. PWA installable (manifest + service worker pour le hors ligne).
- **CI GitHub Actions** : lint (`clippy`, `rustfmt`), tests, puis builds Windows (`.msi`), macOS (`.dmg`), Linux (`.AppImage`), Android (`.apk` / `.aab`), iOS (runner macOS).
- **Releases** : binaires attachés aux GitHub Releases ; Play Store / F-Droid / App Store en V1.
- **Coûts à prévoir** : Apple Developer Program **99 $/an** (obligatoire pour TestFlight et l'App Store ; en MVP, installation perso possible avec un compte gratuit mais à re-signer tous les 7 jours) ; Google Play **25 $ une fois**. Tout le reste est gratuit.
- Licence du code : **MPL-2.0** (cf. 8.10).

---

## 8. Exigences non fonctionnelles

### 8.1 Confidentialité et RGPD ✅
- Les données de poids et d'alimentation sont des **données de santé** (catégorie particulière, art. 9 RGPD). Principe retenu : **le projet n'en traite aucune** — elles restent sur les appareils et dans le Drive de l'utilisatrice.
- **Aucune télémétrie, aucun traceur, aucune publicité, aucune analytics**.
- Seuls appels réseau sortants : Open Food Facts (recherche/lecture, et écriture si contribution), Google Drive, Strava, fournisseur d'IA choisi — **chacun listé dans l'écran *Confidentialité*** avec ce qui est envoyé.
- **Politique de confidentialité** publiée sur le site GitHub Pages (requise par Google et les stores).
- *Réglages → Données* : **Exporter tout** (JSON + CSV) et **Tout supprimer** (local + fichiers Drive, avec double confirmation).
- Mention visible : « Fuck Calorie n'est pas un dispositif médical. Demandez conseil à un professionnel de santé avant un changement important d'alimentation. »

### 8.2 Hors ligne ✅
- 100 % des fonctions marchent hors ligne, sauf : recherche de produits OFF jamais consultés, contribution OFF, import Strava, IA de vision, sync.
- Les actions réseau sont mises en **file d'attente** et rejouées au retour de la connexion.
- Version web : PWA avec service worker ; données dans OPFS/IndexedDB.

### 8.3 Performance ✅
| Mesure | Cible |
|---|---|
| Démarrage à froid (mobile milieu de gamme) | < 2 s jusqu'à l'accueil utilisable |
| Recherche locale (Ciqual + cache) | < 100 ms par frappe |
| Recherche Open Food Facts | < 1,5 s (résultats locaux affichés pendant l'attente) |
| Scan code-barres → écran de quantité (produit en cache) | < 1 s |
| Ajouter un aliment connu | < 10 s, ≤ 4 taps |
| Animations | 60 fps (120 fps sur écrans compatibles) |
| Taille de l'app mobile | < 30 Mo |
| Bundle web (WASM + assets, compressé) | < 3 Mo |
| Sync d'une journée de modifications | < 3 s |

### 8.4 Fiabilité des données ✅
- Écritures SQLite en **transactions** ; aucune perte si l'app est tuée pendant une saisie.
- **Migrations de schéma versionnées** et testées (montée de version sans perte).
- **Sauvegarde locale automatique** hebdomadaire (4 dernières conservées).
- Sync **idempotente** : rejouer un journal deux fois ne change rien ; fusion testée par tests de propriétés (`proptest`) : quel que soit l'ordre de réception, tous les appareils convergent vers le même état.

### 8.5 Sécurité ✅
- Jetons et clés API uniquement dans le **coffre sécurisé** de la plateforme ; jamais dans SQLite, les logs ou la sync.
- Aucun secret dans le dépôt (identifiant client Google public uniquement).
- CI : `cargo audit` et `cargo deny` (vulnérabilités, licences des dépendances) à chaque PR.
- Content Security Policy stricte côté web et dans la webview Tauri.

### 8.6 Accessibilité — WCAG 2.2 AA ✅
- Contraste texte ≥ 4,5:1 (≥ 3:1 pour gros texte et éléments graphiques), y compris sur le verre (cf. 6.5).
- Lecteurs d'écran : **VoiceOver, TalkBack, NVDA/Narrator** ; chaque jauge et graphique a une alternative textuelle (« Protéines : 92 g sur 130 g »).
- Navigation complète **au clavier** sur PC/web, focus visible (contour carré 2 px accent) ; raccourcis : `N` ajouter, `/` rechercher, `←/→` changer de jour, `W` pesée.
- Zones tactiles ≥ 44 × 44 px ; texte agrandissable jusqu'à 200 % sans perte.
- L'information ne repose jamais sur la couleur seule (dépassement = couleur + icône + texte).
- *Réduire les animations* respecté (cf. 6.6).

### 8.7 Langues ✅
- **Français** au MVP, **anglais** en V1.
- Textes externalisés au format **Fluent** (`fluent-rs` / `leptos-fluent`), fichiers `locales/fr/*.ftl`, `locales/en/*.ftl` ; ajout de langues par la communauté sans toucher au code.
- Formats localisés (décimales, dates, premier jour de semaine) ; unités métriques par défaut, impériales en option.
- Noms d'aliments : Ciqual en français (et anglais, la table le fournit), OFF dans la langue disponible.

### 8.8 Télémétrie et diagnostic ✅
- **Aucune** télémétrie ni rapport de plantage automatique.
- **Logs locaux** rotatifs (7 jours, sans données nutritionnelles nominatives) exportables depuis *Réglages → Aide → Exporter les logs*, à joindre à une issue GitHub.

### 8.9 Qualité du code ✅
- Couverture de tests de la crate `core` ≥ 80 % (calculs BMR/TDEE, algorithme adaptatif, parsing d'étiquettes, fusion de sync).
- `clippy` sans avertissement, `rustfmt` imposé.
- Commits conventionnels, `CHANGELOG.md`, versionnage SemVer.
- `README.md`, `CONTRIBUTING.md`, modèles d'issues et de PR.

### 8.10 Licences ✅
| Élément | Licence |
|---|---|
| Code de Fuck Calorie | **MPL-2.0** |
| Open Food Facts (données) | ODbL — attribution obligatoire |
| Ciqual (ANSES) | Licence Ouverte Etalab — attribution |
| Urbanist (police) | SIL OFL 1.1 |
| Phosphor (icônes) | MIT |

Écran *À propos → Licences* généré automatiquement (`cargo about`).

## 9. Roadmap
❓ *Étape 9.*

---

## Journal des décisions
| Date | Décision |
|---|---|
| 2026-11-15 | Public : usage perso d'abord, publication ensuite |
| 2026-11-15 | Stack : Tauri 2 + Leptos, 100 % Rust |
| 2026-11-15 | Sync : sans serveur, via fichier cloud (Google Drive appDataFolder proposé) |
| 2026-11-15 | CDC en Markdown dans le dépôt (`docs/`) |
| 2026-11-15 | Cible calorique adaptative (TDEE réel), estimation Mifflin-St Jeor au démarrage |
| 2026-11-15 | Modes : perte, maintien, prise de masse, recomposition |
| 2026-11-15 | Nutriments : calories, macros, fibres, sucres, saturés, sel |
| 2026-11-15 | Activité : niveau fixe + saisie manuelle + import Strava |
| 2026-11-15 | Base produits : Open Food Facts + Ciqual ; Ciqual embarqué, cache OFF intelligent |
| 2026-11-15 | Repas configurables ; recettes, repas enregistrés, copie, ajout rapide |
| 2026-11-15 | Scan mobile uniquement ; OCR sur l'appareil + IA de vision optionnelle (clé perso) |
| 2026-11-15 | Produit inconnu : création locale + contribution facultative à Open Food Facts |
| 2026-11-15 | Photo de plat par IA : reportée (Later) |
| 2026-11-15 | Accueil centré sur les kcal restantes ; 4 graphiques de progrès |
| 2026-11-15 | Budget quotidien + bilan hebdo sans report ; notifications désactivées par défaut |
| 2026-11-15 | Design : bleu Klein, Urbanist, fond neutre + lumières floues, animations riches, angles droits |
| 2026-11-15 | Sync Google Drive (appDataFolder), sans chiffrement de bout en bout |
| 2026-11-15 | Strava via les identifiants d'app Strava de chaque utilisatrice (aucun serveur) |
| 2026-11-15 | Web statique hébergé sur GitHub Pages |
| 2026-11-15 | Licence MPL-2.0 ; FR (MVP) + EN (V1) ; WCAG 2.2 AA ; aucune télémétrie |
