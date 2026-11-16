# Fuck Calorie — Avancement

> Suivi de ce qui est construit par rapport au [cahier des charges](CAHIER_DES_CHARGES.md).
> Dernière mise à jour : 2026-11-16.

Légende : ✅ fait et testé · 🟡 partiel · ⬜ à faire.

## Vue d'ensemble

| Brique | État | Détail |
|---|---|---|
| Socle du dépôt (workspace, licence, CI) | ✅ | CI : format, clippy, tests sous Linux et Windows |
| `fc-core` — cœur métier | ✅ | 78 tests |
| `fc-storage` — SQLite natif | ✅ | 21 tests ; version web (OPFS) à faire |
| `fc-service` — opérations de l'application | ✅ | 27 tests |
| `app/` — interface Leptos | 🟡 | profil, écran *Aujourd'hui*, recherche d'aliments, quantité, modification d'une ligne, copie, favoris, aliments personnels, repas enregistrés, ajout rapide, pesée, bilan de la semaine |
| `app/src-tauri` — application de bureau | 🟡 | Windows ; Android et iOS à faire |
| `fc-food-db` — Ciqual + Open Food Facts | ✅ | 16 tests ; Ciqual 2025 (3 341 aliments), client Open Food Facts ; miniatures des produits à faire |
| `fc-sync` — Google Drive | ⬜ | L'horloge HLC et les écritures idempotentes sont déjà en place |

## Par section du cahier des charges

| Section | État | Où |
|---|---|---|
| §2.1 Profil, historisation | ✅ | `core::profile`, `service`, écran de démarrage |
| §2.1 Modifier le profil après coup | ⬜ | écran *Réglages* à faire |
| §2.2 BMR Mifflin-St Jeor, TDEE initial | ✅ | `core::energy` |
| §2.3 Poids de tendance, algorithme adaptatif | ✅ | `core::trend`, `core::adaptive` |
| §2.3 Recalcul hebdomadaire, cible figée | ✅ | `service::weekly_review`, à la première ouverture de la semaine ; explication dans la carte *Bilan de la semaine* |
| §2.3 Notification du recalcul, jour de recalcul réglable | ⬜ | le recalcul suit le premier jour de la semaine (lundi) |
| §2.4 Modes d'objectif, projection | ✅ | `core::goal`, affichée dans la carte poids |
| §2.5 Macros et repères par défaut | ✅ | `core::macros`, jauges de la carte héro |
| §2.6 Activité : saisie manuelle, MET, calories Strava | 🟡 | calculs faits ; écran de saisie à faire |
| §2.7 Garde-fous santé | ✅ | `core::goal`, signalés dans la carte héro |
| §2.8 Pesées | ✅ | saisie depuis la carte poids |
| §3.1–3.2 Ciqual embarqué, client et cache Open Food Facts | ✅ | `fc-food-db`, `service` ; sans miniature ni rafraîchissement en arrière-plan |
| §3.3 Recherche | 🟡 | une barre, locale + Open Food Facts, fautes de frappe tolérées, onglets *Récents · Favoris · Mes aliments · Repas* ; onglet *Recettes* et filtre des fiches incomplètes à faire |
| §3.4 Quantités et unités | ✅ | `core::food`, écran de quantité (table des poids moyens par pièce à faire) |
| §3.5 Journal : repas, navigation par jour, modification (quantité, repas), suppression, journée terminée | ✅ | écran *Aujourd'hui* ; un clic sur une ligne l'ouvre |
| §3.5 Repas configurables (renommer, ajouter…), dupliquer une ligne, balayage, note du jour | ⬜ | |
| §3.6 Ajout rapide | ✅ | feuille d'ajout |
| §3.6 Copie, repas enregistrés, favoris | 🟡 | copie depuis la veille (un repas ou la journée), repas enregistrés (créer, ajouter, supprimer), favoris ; copie depuis ou vers une date au choix et décocher des éléments à faire |
| §3.6 Recettes maison | ⬜ | V1 |
| §3.7 Aliments personnels | 🟡 | création manuelle ; modification, code-barres et correction d'une fiche Open Food Facts à faire |
| §4 Scan | ⬜ | |
| §5.1 Navigation (barre / rail) | ⬜ | un seul écran pour l'instant |
| §5.2 Écran *Aujourd'hui* | 🟡 | sans activités du jour |
| §5.4 Budget quotidien, carte « Bilan de la semaine » | ✅ | en tête de l'accueil le jour du recalcul, sous la carte poids les autres jours (en attendant l'écran *Progrès*) |
| §5.3 Écran *Progrès* | ⬜ | |
| §6 Design system | 🟡 | tokens, verre, lumières, grain, Urbanist, clair/sombre, effets réduits ; reflets gyroscope/souris et chiffres qui défilent à faire |
| §7.2 Synchronisation | 🟡 | HLC, tombstones, écritures idempotentes ; journal d'événements et Drive à faire |
| §8.4 Transactions, migrations versionnées | ✅ | `fc-storage` (schéma version 2 : favoris) |
| §8.5 Content Security Policy | ⬜ | désactivée pendant le développement (`tauri.conf.json`) |
| §8.6 Raccourcis clavier ←/→, N et / | ✅ | écran *Aujourd'hui* |

## Écarts et précisions par rapport au cahier des charges

1. **Noms des crates.** Les paquets s'appellent `fc-core`, `fc-storage`… (dossiers
   `crates/core`, `crates/storage`). Un paquet nommé `core` entrerait en conflit avec la
   bibliothèque standard de Rust.
2. **Formule adaptative et jours manquants (§2.3).** La formule est appliquée « par jour » :
   apport moyen des jours journalisés − activités moyennes − variation quotidienne de la
   tendance × 7 700. C'est identique à la formule du cahier des charges quand les 21 jours
   sont journalisés ; quand il en manque, l'apport des jours manquants est supposé égal à la
   moyenne des autres (au lieu de zéro).
3. **Plancher calorique et activités (§2.7).** Le plancher s'applique à la cible de base,
   avant l'ajout des calories d'activité.
4. **Âge.** Le profil est refusé en dessous de 18 ans : la formule de Mifflin-St Jeor et les
   garde-fous du §2.7 ne sont pas valables pour les enfants et les adolescents. Le cahier des
   charges ne fixait pas d'âge minimum.
5. **Valeurs nutritionnelles facultatives.** Dans une fiche aliment, chaque nutriment peut
   être « inconnu » (fiches Open Food Facts incomplètes) ; dans le journal, un nutriment
   inconnu compte pour zéro.
6. **Version web.** Ouverte dans un navigateur, l'interface affiche pour l'instant un message
   renvoyant vers l'application de bureau : le stockage dans le navigateur (OPFS) n'est pas
   encore écrit.
7. **Séparateur des milliers.** Espace insécable normale plutôt que fine : Urbanist dessine
   l'espace fine presque invisible.

8. **Recherche Open Food Facts (§3.1, §3.3).** Le cahier des charges prévoit un anti-rebond
   de 300 ms, mais Open Food Facts n'accepte qu'une dizaine de recherches par minute : la
   recherche en ligne part après 700 ms sans frappe (ou tout de suite avec Entrée), dès
   3 caractères, et l'application se limite elle-même à 8 recherches par minute. La recherche
   locale, elle, répond à chaque frappe dès 2 caractères.
9. **Produits vendus en France (§3.3).** La recherche en ligne ne renvoie *que* les produits
   vendus en France (au lieu de les mettre « en priorité »), triés par popularité. Un produit
   étranger reste accessible par son code-barres.
10. **Contact dans le `User-Agent` (§3.1).** L'adresse du dépôt GitHub remplace l'e-mail de
    contact, pour ne pas publier d'adresse personnelle dans un dépôt public.
11. **Client HTTP (§7.4).** `reqwest` comme prévu, avec `rustls` et le fournisseur de
    chiffrement `ring` : celui par défaut demande CMake et NASM pour compiler sous Windows.
12. **Valeurs Ciqual.** « traces » et « < 0,5 » (sous le seuil de mesure) comptent pour zéro ;
    « - » reste inconnu. Les 143 aliments de la table sans valeur d'énergie ne sont pas
    embarqués. La table vient de la copie publiée dans le dépôt d'Open Food Facts (édition
    du 3 novembre 2025).
13. **Fiche sans calories.** Elle s'affiche dans la recherche (« calories inconnues ») mais ne
    peut pas être ajoutée au journal : l'écran renvoie vers l'ajout rapide, en attendant le
    scan d'étiquette (§4.3).
14. **Intégration continue sous Linux.** Ajout de l'installation des bibliothèques système de
    Tauri : sans elles, la compilation de l'application échouait sur le runner Ubuntu.

15. **Modifier une ligne.** Changer la quantité recalcule les nutriments à partir de la fiche
    *actuelle* de l'aliment. Un ajout rapide n'a pas de quantité : il peut seulement changer
    de repas.
16. **Copie.** Pour l'instant depuis la veille seulement (bouton « Copier d'hier » dans un repas
    vide, « Copier toute la journée d'hier » dans une journée vide).
17. **Repas enregistrés.** Ils se créent depuis la carte d'un repas du journal ; les ajouts
    rapides, qui n'ont pas de fiche, ne sont pas repris.

18. **Variation du poids dans l'algorithme adaptatif (§2.3).** Pente de la droite de
    régression des pesées de la fenêtre, au lieu de la variation du poids de tendance (voir
    les décisions ci-dessous). Il faut en plus que les pesées s'étalent sur au moins 7 jours.
19. **Recalcul hebdomadaire (§2.3).** Il se fait à la première ouverture de l'application
    dans la semaine (pas de tâche de fond), sur les 21 jours qui précèdent le premier jour de
    la semaine. La nouvelle dépense vaut à partir de ce jour : les jours passés gardent leur
    cible. Pas de bilan avant une première semaine complète d'utilisation.
20. **Réglages non synchronisés.** L'historique de la dépense estimée, le dernier bilan et
    « cible figée » sont pour l'instant des réglages de l'appareil ; ils devront passer dans
    les données synchronisées avec `fc-sync`.

## Décisions prises par défaut (16 novembre 2026), à revoir si besoin

- **Pente du poids par régression** (écart 18) : le cahier des charges mesurait la variation
  du poids sur le poids de tendance, qui a environ cinq semaines de retard sur une perte
  régulière. Avec des données parfaites (vraie dépense 2 300 kcal, apport 1 750 kcal), la
  dépense calculée valait 2 027 kcal en semaine 2 et 2 104 en semaine 3, avant de remonter :
  la cible aurait baissé à tort pendant le premier mois. La régression sur les pesées donne
  2 300 dès la troisième semaine. Contrepartie : elle est un peu plus sensible aux variations
  d'eau d'un jour à l'autre (de l'ordre de 100 kcal avec ± 0,6 kg de bruit), ce que
  l'amortissement de 100 kcal par semaine borne.
- **Recherche en ligne limitée à la France** (écart 9) : résultats plus pertinents pour un
  usage en France ; un réglage de pays viendra avec la version publique.
- **Contact du `User-Agent`** (écart 10) : l'adresse du dépôt GitHub.

## Prochaines étapes proposées

1. **Navigation** et écrans *Réglages* (modifier le profil) et *Progrès* (graphiques).
2. **Android** : build APK, scan de code-barres.
3. **`fc-sync`** : journal d'événements, fusion, Google Drive.
4. **Finitions du journal** : copie depuis une date au choix, repas configurables, modifier un
   aliment personnel, saisie manuelle d'une activité (§2.6).

## À vérifier à la main

Les phases « recherche d'aliments », « journal complet » et « recalcul hebdomadaire » ont été
compilées et testées (format, clippy, 147 tests), mais ni l'interface ni les appels réels à Open Food Facts n'ont
pu être lancés depuis l'environnement de développement. À essayer avec `cargo tauri dev` :

- chercher « pomme », puis « yahourt » (faute volontaire) : résultats Ciqual immédiats ;
- chercher « nutella » : des produits Open Food Facts apparaissent sous les résultats locaux ;
- ouvrir un produit, changer d'unité, ajouter : la ligne apparaît dans le repas, et l'aliment
  dans *Récents* à la prochaine ouverture ;
- couper le réseau : la recherche locale et les produits déjà ouverts fonctionnent toujours ;
- cliquer sur une ligne du journal : changer la quantité, puis le repas ;
- dans un repas vide, « Copier d'hier » ; dans un repas rempli, « Enregistrer ce repas », puis
  le retrouver dans l'onglet *Repas* de la feuille d'ajout ;
- mettre un aliment en favori, créer un aliment personnel (onglet *Mes aliments*) ;
- le bilan de la semaine n'apparaît qu'après une première semaine complète d'utilisation ;
  « Figer ma cible » puis « Libérer ma cible » changent le texte de la carte.
