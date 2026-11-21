// Optional niceties; the page is complete without JavaScript.
(function () {
  "use strict";

  // Copy buttons on command blocks (prompts are not copied).
  if (navigator.clipboard && window.isSecureContext) {
    document.documentElement.classList.add("js");
    document.querySelectorAll(".code[data-copy]").forEach(function (block) {
      var code = block.querySelector("code");
      if (!code) return;
      var button = document.createElement("button");
      button.type = "button";
      button.className = "copy";
      button.textContent = "Copy";
      button.setAttribute("aria-label", "Copy command");
      button.addEventListener("click", function () {
        var clone = code.cloneNode(true);
        clone.querySelectorAll(".prompt").forEach(function (p) { p.remove(); });
        navigator.clipboard.writeText(clone.textContent.trim()).then(function () {
          button.textContent = "Copied";
          setTimeout(function () { button.textContent = "Copy"; }, 1600);
        });
      });
      block.appendChild(button);
    });
  }

  // Close the mobile menu after a link is chosen.
  var menu = document.querySelector(".nav-menu");
  if (menu) {
    menu.addEventListener("click", function (event) {
      if (event.target.closest("a")) menu.removeAttribute("open");
    });
    document.addEventListener("keydown", function (event) {
      if (event.key === "Escape" && menu.hasAttribute("open")) {
        menu.removeAttribute("open");
        menu.querySelector("summary").focus();
      }
    });
  }
})();
