//! The tray's texts in the app's language. The web UI translates itself (`ui/src/i18n`); the shell
//! only needs the few texts of the tray menu and popup. The language is the one chosen in the
//! settings, else the first supported system language, else English (like the UI).

/// The UI languages, English first (the fallback).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    De,
    Fr,
    It,
    Es,
    Pt,
}

impl Lang {
    /// The language of a BCP 47 or POSIX tag ("de-AT", "pt_BR.UTF-8"), if the app has it.
    fn of(tag: &str) -> Option<Self> {
        let base = tag.split(['-', '_', '.']).next()?.to_ascii_lowercase();
        Some(match base.as_str() {
            "en" => Self::En,
            "de" => Self::De,
            "fr" => Self::Fr,
            "it" => Self::It,
            "es" => Self::Es,
            "pt" => Self::Pt,
            _ => return None,
        })
    }
}

/// The chosen language, else the first supported one of `system`, else English.
pub fn resolve(chosen: Option<&str>, system: &[String]) -> Lang {
    chosen
        .and_then(Lang::of)
        .or_else(|| system.iter().find_map(|tag| Lang::of(tag)))
        .unwrap_or_default()
}

/// The app's language for the given setting (`None`: the system language).
pub fn current(chosen: Option<&str>) -> Lang {
    resolve(chosen, &system_languages())
}

/// Texts of the native tray menu (platforms without the popup).
#[derive(Debug, Clone, Copy)]
pub struct MenuTexts {
    pub no_recent: &'static str,
    pub favorites: &'static str,
    pub no_favorites: &'static str,
    pub launchers: &'static str,
    /// `{version}` is replaced.
    pub update: &'static str,
    pub open: &'static str,
    pub exit: &'static str,
}

pub fn menu_texts(lang: Lang) -> MenuTexts {
    match lang {
        Lang::En => MenuTexts {
            no_recent: "No recently played games",
            favorites: "Favorites",
            no_favorites: "No favorites",
            launchers: "Launchers",
            update: "Update to {version}…",
            open: "Open Questkeep",
            exit: "Exit",
        },
        Lang::De => MenuTexts {
            no_recent: "Keine zuletzt gespielten Spiele",
            favorites: "Favoriten",
            no_favorites: "Keine Favoriten",
            launchers: "Launcher",
            update: "Auf {version} aktualisieren…",
            open: "Questkeep öffnen",
            exit: "Beenden",
        },
        Lang::Fr => MenuTexts {
            no_recent: "Aucun jeu joué récemment",
            favorites: "Favoris",
            no_favorites: "Aucun favori",
            launchers: "Lanceurs",
            update: "Mettre à jour vers {version}…",
            open: "Ouvrir Questkeep",
            exit: "Quitter",
        },
        Lang::It => MenuTexts {
            no_recent: "Nessun gioco giocato di recente",
            favorites: "Preferiti",
            no_favorites: "Nessun preferito",
            launchers: "Launcher",
            update: "Aggiorna a {version}…",
            open: "Apri Questkeep",
            exit: "Esci",
        },
        Lang::Es => MenuTexts {
            no_recent: "No hay juegos jugados recientemente",
            favorites: "Favoritos",
            no_favorites: "No hay favoritos",
            launchers: "Lanzadores",
            update: "Actualizar a {version}…",
            open: "Abrir Questkeep",
            exit: "Salir",
        },
        Lang::Pt => MenuTexts {
            no_recent: "Nenhum jogo jogado recentemente",
            favorites: "Favoritos",
            no_favorites: "Nenhum favorito",
            launchers: "Inicializadores",
            update: "Atualizar para {version}…",
            open: "Abrir o Questkeep",
            exit: "Sair",
        },
    }
}

/// Texts of the Windows tray popup.
#[cfg(windows)]
pub fn popup_texts(lang: Lang) -> questkeep_tray_popup::PopupTexts {
    let menu = menu_texts(lang);
    let (recent, empty_recent, exit_app, update, open_launcher, play) = match lang {
        Lang::En => (
            "Recently played",
            "No recently played games yet",
            "Exit Questkeep",
            "Update to {version}",
            "Open {name}",
            "Play",
        ),
        Lang::De => (
            "Zuletzt gespielt",
            "Noch keine zuletzt gespielten Spiele",
            "Questkeep beenden",
            "Auf {version} aktualisieren",
            "{name} öffnen",
            "Spielen",
        ),
        Lang::Fr => (
            "Joués récemment",
            "Aucun jeu joué récemment pour l'instant",
            "Quitter Questkeep",
            "Mettre à jour vers {version}",
            "Ouvrir {name}",
            "Jouer",
        ),
        Lang::It => (
            "Giocati di recente",
            "Ancora nessun gioco giocato di recente",
            "Esci da Questkeep",
            "Aggiorna a {version}",
            "Apri {name}",
            "Gioca",
        ),
        Lang::Es => (
            "Jugados recientemente",
            "Aún no hay juegos jugados recientemente",
            "Salir de Questkeep",
            "Actualizar a {version}",
            "Abrir {name}",
            "Jugar",
        ),
        Lang::Pt => (
            "Jogados recentemente",
            "Nenhum jogo jogado recentemente ainda",
            "Sair do Questkeep",
            "Atualizar para {version}",
            "Abrir {name}",
            "Jogar",
        ),
    };
    questkeep_tray_popup::PopupTexts {
        recent: recent.into(),
        favorites: menu.favorites.into(),
        launchers: menu.launchers.into(),
        empty_recent: empty_recent.into(),
        open: menu.open.into(),
        exit: menu.exit.into(),
        exit_app: exit_app.into(),
        update: update.into(),
        open_launcher: open_launcher.into(),
        play: play.into(),
    }
}

/// The user's preferred display languages, most preferred first.
#[cfg(windows)]
fn system_languages() -> Vec<String> {
    windows_impl::preferred_ui_languages()
}

/// The user's languages from the usual environment variables (`LANGUAGE` lists several).
#[cfg(not(windows))]
fn system_languages() -> Vec<String> {
    ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .flat_map(|value| value.split(':').map(str::to_string).collect::<Vec<_>>())
        .filter(|tag| !tag.is_empty() && tag != "C" && tag != "POSIX")
        .collect()
}

/// The user's country (ISO 3166, e.g. `DE`) from the system's region setting, for regional
/// suggestions.
#[cfg(windows)]
pub fn system_region() -> Option<String> {
    windows_impl::user_geo_name()
}

/// The user's country from the locale environment variables (`de_DE.UTF-8` → `DE`).
#[cfg(not(windows))]
pub fn system_region() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find_map(|value| questkeep_core::price_sites::region_of_locale(&value))
}

// Each call is a plain Win32 FFI call with owned buffers.
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_impl {
    use windows_sys::Win32::Globalization::{
        GetUserDefaultGeoName, GetUserPreferredUILanguages, MUI_LANGUAGE_NAME,
    };

    /// The region set in Windows (a two-letter code; UN M.49 numbers for wider areas are skipped).
    pub fn user_geo_name() -> Option<String> {
        let mut buffer = [0u16; 16];
        // SAFETY: the buffer and its length match; the result is the written length with NUL.
        let len = unsafe { GetUserDefaultGeoName(buffer.as_mut_ptr(), buffer.len() as i32) };
        let len = usize::try_from(len).ok()?.checked_sub(1)?;
        let name = String::from_utf16_lossy(buffer.get(..len)?);
        (name.len() == 2 && name.bytes().all(|b| b.is_ascii_alphabetic())).then_some(name)
    }

    pub fn preferred_ui_languages() -> Vec<String> {
        let mut count = 0u32;
        let mut len = 0u32;
        // SAFETY: a null buffer asks for the needed length only.
        let ok = unsafe {
            GetUserPreferredUILanguages(
                MUI_LANGUAGE_NAME,
                &mut count,
                std::ptr::null_mut(),
                &mut len,
            )
        };
        if ok == 0 || len == 0 {
            return Vec::new();
        }
        let mut buffer = vec![0u16; len as usize];
        // SAFETY: `buffer` holds `len` UTF-16 units, as the first call asked for.
        let ok = unsafe {
            GetUserPreferredUILanguages(
                MUI_LANGUAGE_NAME,
                &mut count,
                buffer.as_mut_ptr(),
                &mut len,
            )
        };
        if ok == 0 {
            return Vec::new();
        }
        // A list of NUL-terminated names, ended by an empty one.
        buffer
            .split(|&c| c == 0)
            .filter(|name| !name.is_empty())
            .map(String::from_utf16_lossy)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|t| (*t).to_string()).collect()
    }

    #[test]
    fn the_chosen_language_wins() {
        assert_eq!(resolve(Some("fr"), &tags(&["de-DE"])), Lang::Fr);
    }

    #[test]
    fn without_a_choice_the_first_supported_system_language_is_used() {
        assert_eq!(resolve(None, &tags(&["ja-JP", "pt-BR", "de"])), Lang::Pt);
        assert_eq!(resolve(None, &tags(&["de_AT.UTF-8"])), Lang::De);
    }

    #[test]
    fn unknown_languages_fall_back_to_english() {
        assert_eq!(resolve(Some("xx"), &tags(&["ja-JP"])), Lang::En);
        assert_eq!(resolve(None, &[]), Lang::En);
    }

    #[cfg(windows)]
    #[test]
    fn english_popup_texts_are_the_popup_defaults() {
        assert_eq!(
            popup_texts(Lang::En),
            questkeep_tray_popup::PopupTexts::default()
        );
    }

    #[cfg(windows)]
    #[test]
    fn every_language_keeps_the_placeholders() {
        for lang in [Lang::En, Lang::De, Lang::Fr, Lang::It, Lang::Es, Lang::Pt] {
            let texts = popup_texts(lang);
            assert!(texts.update.contains("{version}"), "{lang:?}");
            assert!(texts.open_launcher.contains("{name}"), "{lang:?}");
            assert!(menu_texts(lang).update.contains("{version}"), "{lang:?}");
        }
    }
}
