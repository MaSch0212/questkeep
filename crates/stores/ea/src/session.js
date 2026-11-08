(function () {
  var reportSignedOut = REPORT_SIGNED_OUT;
  var target = "https://questkeep.invalid/ea-session";
  if (location.hostname === "signin.ea.com") {
    // The hidden window renews the session through the website login: with a remembered login
    // it never stops here, so a login form that stays means the session is really signed out.
    if (reportSignedOut) {
      setTimeout(function () { location.href = target + "?signedout=1"; }, 5000);
      return;
    }
    var tries = 0;
    var tick = setInterval(function () {
      var box = document.getElementById("rememberMe");
      if (box || ++tries > 40) clearInterval(tick);
      if (box && !box.checked) box.click();
    }, 250);
    return;
  }
  if (location.hostname !== "accounts.ea.com" || location.pathname !== "/connect/auth" ||
      location.search.indexOf("nucleus") < 0) return;
  function capture() {
    var data = null;
    try { data = JSON.parse(document.body.innerText); } catch (e) {}
    if (data && data.access_token) {
      location.href = target + "?token=" + encodeURIComponent(data.access_token) +
        "&expires=" + encodeURIComponent(data.expires_in || "");
    } else if (reportSignedOut) {
      location.href = target + "?signedout=1";
    }
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", capture);
  } else {
    capture();
  }
})();
