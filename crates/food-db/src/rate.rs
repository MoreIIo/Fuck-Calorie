//! Limiteur de débit : Open Food Facts impose environ 100 lectures de produit et
//! 10 recherches par minute (§3.1). Les dépasser peut faire bloquer l'adresse IP de
//! l'utilisatrice : l'application s'arrête donc d'elle-même avant.

use std::collections::VecDeque;

/// Fenêtre glissante : au plus `max` appels sur `window_ms` millisecondes. L'heure est
/// passée en paramètre (testable sans horloge).
#[derive(Debug, Clone)]
pub struct RateLimiter {
    max: usize,
    window_ms: i64,
    calls: VecDeque<i64>,
}

impl RateLimiter {
    #[must_use]
    pub fn new(max: usize, window_ms: i64) -> Self {
        Self {
            max,
            window_ms,
            calls: VecDeque::with_capacity(max),
        }
    }

    /// Enregistre un appel à l'instant `now_ms` s'il est permis. Sinon, renvoie le délai
    /// d'attente en millisecondes avant le prochain appel possible.
    pub fn try_acquire(&mut self, now_ms: i64) -> Result<(), i64> {
        while self
            .calls
            .front()
            .is_some_and(|&t| now_ms - t >= self.window_ms || t > now_ms)
        {
            self.calls.pop_front();
        }
        if self.calls.len() < self.max {
            self.calls.push_back(now_ms);
            return Ok(());
        }
        let oldest = self.calls.front().copied().unwrap_or(now_ms);
        Err((oldest + self.window_ms - now_ms).max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sliding_window() {
        let mut limiter = RateLimiter::new(3, 60_000);
        assert_eq!(limiter.try_acquire(0), Ok(()));
        assert_eq!(limiter.try_acquire(10_000), Ok(()));
        assert_eq!(limiter.try_acquire(20_000), Ok(()));
        // Quatrième appel dans la minute : refusé, il faut attendre que le premier sorte de
        // la fenêtre.
        assert_eq!(limiter.try_acquire(30_000), Err(30_000));
        assert_eq!(limiter.try_acquire(59_999), Err(1));
        assert_eq!(limiter.try_acquire(60_000), Ok(()));
        assert_eq!(limiter.try_acquire(60_001), Err(9_999));
        // Un refus ne compte pas comme un appel.
        assert_eq!(limiter.try_acquire(70_000), Ok(()));
    }

    #[test]
    fn clock_going_backwards_does_not_block_forever() {
        let mut limiter = RateLimiter::new(1, 60_000);
        assert_eq!(limiter.try_acquire(1_000_000), Ok(()));
        // L'horloge de l'appareil a reculé : les anciens appels « du futur » sont oubliés.
        assert_eq!(limiter.try_acquire(5_000), Ok(()));
    }
}
