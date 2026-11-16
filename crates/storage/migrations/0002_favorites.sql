-- Version 2 : aliments favoris (§3.6).
--
-- Une ligne par aliment marqué (ou démarqué) : l'étoile est un simple drapeau, synchronisé
-- comme le reste (la version la plus récente gagne).

CREATE TABLE favorites (
    food_id  TEXT PRIMARY KEY NOT NULL,
    hlc      TEXT NOT NULL,
    favorite INTEGER NOT NULL DEFAULT 1
) STRICT;
