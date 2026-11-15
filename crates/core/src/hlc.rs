//! Horloge logique hybride (HLC) et métadonnées de synchronisation (§7.2).
//!
//! Chaque modification synchronisée porte un [`Hlc`]. À la fusion, la valeur portant le plus
//! grand `Hlc` gagne (« dernier écrivain gagnant »). L'ordre est total : à horodatage et
//! compteur égaux, l'identifiant d'appareil départage, donc tous les appareils tranchent de la
//! même façon.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Horodatage logique hybride. L'ordre de comparaison est `(wall_ms, counter, device)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Hlc {
    /// Millisecondes depuis l'époque Unix (heure murale, éventuellement « poussée » en avant).
    pub wall_ms: u64,
    /// Compteur qui départage les événements d'une même milliseconde.
    pub counter: u32,
    /// Appareil qui a produit l'événement.
    pub device: Uuid,
}

/// Valeur maximale du compteur : 5 chiffres dans la forme texte.
const MAX_COUNTER: u32 = 99_999;

impl Hlc {
    /// Plus petit `Hlc` possible : tout événement réel lui est supérieur.
    pub const ZERO: Self = Self {
        wall_ms: 0,
        counter: 0,
        device: Uuid::nil(),
    };
}

/// Forme texte triable : `0001794758275067-00000-<uuid>`. L'ordre alphabétique de cette
/// chaîne est le même que l'ordre des `Hlc`, ce qui permet de comparer directement en SQL.
impl fmt::Display for Hlc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:016}-{:05}-{}",
            self.wall_ms,
            self.counter,
            self.device.as_hyphenated()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("horodatage HLC invalide : {0:?}")]
pub struct ParseHlcError(String);

impl FromStr for Hlc {
    type Err = ParseHlcError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseHlcError(s.to_owned());
        let mut parts = s.splitn(3, '-');
        let wall_ms = parts.next().and_then(|p| p.parse().ok()).ok_or_else(err)?;
        let counter = parts.next().and_then(|p| p.parse().ok()).ok_or_else(err)?;
        let device = parts
            .next()
            .and_then(|p| Uuid::parse_str(p).ok())
            .ok_or_else(err)?;
        Ok(Self {
            wall_ms,
            counter,
            device,
        })
    }
}

impl From<Hlc> for String {
    fn from(hlc: Hlc) -> Self {
        hlc.to_string()
    }
}

impl TryFrom<String> for Hlc {
    type Error = ParseHlcError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

/// Horloge d'un appareil. Elle garantit des horodatages **strictement croissants**, même si
/// l'heure du système recule ou si deux événements tombent dans la même milliseconde.
#[derive(Debug, Clone)]
pub struct HlcClock {
    last: Hlc,
}

impl HlcClock {
    /// Nouvelle horloge pour l'appareil `device`.
    #[must_use]
    pub fn new(device: Uuid) -> Self {
        Self {
            last: Hlc {
                wall_ms: 0,
                counter: 0,
                device,
            },
        }
    }

    /// Reprend une horloge là où elle s'était arrêtée (dernier `Hlc` émis, relu au démarrage).
    #[must_use]
    pub fn resume(last: Hlc) -> Self {
        Self { last }
    }

    #[must_use]
    pub fn device(&self) -> Uuid {
        self.last.device
    }

    /// Horodate un événement local. `now_ms` est l'heure système en millisecondes Unix.
    pub fn tick(&mut self, now_ms: u64) -> Hlc {
        if now_ms > self.last.wall_ms {
            self.last.wall_ms = now_ms;
            self.last.counter = 0;
        } else if self.last.counter >= MAX_COUNTER {
            // Compteur plein (horloge système bloquée ou très en retard) : on avance d'une
            // milliseconde pour que la forme texte reste triable.
            self.last.wall_ms += 1;
            self.last.counter = 0;
        } else {
            self.last.counter += 1;
        }
        self.last
    }

    /// Tient compte d'un événement reçu d'un autre appareil : l'horloge locale ne produira
    /// plus jamais d'horodatage inférieur à `remote`.
    pub fn observe(&mut self, remote: Hlc, now_ms: u64) {
        let wall = now_ms.max(self.last.wall_ms).max(remote.wall_ms);
        let counter = if wall == self.last.wall_ms && wall == remote.wall_ms {
            self.last.counter.max(remote.counter)
        } else if wall == self.last.wall_ms {
            self.last.counter
        } else if wall == remote.wall_ms {
            remote.counter
        } else {
            0
        };
        self.last.wall_ms = wall;
        self.last.counter = counter.min(MAX_COUNTER);
    }
}

/// Métadonnées communes à tous les objets synchronisés (§3.8) : identifiant UUID v7,
/// horodatage HLC de la dernière modification, et marqueur de suppression (« tombstone »).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncMeta {
    pub id: Uuid,
    pub hlc: Hlc,
    /// `true` = supprimé. L'objet est conservé pour que la suppression se propage.
    #[serde(default)]
    pub deleted: bool,
}

impl SyncMeta {
    /// Métadonnées d'un nouvel objet : identifiant UUID v7 (triable par date de création).
    #[must_use]
    pub fn new(hlc: Hlc) -> Self {
        Self {
            id: Uuid::now_v7(),
            hlc,
            deleted: false,
        }
    }

    /// Métadonnées avec un identifiant imposé (import, tests, identifiants déterministes).
    #[must_use]
    pub fn with_id(id: Uuid, hlc: Hlc) -> Self {
        Self {
            id,
            hlc,
            deleted: false,
        }
    }

    /// Marque l'objet comme modifié à l'instant `hlc`.
    pub fn touch(&mut self, hlc: Hlc) {
        self.hlc = hlc;
    }

    /// Marque l'objet comme supprimé à l'instant `hlc`.
    pub fn delete(&mut self, hlc: Hlc) {
        self.hlc = hlc;
        self.deleted = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn device(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn tick_is_strictly_increasing_even_if_time_goes_back() {
        let mut clock = HlcClock::new(device(1));
        let a = clock.tick(1_000);
        let b = clock.tick(1_000); // même milliseconde
        let c = clock.tick(900); // l'heure système a reculé
        let d = clock.tick(2_000);
        assert!(a < b && b < c && c < d);
        assert_eq!((b.wall_ms, b.counter), (1_000, 1));
        assert_eq!((c.wall_ms, c.counter), (1_000, 2));
        assert_eq!((d.wall_ms, d.counter), (2_000, 0));
        assert_eq!(clock.device(), device(1));
    }

    #[test]
    fn observe_moves_clock_past_remote() {
        let mut clock = HlcClock::new(device(1));
        clock.tick(1_000);
        // Un autre appareil a une horloge en avance de 10 s.
        let remote = Hlc {
            wall_ms: 11_000,
            counter: 7,
            device: device(2),
        };
        clock.observe(remote, 1_500);
        let next = clock.tick(1_600);
        assert!(next > remote);
        assert_eq!((next.wall_ms, next.counter), (11_000, 8));

        // Événement distant plus ancien : rien ne recule.
        clock.observe(
            Hlc {
                wall_ms: 5,
                counter: 0,
                device: device(2),
            },
            1_700,
        );
        assert!(clock.tick(1_800) > next);

        // Même milliseconde des deux côtés : on garde le plus grand compteur.
        let mut clock = HlcClock::resume(Hlc {
            wall_ms: 50,
            counter: 3,
            device: device(1),
        });
        clock.observe(
            Hlc {
                wall_ms: 50,
                counter: 9,
                device: device(2),
            },
            10,
        );
        assert_eq!(clock.tick(10).counter, 10);

        // L'heure locale est la plus récente : le compteur repart de zéro.
        let mut clock = HlcClock::new(device(1));
        clock.observe(
            Hlc {
                wall_ms: 50,
                counter: 9,
                device: device(2),
            },
            60,
        );
        assert_eq!(clock.tick(60).counter, 1);
    }

    #[test]
    fn counter_overflow_advances_the_wall_clock() {
        let mut clock = HlcClock::resume(Hlc {
            wall_ms: 500,
            counter: 99_998,
            device: device(1),
        });
        let a = clock.tick(100);
        let b = clock.tick(100);
        assert_eq!((a.wall_ms, a.counter), (500, 99_999));
        assert_eq!((b.wall_ms, b.counter), (501, 0));
        assert!(a < b && a.to_string() < b.to_string());
    }

    #[test]
    fn device_breaks_ties() {
        let a = Hlc {
            wall_ms: 10,
            counter: 0,
            device: device(1),
        };
        let b = Hlc {
            wall_ms: 10,
            counter: 0,
            device: device(2),
        };
        assert!(a < b);
        assert!(Hlc::ZERO < a);
    }

    #[test]
    fn text_round_trip_and_serde() {
        let hlc = Hlc {
            wall_ms: 1_794_758_275_067,
            counter: 42,
            device: device(0xABCD),
        };
        let text = hlc.to_string();
        assert_eq!(
            text,
            "0001794758275067-00042-00000000-0000-0000-0000-00000000abcd"
        );
        assert_eq!(text.parse::<Hlc>().unwrap(), hlc);
        let json = serde_json::to_string(&hlc).unwrap();
        assert_eq!(serde_json::from_str::<Hlc>(&json).unwrap(), hlc);
        assert!("n'importe quoi".parse::<Hlc>().is_err());
        assert!(
            "12-xx-00000000-0000-0000-0000-00000000abcd"
                .parse::<Hlc>()
                .is_err()
        );
        assert!(serde_json::from_str::<Hlc>("\"12-3\"").is_err());
    }

    #[test]
    fn sync_meta_lifecycle() {
        let mut clock = HlcClock::new(device(1));
        let mut meta = SyncMeta::new(clock.tick(100));
        assert!(!meta.deleted);
        assert_eq!(meta.id.get_version_num(), 7);
        let t2 = clock.tick(200);
        meta.touch(t2);
        assert_eq!(meta.hlc, t2);
        let t3 = clock.tick(300);
        meta.delete(t3);
        assert!(meta.deleted && meta.hlc == t3);
        let fixed = SyncMeta::with_id(device(9), t3);
        assert_eq!(fixed.id, device(9));
    }

    proptest! {
        /// L'ordre des formes texte est le même que l'ordre des `Hlc` (comparaison en SQL).
        #[test]
        fn text_order_matches_hlc_order(
            w1 in 0u64..9_999_999_999_999_999, c1 in 0u32..99_999, d1 in any::<u128>(),
            w2 in 0u64..9_999_999_999_999_999, c2 in 0u32..99_999, d2 in any::<u128>(),
        ) {
            let a = Hlc { wall_ms: w1, counter: c1, device: Uuid::from_u128(d1) };
            let b = Hlc { wall_ms: w2, counter: c2, device: Uuid::from_u128(d2) };
            prop_assert_eq!(a.cmp(&b), a.to_string().cmp(&b.to_string()));
            prop_assert_eq!(a.to_string().parse::<Hlc>().unwrap(), a);
        }

        /// Quoi qu'il arrive à l'heure système, chaque `tick` est supérieur au précédent.
        #[test]
        fn ticks_always_increase(times in proptest::collection::vec(0u64..10_000, 1..200)) {
            let mut clock = HlcClock::new(Uuid::from_u128(1));
            let mut previous = Hlc::ZERO;
            for t in times {
                let next = clock.tick(t);
                prop_assert!(next > previous);
                previous = next;
            }
        }
    }
}
