#!/usr/bin/env python3
"""Verify that every relative link and image in the Markdown docs actually resolves.

CI skips all `**.md` changes (`paths-ignore` in ci.yml), so nothing catches a broken
cross-reference — which is how the docs reached the state this script was written to clean up.
Run it after moving or renaming anything under docs/.

Checks:
  * relative links point at a file that exists
  * `#anchors` match a real heading in the target file
  * relative <img src=...> and ![](...) targets exist
  * the effect catalogue in docs/GALLERY.md still matches assets/effects/*.pfx

Remote URLs are not fetched — this is a structural check, not a link-rot check.

Usage:  scripts/check_docs.py
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SKIP_DIRS = {".git", "target", "node_modules", ".venv", "scratch_lattice", "__pycache__"}

MD_LINK = re.compile(r"(?<!!)\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
MD_IMAGE = re.compile(r"!\[[^\]]*\]\(([^)\s]+)")
HTML_SRC = re.compile(r"<img[^>]*\ssrc=\"([^\"]+)\"", re.I)
HEADING = re.compile(r"^(#{1,6})\s+(.*?)\s*#*$", re.M)
# GitHub honours both `id=` and the older `name=` form on explicit anchors.
HTML_ANCHOR = re.compile(r"<a\s+(?:id|name)=\"([^\"]+)\"", re.I)


def slug(text: str) -> str:
    """GitHub's heading-anchor rule: lowercase, strip punctuation, spaces to hyphens."""
    text = re.sub(r"<[^>]+>", "", text)
    text = re.sub(r"[`*_~]", "", text).strip().lower()
    text = re.sub(r"[^\w\s-]", "", text)
    return re.sub(r"\s+", "-", text)


def anchors(path: Path) -> set[str]:
    text = path.read_text(encoding="utf-8", errors="replace")
    out: set[str] = set()
    seen: dict[str, int] = {}
    for _, title in HEADING.findall(text):
        s = slug(title)
        # GitHub disambiguates repeats as `-1`, `-2`, …
        n = seen.get(s, 0)
        out.add(s if n == 0 else f"{s}-{n}")
        seen[s] = n + 1
    out.update(HTML_ANCHOR.findall(text))
    return out


def md_files() -> list[Path]:
    return sorted(
        p for p in REPO.rglob("*.md")
        if not any(part in SKIP_DIRS for part in p.relative_to(REPO).parts)
    )


def check_links() -> list[str]:
    errors: list[str] = []
    anchor_cache: dict[Path, set[str]] = {}

    for md in md_files():
        text = md.read_text(encoding="utf-8", errors="replace")
        targets = [(m, "link") for m in MD_LINK.findall(text)]
        targets += [(m, "image") for m in MD_IMAGE.findall(text)]
        targets += [(m, "image") for m in HTML_SRC.findall(text)]

        for target, kind in targets:
            if target.startswith(("http://", "https://", "mailto:", "data:")):
                continue
            rel = md.relative_to(REPO)

            if target.startswith("#"):
                path, frag = md, target[1:]
            else:
                head, _, frag = target.partition("#")
                path = (md.parent / head).resolve()
                if not path.exists():
                    errors.append(f"{rel}: {kind} target does not exist -> {target}")
                    continue

            if frag and path.suffix == ".md":
                if path not in anchor_cache:
                    anchor_cache[path] = anchors(path)
                if frag.lower() not in {a.lower() for a in anchor_cache[path]}:
                    errors.append(f"{rel}: anchor not found -> {target}")
    return errors


def check_gallery() -> list[str]:
    gallery = REPO / "docs" / "GALLERY.md"
    if not gallery.exists():
        return ["docs/GALLERY.md is missing"]

    text = gallery.read_text()
    shipped = {}
    for p in sorted((REPO / "assets" / "effects").glob("*.pfx")):
        data = json.loads(p.read_text())
        if not data.get("hidden"):
            shipped[p.stem] = data["name"]

    errors = []
    for stem, name in shipped.items():
        if f">{name}</b>" not in text:
            errors.append(f"docs/GALLERY.md: shipped effect missing from the gallery -> {name}")
        if f"/{stem}.webp" not in text:
            errors.append(f"docs/GALLERY.md: no clip referenced for -> {stem}")

    claimed = re.search(r"\*\*(\d+) effects\.\*\*", text)
    if claimed and int(claimed.group(1)) != len(shipped):
        errors.append(
            f"docs/GALLERY.md claims {claimed.group(1)} effects, {len(shipped)} are shipped"
        )

    # The count that kept going stale, wherever it is asserted in prose.
    for doc in ("README.md", "docs/TUTORIALS.md"):
        p = REPO / doc
        if p.exists():
            for m in re.finditer(r"\*\*(\d+) built-in", p.read_text()):
                if int(m.group(1)) != len(shipped):
                    errors.append(f"{doc} claims {m.group(1)} built-in effects, not {len(shipped)}")
    return errors


def main() -> int:
    errors = check_links() + check_gallery()
    if errors:
        print(f"{len(errors)} problem(s):\n", file=sys.stderr)
        for e in errors:
            print(f"  {e}", file=sys.stderr)
        return 1
    print(f"docs OK — {len(md_files())} markdown files, all relative links and anchors resolve")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
