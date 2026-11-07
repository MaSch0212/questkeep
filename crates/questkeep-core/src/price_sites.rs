//! Price comparison websites the user keeps (opened in their browser) and the ones
//! suggested for their region. Questkeep fetches nothing from them; it only knows their
//! addresses.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A price comparison website.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PriceSite {
    pub name: String,
    /// An https address.
    pub url: String,
}

/// Sites suggested in Settings: the regional ones first, then those that work everywhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PriceSiteSuggestions {
    /// The user's country (ISO 3166 code, e.g. `DE`), when known.
    pub region: Option<String>,
    pub regional: Vec<PriceSite>,
    pub global: Vec<PriceSite>,
}

fn site(name: &str, url: &str) -> PriceSite {
    PriceSite {
        name: name.to_string(),
        url: url.to_string(),
    }
}

/// Comparison sites that cover every region.
fn global() -> Vec<PriceSite> {
    vec![
        site("GG.deals", "https://gg.deals/"),
        site("IsThereAnyDeal", "https://isthereanydeal.com/"),
        site("AllKeyShop", "https://www.allkeyshop.com/blog/"),
        site("SteamDB", "https://steamdb.info/"),
    ]
}

/// Deal sites of a country (`DE`, `GB`, ...): mostly the local deal community.
fn regional(region: &str) -> Vec<PriceSite> {
    match region {
        "DE" | "AT" | "CH" => vec![
            site("mydealz", "https://www.mydealz.de/"),
            site("Keyforsteam", "https://www.keyforsteam.de/"),
        ],
        "FR" | "BE" | "LU" => vec![site("Dealabs", "https://www.dealabs.com/")],
        "GB" | "IE" => vec![site("hotukdeals", "https://www.hotukdeals.com/")],
        "US" => vec![
            site("Slickdeals", "https://slickdeals.net/"),
            site("CheapShark", "https://www.cheapshark.com/"),
        ],
        "ES" => vec![site("Chollometro", "https://www.chollometro.com/")],
        "IT" => vec![site("Pepper.it", "https://www.pepper.it/")],
        "BR" => vec![site("Pelando", "https://www.pelando.com.br/")],
        _ => Vec::new(),
    }
}

/// The suggestions for `region` (an ISO 3166 code in any case; `None` when unknown).
pub fn suggestions(region: Option<&str>) -> PriceSiteSuggestions {
    let region = region
        .map(str::trim)
        .filter(|r| r.len() == 2 && r.bytes().all(|b| b.is_ascii_alphabetic()))
        .map(str::to_ascii_uppercase);
    PriceSiteSuggestions {
        regional: region.as_deref().map(regional).unwrap_or_default(),
        global: global(),
        region,
    }
}

/// The user's country from a POSIX locale (`de_DE.UTF-8` → `DE`), for systems without a
/// region setting of their own.
pub fn region_of_locale(locale: &str) -> Option<String> {
    let tag = locale.split(['.', '@']).next()?;
    let country = tag.split(['_', '-']).nth(1)?;
    (country.len() == 2 && country.bytes().all(|b| b.is_ascii_alphabetic()))
        .then(|| country.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regional_sites_come_with_the_global_ones() {
        let de = suggestions(Some("de"));
        assert_eq!(de.region.as_deref(), Some("DE"));
        assert_eq!(de.regional[0].name, "mydealz");
        assert!(de.global.iter().any(|s| s.name == "GG.deals"));
        let unknown = suggestions(Some("ZZ"));
        assert!(unknown.regional.is_empty());
        assert_eq!(unknown.global.len(), de.global.len());
        assert_eq!(suggestions(Some("Germany")).region, None);
        assert_eq!(suggestions(None).region, None);
    }

    #[test]
    fn every_suggestion_is_https() {
        for region in ["DE", "FR", "GB", "US", "ES", "IT", "BR"] {
            let all = suggestions(Some(region));
            for site in all.regional.iter().chain(&all.global) {
                assert!(site.url.starts_with("https://"), "{}", site.url);
            }
        }
    }

    #[test]
    fn reads_the_country_of_a_locale() {
        assert_eq!(region_of_locale("de_DE.UTF-8").as_deref(), Some("DE"));
        assert_eq!(region_of_locale("pt-br").as_deref(), Some("BR"));
        assert_eq!(region_of_locale("C.UTF-8"), None);
        assert_eq!(region_of_locale("en"), None);
    }
}
