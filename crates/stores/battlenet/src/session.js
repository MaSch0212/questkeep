(function () {
  var reportSignedOut = REPORT_SIGNED_OUT;
  var target = "https://questkeep.invalid/battlenet-session";
  var host = location.hostname;
  if (host !== "battle.net" && !/\.battle\.net$/.test(host)) return;
  if (/^\/login/.test(location.pathname)) {
    if (reportSignedOut) {
      setTimeout(function () { location.href = target + "?signedout=1"; }, 5000);
    }
    return;
  }
  if (host !== "account.battle.net" || /^\/(callback|oauth2)\//.test(location.pathname)) return;
  function get(path, anyStatus) {
    return fetch(path, { credentials: "include", headers: { Accept: "application/json" } })
      .then(function (r) {
        if (!r.ok && !anyStatus) throw new Error("http " + r.status);
        return r.json();
      });
  }
  function tag(details) {
    var t = details && details.battleTag;
    if (t && typeof t === "object") return t.name ? t.name + (t.code ? "#" + t.code : "") : "";
    return typeof t === "string" ? t : "";
  }
  get("/api/", true).then(function (status) {
    if (!status || !status.authenticated) throw new Error("signedout");
    return Promise.all([
      get("/api/games-and-subs"),
      get("/api/details").catch(function () { return null; })
    ]);
  }).then(function (r) {
    var games = ((r[0] && r[0].gameAccounts) || []).map(function (g) {
      return { titleId: g.titleId, localizedGameName: g.localizedGameName };
    });
    location.href = target + "?name=" + encodeURIComponent(tag(r[1])) +
      "&games=" + encodeURIComponent(JSON.stringify(games));
  }).catch(function (e) {
    if (reportSignedOut && e && e.message === "signedout") location.href = target + "?signedout=1";
  });
})();
