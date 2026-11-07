import { describe, expect, it } from "vitest";
import { checkSite, hasSite, moveSite, openSuggestions, siteUrl } from "./priceSites";

const gg = { name: "GG.deals", url: "https://gg.deals/" };
const itad = { name: "IsThereAnyDeal", url: "https://isthereanydeal.com/" };

describe("price sites", () => {
  it("reads https addresses, adding the scheme to a bare host", () => {
    expect(siteUrl("gg.deals")).toBe("https://gg.deals/");
    expect(siteUrl(" https://www.mydealz.de/gruppe/games ")).toBe(
      "https://www.mydealz.de/gruppe/games",
    );
    expect(siteUrl("http://gg.deals")).toBeNull();
    expect(siteUrl("javascript:alert(1)")).toBeNull();
    expect(siteUrl("localhost")).toBeNull();
    expect(siteUrl("")).toBeNull();
  });

  it("checks the form: address, duplicates, and a name from the host", () => {
    expect(checkSite([gg], "", "nope")).toEqual({ problem: "url" });
    expect(checkSite([gg], "Again", "www.gg.deals")).toEqual({ problem: "duplicate" });
    expect(checkSite([gg], "", "isthereanydeal.com")).toEqual({
      site: { name: "isthereanydeal.com", url: "https://isthereanydeal.com/" },
    });
    expect(hasSite([gg], "https://www.gg.deals/de/")).toBe(true);
  });

  it("moves sites within the list", () => {
    expect(moveSite([gg, itad], 1, -1)).toEqual([itad, gg]);
    expect(moveSite([gg, itad], 0, -1)).toEqual([gg, itad]);
    expect(moveSite([gg, itad], 0, 1)).toEqual([itad, gg]);
  });

  it("suggests what isn't added yet, regional first", () => {
    const mydealz = { name: "mydealz", url: "https://www.mydealz.de/" };
    const suggestions = { region: "DE", regional: [mydealz], global: [gg, itad] };
    expect(openSuggestions(suggestions, [gg])).toEqual([mydealz, itad]);
  });
});
