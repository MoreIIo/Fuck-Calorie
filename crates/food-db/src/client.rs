//! Client HTTP Open Food Facts (§3.1).
//!
//! - En-tête `User-Agent` : `FuckCalorie/<version> (<contact>)`.
//! - Débit limité par l'application elle-même, avec une marge sous les limites d'Open Food
//!   Facts (100 lectures de produit et 10 recherches par minute).
//! - Rien d'autre que le code-barres ou le texte recherché n'est envoyé (§8.1).

#[cfg(any(target_os = "android", test))]
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use fc_core::food::Food;

use crate::off::{self, OffError};
use crate::rate::RateLimiter;

const MINUTE_MS: i64 = 60_000;
/// Lectures de produit par minute (limite d'Open Food Facts : 100).
const PRODUCTS_PER_MINUTE: usize = 60;
/// Recherches par minute (limite d'Open Food Facts : 10).
const SEARCHES_PER_MINUTE: usize = 8;

/// Client Open Food Facts. Un seul par application : il porte les compteurs de débit.
pub struct OffClient {
    http: reqwest::Client,
    products: Mutex<RateLimiter>,
    searches: Mutex<RateLimiter>,
}

fn acquire(limiter: &Mutex<RateLimiter>, now_ms: i64) -> Result<(), OffError> {
    let mut limiter = limiter
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    limiter
        .try_acquire(now_ms)
        .map_err(|wait_ms| OffError::RateLimited {
            wait_s: (wait_ms + 999) / 1000,
        })
}

impl OffClient {
    /// Crée le client. `app_version` est la version de Fuck Calorie, pour le `User-Agent`.
    pub fn new(app_version: &str) -> Result<Self, OffError> {
        // Fournisseur de chiffrement de rustls ; sans effet s'il est déjà installé.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let builder = reqwest::Client::builder()
            .user_agent(off::user_agent(app_version))
            .timeout(Duration::from_secs(10));
        #[cfg(target_os = "android")]
        let builder = builder.tls_backend_preconfigured(embedded_roots_tls()?);
        let http = builder.build().map_err(|_| OffError::Network)?;
        Ok(Self {
            http,
            products: Mutex::new(RateLimiter::new(PRODUCTS_PER_MINUTE, MINUTE_MS)),
            searches: Mutex::new(RateLimiter::new(SEARCHES_PER_MINUTE, MINUTE_MS)),
        })
    }

    /// Télécharge le texte d'une réponse. `Ok(None)` pour une réponse « 404 ».
    async fn get(&self, url: &str) -> Result<Option<String>, OffError> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|_| OffError::Network)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(OffError::Server(status.as_u16()));
        }
        response
            .text()
            .await
            .map(Some)
            .map_err(|_| OffError::Network)
    }

    /// Fiche d'un produit par code-barres. `Ok(None)` : produit inconnu d'Open Food Facts.
    pub async fn product(&self, barcode: &str, now_ms: i64) -> Result<Option<Food>, OffError> {
        let url = off::product_url(barcode)?;
        acquire(&self.products, now_ms)?;
        match self.get(&url).await? {
            Some(body) => off::parse_product_response(&body, now_ms),
            None => Ok(None),
        }
    }

    /// Recherche de produits par texte.
    pub async fn search(&self, query: &str, now_ms: i64) -> Result<Vec<Food>, OffError> {
        acquire(&self.searches, now_ms)?;
        match self.get(&off::search_url(query)).await? {
            Some(body) => off::parse_search_response(&body, now_ms),
            None => Ok(Vec::new()),
        }
    }
}

/// Configuration TLS avec les certificats racine embarqués (Mozilla, `webpki-roots`), pour
/// Android.
#[cfg(any(target_os = "android", test))]
fn embedded_roots_tls() -> Result<rustls::ClientConfig, OffError> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    Ok(rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|_| OffError::Network)?
    .with_root_certificates(roots)
    .with_no_client_auth())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_tls_configuration_is_accepted() {
        // Si la version de rustls ne correspondait pas à celle de reqwest, la configuration
        // serait refusée à la construction du client.
        let tls = embedded_roots_tls().unwrap();
        reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .build()
            .unwrap();
    }

    #[test]
    fn client_builds_and_limits_searches() {
        let client = OffClient::new("0.1.0").unwrap();
        for _ in 0..SEARCHES_PER_MINUTE {
            assert_eq!(acquire(&client.searches, 0), Ok(()));
        }
        assert_eq!(
            acquire(&client.searches, 500),
            Err(OffError::RateLimited { wait_s: 60 })
        );
        // Les lectures de produit ont leur propre compteur.
        assert_eq!(acquire(&client.products, 500), Ok(()));
    }

    #[test]
    fn invalid_barcode_is_refused_before_any_request() {
        let client = OffClient::new("0.1.0").unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        assert_eq!(
            runtime.block_on(client.product("abc", 0)),
            Err(OffError::InvalidBarcode)
        );
    }
}
