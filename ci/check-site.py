#!/usr/bin/env python3
"""Static checks for the project site (site/index.html): structure, accessibility
basics and the Content-Security-Policy the page carries in a <meta> tag.

GitHub Pages sends no custom response headers, so the only CSP the site can have
is the <meta http-equiv> one; this script keeps the markup compatible with it
(no inline styles, no inline scripts, no event-handler attributes, only
same-origin resources) and keeps the policy itself strict.

Usage: ci/check-site.py [site/index.html]   (stdlib only; exit 1 on any failure)
"""
from __future__ import annotations

import re
import sys
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlsplit

VOID = {"area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta",
        "source", "track", "wbr"}
REQUIRED_CSP = {
    "default-src": {"'none'"},
    "script-src": {"'self'"},
    "style-src": {"'self'"},
    "img-src": {"'self'"},
    "font-src": {"'self'"},
    "object-src": {"'none'"},
    "base-uri": {"'none'"},
    "form-action": {"'none'"},
}
# directives browsers ignore inside <meta>: having them there only looks safe
META_IGNORED = {"frame-ancestors", "report-uri", "report-to", "sandbox"}


class Page(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.doctype = ""
        self.html_attrs: dict[str, str | None] = {}
        self.metas: list[dict[str, str | None]] = []
        self.links: list[dict[str, str | None]] = []
        self.title = ""
        self.stack: list[tuple[str, dict[str, str | None]]] = []
        self.ids: dict[str, int] = {}
        self.headings: list[tuple[int, str, int]] = []
        self.anchors: list[dict] = []
        self.imgs: list[tuple[dict, int]] = []
        self.svgs: list[tuple[dict, int]] = []
        self.svg_depth = 0
        self.resources: list[tuple[str, str, int]] = []
        self.inline_styles: list[int] = []
        self.inline_scripts: list[int] = []
        self.style_tags: list[int] = []
        self.handlers: list[tuple[str, int]] = []
        self.navs: list[tuple[str | None, int]] = []
        self.sections: list[tuple[dict, int]] = []
        self.details: list[tuple[bool, int]] = []
        self.landmarks: set[str] = set()
        self.first_body_anchor: dict | None = None
        self.in_body = False
        self.aria_refs: list[tuple[str, str, int]] = []
        self.text_collect: list[list[str]] = []
        self.buttons: list[tuple[dict, int]] = []
        self.tables: list[int] = []

    def handle_decl(self, decl: str) -> None:
        if not self.doctype:
            self.doctype = decl

    def handle_starttag(self, tag: str, attrs_list: list[tuple[str, str | None]]) -> None:
        attrs = dict(attrs_list)
        line = self.getpos()[0]
        if tag == "html":
            self.html_attrs = attrs
        elif tag == "meta":
            self.metas.append(attrs)
        elif tag == "link":
            self.links.append(attrs)
            href = attrs.get("href")
            if href and attrs.get("rel") in {"stylesheet", "icon", "apple-touch-icon", "preload"}:
                self.resources.append((tag, href, line))
        elif tag == "body":
            self.in_body = True
        if "id" in attrs and attrs["id"] is not None:
            self.ids[attrs["id"]] = self.ids.get(attrs["id"], 0) + 1
        if "style" in attrs:
            self.inline_styles.append(line)
        for k in attrs:
            if k.startswith("on"):
                self.handlers.append((k, line))
        for k in ("aria-labelledby", "aria-describedby", "aria-controls"):
            if attrs.get(k):
                for ref in attrs[k].split():
                    self.aria_refs.append((k, ref, line))
        if tag in ("header", "main", "footer", "nav"):
            if tag != "nav" or not self.svg_depth:
                self.landmarks.add(tag)
        if tag == "nav":
            self.navs.append((attrs.get("aria-label"), line))
        if tag == "section":
            self.sections.append((attrs, line))
        if tag == "details":
            self.details.append((False, line))
        if tag == "summary" and self.details:
            self.details[-1] = (True, self.details[-1][1])
        if tag == "svg":
            self.svg_depth += 1
            self.svgs.append((attrs, line))
        if tag == "img":
            self.imgs.append((attrs, line))
            if attrs.get("src"):
                self.resources.append((tag, attrs["src"], line))
        if tag == "script":
            if attrs.get("src"):
                self.resources.append((tag, attrs["src"], line))
            else:
                self.inline_scripts.append(line)
        if tag == "style":
            self.style_tags.append(line)
        if tag == "use" and attrs.get("href"):
            self.resources.append((tag, attrs["href"], line))
        if tag == "a":
            rec = {"attrs": attrs, "line": line, "text": []}
            self.anchors.append(rec)
            self.text_collect.append(rec["text"])
            if self.in_body and self.first_body_anchor is None:
                self.first_body_anchor = rec
        if tag == "button":
            self.buttons.append((attrs, line))
        if tag == "table":
            self.tables.append(line)
        if re.fullmatch(r"h[1-6]", tag):
            rec = {"text": []}
            self.headings.append((int(tag[1]), line, len(self.text_collect)))
            self.text_collect.append(rec["text"])
        if tag not in VOID:
            self.stack.append((tag, attrs))

    def handle_endtag(self, tag: str) -> None:
        if tag == "svg":
            self.svg_depth -= 1
        if tag == "a" and self.text_collect:
            self.text_collect.pop()
        if re.fullmatch(r"h[1-6]", tag) and self.text_collect:
            txt = "".join(self.text_collect.pop()).strip()
            lvl, line, _ = self.headings[-1]
            self.headings[-1] = (lvl, line, txt)  # type: ignore[assignment]
        while self.stack:
            t, _ = self.stack.pop()
            if t == tag:
                break

    def handle_data(self, data: str) -> None:
        if self.stack and self.stack[-1][0] == "title":
            self.title += data
        for buf in self.text_collect:
            buf.append(data)


def main(argv: list[str]) -> int:
    path = Path(argv[1] if len(argv) > 1 else "site/index.html")
    site_dir = path.parent
    html = path.read_text(encoding="utf-8")
    page = Page()
    page.feed(html)
    page.close()
    problems: list[str] = []
    notes: list[str] = []

    def bad(msg: str) -> None:
        problems.append(f"{path}: {msg}")

    # ---- document basics
    if page.doctype.lower() != "doctype html":
        bad("missing <!doctype html>")
    if not (page.html_attrs.get("lang") or "").strip():
        bad("<html> has no lang attribute")
    if not page.title.strip():
        bad("empty <title>")
    metas = {(m.get("name") or m.get("property") or m.get("http-equiv") or "").lower(): m for m in page.metas}
    if not any((m.get("charset") or "").lower() == "utf-8" for m in page.metas):
        bad("no <meta charset=utf-8>")
    if "viewport" not in metas:
        bad("no viewport meta")
    if not (metas.get("description", {}).get("content") or "").strip():
        bad("no meta description")
    if not any(l.get("rel") == "canonical" and l.get("href") for l in page.links):
        bad("no canonical link")

    # ---- CSP in <meta>
    csp_meta = metas.get("content-security-policy")
    if not csp_meta or not csp_meta.get("content"):
        bad("no <meta http-equiv=\"Content-Security-Policy\">")
    else:
        # the meta tag must come before anything that loads a resource
        order = [m for m in page.metas]
        csp_index = order.index(csp_meta)
        earlier_links = [l for l in page.links]  # links are head-only here
        head_html = html[: html.lower().find("</head>")]
        csp_pos = head_html.lower().find("content-security-policy")
        first_link = head_html.lower().find("<link")
        first_script = head_html.lower().find("<script")
        for what, pos in (("<link>", first_link), ("<script>", first_script)):
            if pos != -1 and pos < csp_pos:
                bad(f"CSP meta comes after the first {what}; resources before it are unprotected")
        del csp_index, earlier_links
        policy: dict[str, set[str]] = {}
        for part in csp_meta["content"].split(";"):
            toks = part.split()
            if toks:
                policy[toks[0].lower()] = set(toks[1:])
        for directive, must in REQUIRED_CSP.items():
            have = policy.get(directive)
            if have is None:
                bad(f"CSP lacks {directive}")
            elif not must <= have:
                bad(f"CSP {directive} must include {' '.join(sorted(must))} (has: {' '.join(sorted(have)) or 'nothing'})")
        for directive, srcs in policy.items():
            if directive in META_IGNORED:
                bad(f"CSP {directive} is ignored in a <meta> policy; it gives no protection here")
            for s in srcs:
                if s in ("'unsafe-inline'", "'unsafe-eval'", "*", "data:" if directive == "script-src" else "", "http:", "https:"):
                    bad(f"CSP {directive} allows {s}")
                if s.startswith(("http://", "https://")):
                    bad(f"CSP {directive} allows the external origin {s}; the site must stay self-contained")
        notes.append("CSP: " + "; ".join(f"{d} {' '.join(sorted(v))}" for d, v in policy.items()))

    # ---- markup compatible with that CSP
    for line in page.inline_styles:
        bad(f"line {line}: inline style= attribute (blocked by style-src 'self'; use a class)")
    for line in page.style_tags:
        bad(f"line {line}: <style> block (blocked by style-src 'self')")
    for line in page.inline_scripts:
        bad(f"line {line}: inline <script> (blocked by script-src 'self')")
    for name, line in page.handlers:
        bad(f"line {line}: {name}= event handler attribute (blocked by CSP)")
    for tag, href, line in page.resources:
        if href.startswith("#"):
            if href[1:] not in page.ids:
                bad(f"line {line}: <{tag}> references missing fragment {href}")
            continue
        parts = urlsplit(href)
        if parts.scheme or parts.netloc:
            bad(f"line {line}: <{tag}> loads a cross-origin resource {href}; the site makes no external requests")
            continue
        target = site_dir / parts.path
        if not target.is_file():
            bad(f"line {line}: <{tag}> references {href}, which does not exist under {site_dir}/")
    for a in page.anchors:
        href = a["attrs"].get("href") or ""
        if href.lower().startswith("javascript:"):
            bad(f"line {a['line']}: javascript: link")

    # ---- ids, fragments, aria references
    for i, n in page.ids.items():
        if n > 1:
            bad(f"duplicate id '{i}' ({n} times)")
    for a in page.anchors:
        href = a["attrs"].get("href") or ""
        if href.startswith("#") and href != "#" and href[1:] not in page.ids:
            bad(f"line {a['line']}: link to missing fragment {href}")
        if not href:
            bad(f"line {a['line']}: <a> without href")
    for kind, ref, line in page.aria_refs:
        if ref not in page.ids:
            bad(f"line {line}: {kind} references missing id '{ref}'")
    for a in page.anchors:
        text = re.sub(r"\s+", " ", "".join(a["text"])).strip()
        if not text and not (a["attrs"].get("aria-label") or "").strip():
            bad(f"line {a['line']}: link has no accessible name (text or aria-label)")
    for attrs, line in page.buttons:
        if not (attrs.get("aria-label") or "").strip():
            notes.append(f"line {line}: <button> relies on its text content for a name")

    # ---- headings and landmarks
    h1 = [h for h in page.headings if h[0] == 1]
    if len(h1) != 1:
        bad(f"expected exactly one <h1>, found {len(h1)}")
    prev = 0
    for lvl, line, text in page.headings:
        if prev and lvl > prev + 1:
            bad(f"line {line}: heading level skips from h{prev} to h{lvl} ('{text}')")
        if not str(text).strip():
            bad(f"line {line}: empty h{lvl}")
        prev = lvl
    for lm in ("header", "main", "footer"):
        if lm not in page.landmarks:
            bad(f"no <{lm}> landmark")
    if "main" not in page.ids:
        bad("no element with id='main' for the skip link")
    fa = page.first_body_anchor
    if not fa or (fa["attrs"].get("href") or "") != "#main":
        bad("the first link in <body> must be a skip link to #main")
    labels = [l for l, _ in page.navs]
    for label, line in page.navs:
        if not (label or "").strip():
            bad(f"line {line}: <nav> without aria-label (several navs need distinct names)")
    dup_nav = {l for l in labels if l and labels.count(l) > 1}
    if dup_nav:
        bad(f"navs share the same aria-label: {sorted(dup_nav)}")
    for attrs, line in page.sections:
        if not attrs.get("aria-labelledby") and not attrs.get("aria-label"):
            bad(f"line {line}: <section> without aria-labelledby/aria-label")
    for has_summary, line in page.details:
        if not has_summary:
            bad(f"line {line}: <details> without <summary>")

    # ---- images and inline svg
    for attrs, line in page.imgs:
        if "alt" not in attrs:
            bad(f"line {line}: <img> without alt")
    for attrs, line in page.svgs:
        hidden = attrs.get("aria-hidden") == "true"
        named = attrs.get("role") == "img" and (attrs.get("aria-label") or "").strip()
        if not hidden and not named:
            bad(f"line {line}: <svg> is neither aria-hidden nor role=img with aria-label")
    for line in page.tables:
        notes.append(f"line {line}: <table> present; check headers manually")

    # ---- external links: rel noopener when they open elsewhere
    for a in page.anchors:
        href = a["attrs"].get("href") or ""
        if href.startswith(("http://", "https://")):
            if href.startswith("http://"):
                bad(f"line {a['line']}: plain http link {href}")
            if a["attrs"].get("target") == "_blank" and "noopener" not in (a["attrs"].get("rel") or ""):
                bad(f"line {a['line']}: target=_blank without rel=noopener")

    for n in notes:
        print(f"  note: {n}")
    if problems:
        print("\n".join(problems), file=sys.stderr)
        print(f"==> FAIL: site check ({len(problems)} problems)", file=sys.stderr)
        return 1
    print(f"==> site OK: {path} ({len(page.headings)} headings, {len(page.anchors)} links, "
          f"{len(page.resources)} same-origin resources, {len(page.ids)} ids)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
