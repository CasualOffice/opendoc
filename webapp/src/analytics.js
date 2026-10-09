// Google Analytics (gtag.js) for the project's own hosted site and editor.
//
// Loaded only on the canonical host. The same `editor.html` and pages ship in
// the self-host image (`Dockerfile.editor`) and run under localhost in the
// test suites, and neither should report to this project's GA property: the
// runtime sends no telemetry by default (docs/07), and a self-hoster's users
// are theirs. A classic script, so it runs before any module and never blocks
// the page — the tag itself is fetched async.
(function () {
  var HOSTS = ["opendoc.casualoffice.org"];
  var ID = "G-4DEDXRTCF4";
  if (HOSTS.indexOf(window.location.hostname) === -1) return;

  var tag = document.createElement("script");
  tag.async = true;
  tag.src = "https://www.googletagmanager.com/gtag/js?id=" + ID;
  document.head.appendChild(tag);

  window.dataLayer = window.dataLayer || [];
  function gtag() {
    window.dataLayer.push(arguments);
  }
  window.gtag = gtag;
  gtag("js", new Date());
  gtag("config", ID);
})();
