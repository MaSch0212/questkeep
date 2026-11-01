import { afterEach, describe, expect, it } from "vitest";
import { localeOf, locale, resolveLocale, setLanguage, translator, type Translations } from ".";

const en = {
  hello: "Hello {name}",
  games: (p: { count: number }) => (p.count === 1 ? "1 game" : `${p.count} games`),
};
const dict: Translations<typeof en> = {
  en,
  de: { hello: "Hallo {name}", games: (p) => (p.count === 1 ? "1 Spiel" : `${p.count} Spiele`) },
  fr: { hello: "Bonjour {name}", games: (p) => `${p.count} jeux` },
  it: { hello: "Ciao {name}", games: (p) => `${p.count} giochi` },
  es: { hello: "Hola {name}", games: (p) => `${p.count} juegos` },
  pt: { hello: "Olá {name}", games: (p) => `${p.count} jogos` },
};
const tr = translator(dict);

afterEach(() => setLanguage(null));

describe("i18n", () => {
  it("picks the chosen language, else a supported system language, else English", () => {
    expect(localeOf("de-AT")).toBe("de");
    expect(localeOf("pt_BR")).toBe("pt");
    expect(localeOf("nl-NL")).toBeNull();
    expect(resolveLocale("fr", ["de-DE"])).toBe("fr");
    expect(resolveLocale(null, ["nl-NL", "es-ES"])).toBe("es");
    expect(resolveLocale(null, ["ja-JP"])).toBe("en");
    expect(resolveLocale("xx", [])).toBe("en");
  });

  it("translates with placeholders and functions in the current language", () => {
    setLanguage("en");
    expect(tr("hello", { name: "Ada" })).toBe("Hello Ada");
    expect(tr("games", { count: 1 })).toBe("1 game");
    setLanguage("de");
    expect(locale()).toBe("de");
    expect(tr("hello", { name: "Ada" })).toBe("Hallo Ada");
    expect(tr("games", { count: 3 })).toBe("3 Spiele");
    expect(document.documentElement.lang).toBe("de");
  });
});
