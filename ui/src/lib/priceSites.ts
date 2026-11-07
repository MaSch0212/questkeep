import type { PriceSite } from "../bindings/PriceSite";
import type { PriceSiteSuggestions } from "../bindings/PriceSiteSuggestions";

/** Why a typed site can't be added, or null when it can. */
export type SiteProblem = "name" | "url" | "duplicate";

/** The address as typed, made https (a bare host gets the scheme). Null when it isn't one. */
export function siteUrl(text: string): string | null {
  const raw = text.trim();
  if (raw === "") return null;
  const withScheme = /^[a-z][a-z0-9+.-]*:/i.test(raw) ? raw : `https://${raw}`;
  try {
    const url = new URL(withScheme);
    if (url.protocol !== "https:" || url.hostname === "" || !url.hostname.includes(".")) {
      return null;
    }
    return url.href;
  } catch {
    return null;
  }
}

const hostOf = (url: string) => {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
};

/** Whether `url` is the same site as one in the list (by host, `www.` or not). */
export const hasSite = (sites: PriceSite[], url: string) =>
  sites.some((s) => hostOf(s.url) === hostOf(url));

/** The site to add from the form, or what is wrong with it. */
export function checkSite(
  sites: PriceSite[],
  name: string,
  address: string,
): { site: PriceSite } | { problem: SiteProblem } {
  const url = siteUrl(address);
  if (url === null) return { problem: "url" };
  if (hasSite(sites, url)) return { problem: "duplicate" };
  const title = name.trim() || hostOf(url);
  if (title === "") return { problem: "name" };
  return { site: { name: title, url } };
}

/** The list with the site at `index` moved one place up (or down with `by` 1). */
export function moveSite(sites: PriceSite[], index: number, by: -1 | 1): PriceSite[] {
  const to = index + by;
  if (index < 0 || index >= sites.length || to < 0 || to >= sites.length) return sites;
  const next = [...sites];
  const [moved] = next.splice(index, 1);
  if (moved) next.splice(to, 0, moved);
  return next;
}

/** The suggestions not added yet: the regional ones first, then the global ones. */
export const openSuggestions = (suggestions: PriceSiteSuggestions, sites: PriceSite[]) =>
  [...suggestions.regional, ...suggestions.global].filter((s) => !hasSite(sites, s.url));
