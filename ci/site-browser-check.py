#!/usr/bin/env python3
"""Load the project site in a real headless browser and check what the static
checker cannot: that the CSP in the <meta> tag blocks nothing the page needs,
that no request leaves the origin, that the interactive bits work, and an
axe-core accessibility scan (with contrast) at desktop and phone widths.

Local check, not CI: needs Playwright for Python with its Chromium
(`python3 -m playwright install chromium`) and, for the axe scan, axe.min.js
from the axe-core npm package (`npm pack axe-core`), pointed at with --axe.

Usage: ci/site-browser-check.py [--axe path/to/axe.min.js] [--shots DIR] [site-dir]
Exit 1 on any failure. The site is served from a local HTTP server, so the
page runs in a secure context exactly as on GitHub Pages (copy buttons appear).
"""
from __future__ import annotations

import argparse
import functools
import http.server
import json
import socketserver
import sys
import threading
from pathlib import Path

from playwright.sync_api import sync_playwright

VIEWPORTS = {"desktop": (1280, 900), "phone": (390, 844)}


def serve(directory: Path) -> tuple[socketserver.TCPServer, int]:
    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(directory))
    handler.log_message = lambda *a, **k: None  # type: ignore[attr-defined]
    srv = socketserver.TCPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv, srv.server_address[1]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("site", nargs="?", default="site")
    ap.add_argument("--axe", help="path to axe.min.js (axe-core); skipped when absent")
    ap.add_argument("--shots", help="directory for screenshots")
    args = ap.parse_args()
    site = Path(args.site)
    axe_src = Path(args.axe).read_text(encoding="utf-8") if args.axe else None
    srv, port = serve(site)
    origin = f"http://127.0.0.1:{port}"
    problems: list[str] = []
    bad = problems.append

    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        for name, (w, h) in VIEWPORTS.items():
            ctx = browser.new_context(viewport={"width": w, "height": h})
            page = ctx.new_page()
            console: list[str] = []
            failed: list[str] = []
            requests: list[str] = []
            page.on("console", lambda m: console.append(f"{m.type}: {m.text}") if m.type in ("error", "warning") else None)
            page.on("pageerror", lambda e: console.append(f"pageerror: {e}"))
            page.on("requestfailed", lambda r: failed.append(f"{r.url} ({r.failure})"))
            page.on("request", lambda r: requests.append(r.url))
            page.add_init_script(
                "window.__csp=[];document.addEventListener('securitypolicyviolation',"
                "e=>window.__csp.push(e.violatedDirective+' '+(e.blockedURI||'inline')+' @'+e.sourceFile+':'+e.lineNumber));"
            )
            resp = page.goto(f"{origin}/index.html", wait_until="networkidle")
            if not resp or resp.status != 200:
                bad(f"[{name}] index.html did not load (status {resp.status if resp else 'none'})")
            csp_hits = page.evaluate("window.__csp")
            for hit in csp_hits:
                bad(f"[{name}] CSP violation: {hit}")
            for c in console:
                bad(f"[{name}] console {c}")
            for f in failed:
                bad(f"[{name}] request failed: {f}")
            foreign = [u for u in requests if not u.startswith(origin)]
            for u in foreign:
                bad(f"[{name}] request left the origin: {u}")

            # fonts resolved, styles applied (the CSS loaded under the CSP)
            fonts_ok = page.evaluate("document.fonts.ready.then(() => document.fonts.check('700 1em \"Washi Mincho\"'))")
            if not fonts_ok:
                bad(f"[{name}] the heading font did not load")
            h1_font = page.evaluate("getComputedStyle(document.querySelector('h1')).fontFamily")
            if "Washi Mincho" not in h1_font:
                bad(f"[{name}] stylesheet not applied to h1 (font-family: {h1_font})")
            if page.evaluate("getComputedStyle(document.querySelector('.svg-defs')).position") != "absolute":
                bad(f"[{name}] .svg-defs is not positioned absolute (inline style replacement missing)")
            h3 = page.evaluate("getComputedStyle(document.querySelector('.h3-sub')).fontSize")
            if abs(float(h3.rstrip("px")) - 1.35 * 16) > 0.6:
                bad(f"[{name}] .h3-sub font-size is {h3}, expected ~21.6px")

            # the page is a secure context on localhost, so site.js adds copy buttons
            n_copy = page.locator("button.copy").count()
            n_blocks = page.locator(".code[data-copy]").count()
            if n_copy != n_blocks:
                bad(f"[{name}] expected a copy button on each of {n_blocks} code blocks, found {n_copy}")
            for btn in page.locator("button.copy").all()[:2]:
                if not btn.get_attribute("aria-label"):
                    bad(f"[{name}] copy button without aria-label")

            # skip link is first in tab order and lands on main
            page.keyboard.press("Tab")
            focused = page.evaluate("document.activeElement && document.activeElement.getAttribute('href')")
            if focused != "#main":
                bad(f"[{name}] first Tab focuses {focused!r}, not the skip link")
            page.keyboard.press("Enter")
            if page.evaluate("location.hash") != "#main":
                bad(f"[{name}] skip link did not navigate to #main")

            # every nav target exists and scrolls
            for href in page.eval_on_selector_all("nav a[href^='#']", "els => els.map(e => e.getAttribute('href'))"):
                if page.locator(href).count() != 1:
                    bad(f"[{name}] nav target {href} not found")

            # details/summary (FAQ) open with keyboard
            first_sum = page.locator(".faq details summary").first
            first_sum.focus()
            page.keyboard.press("Enter")
            if not page.locator(".faq details").first.get_attribute("open") is not None:
                bad(f"[{name}] FAQ <details> did not open on Enter")

            if name == "phone":
                menu = page.locator("details.nav-menu")
                if not menu.is_visible():
                    bad("[phone] mobile menu toggle not visible")
                else:
                    menu.locator("summary").click()
                    if menu.get_attribute("open") is None:
                        bad("[phone] mobile menu did not open")
                    menu.locator("a[href='#faq']").click()
                    if menu.get_attribute("open") is not None:
                        bad("[phone] mobile menu stayed open after choosing a link")
                    if page.evaluate("location.hash") != "#faq":
                        bad("[phone] menu link did not navigate")
                    if page.evaluate("document.documentElement.scrollWidth > document.documentElement.clientWidth"):
                        bad("[phone] horizontal overflow: page is wider than the viewport")

            if axe_src:
                page.evaluate(axe_src)
                result = page.evaluate(
                    "axe.run(document, {runOnly: {type: 'tag', values: ['wcag2a','wcag2aa','wcag21a','wcag21aa','best-practice']}})"
                )
                for v in result["violations"]:
                    targets = "; ".join(n["target"][0] for n in v["nodes"][:4])
                    bad(f"[{name}] axe {v['impact']}: {v['id']} ({v['help']}) at {targets}")
                print(f"  axe [{name}]: {len(result['passes'])} rules passed, "
                      f"{len(result['violations'])} violations, {len(result['incomplete'])} needs-review")
                for inc in result["incomplete"]:
                    print(f"    review [{name}]: {inc['id']} ({len(inc['nodes'])} nodes)")

            if args.shots:
                Path(args.shots).mkdir(parents=True, exist_ok=True)
                page.goto(f"{origin}/index.html", wait_until="networkidle")
                page.screenshot(path=f"{args.shots}/site-{name}.png", full_page=True)
            print(f"  [{name}] {len(requests)} requests, {len(csp_hits)} CSP violations, {len(console)} console errors")
            ctx.close()
        browser.close()
    srv.shutdown()

    if problems:
        print("\n".join(problems), file=sys.stderr)
        print(f"==> FAIL: browser check ({len(problems)} problems)", file=sys.stderr)
        return 1
    print("==> browser check OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
