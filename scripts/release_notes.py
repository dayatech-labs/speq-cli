#!/usr/bin/env python3
"""Release notes from the commits since the last release tag: the range next_version.py took the version from.

Writes into OUT_DIR:

  release-notes.md  GitHub Release body: every commit (merges left out), grouped by type, newest first
  chat-card.json    Google Chat webhook message (cards v2): the same groups, at most CARD_COMMITS commits

Usage: release_notes.py TAG REPO_URL OUT_DIR [--repo DIR]
"""
import html
import json
import pathlib
import sys

from next_version import git, last_tag, parse

SECTIONS = (
    ("breaking", "Breaking changes"),
    ("feat", "Features"),
    ("fix", "Fixes"),
    ("perf", "Performance"),
    ("refactor", "Refactoring"),
    ("other", "Other changes"),
)
# Keeps the card readable (and far below Chat's 32 KB message limit); the release notes list every commit.
CARD_COMMITS = 30


def commits(repo, previous):
    """Commits after `previous` (every commit when None), newest first, with the section each belongs in."""
    raw = git(repo, "log", f"{previous}..HEAD" if previous else "HEAD", "--no-merges", "--format=%H%x1f%an%x1f%s%x1f%b%x1e")
    found = []
    for record in raw.split("\x1e"):
        if not record.strip():
            continue
        sha, author, subject, body = record.lstrip("\n").split("\x1f", 3)
        parsed = parse(subject, body)
        if parsed is None:
            section, scope, summary = "other", None, subject
        else:
            kind, scope, summary, breaking = parsed
            section = "breaking" if breaking else kind if kind in dict(SECTIONS) else "other"
            if section == "other":
                scope, summary = None, subject
        found.append({"sha": sha, "author": author, "section": section, "scope": scope, "summary": summary})
    return found


def grouped(found):
    """(title, commits) of every non-empty section, in SECTIONS order."""
    return [(title, [c for c in found if c["section"] == key]) for key, title in SECTIONS if any(c["section"] == key for c in found)]


def markdown(found, url, previous, tag):
    out = []
    for title, group in grouped(found):
        out += [f"## {title}", ""]
        for c in group:
            scope = f"**{c['scope']}:** " if c["scope"] else ""
            out.append(f"- {scope}{c['summary']} ([{c['sha'][:7]}]({url}/commit/{c['sha']})) by {c['author']}")
        out.append("")
    changes = f"{url}/compare/{previous}...{tag}" if previous else f"{url}/commits/{tag}"
    out.append(f"**Full Changelog**: {changes}")
    return "\n".join(out) + "\n"


def card(found, url, previous, tag):
    def line(c):
        scope = f"<b>{html.escape(c['scope'])}:</b> " if c["scope"] else ""
        link = f'<a href="{html.escape(url)}/commit/{c["sha"]}">{c["sha"][:7]}</a>'
        return f"• {scope}{html.escape(c['summary'])} · {link} <font color=\"#80868b\">{html.escape(c['author'])}</font>"

    sections, shown = [], 0
    for title, group in grouped(found):
        room = group[: CARD_COMMITS - shown]
        if not room:
            break
        shown += len(room)
        text = "<br>".join(map(line, room))
        sections.append({"header": f"{title} ({len(group)})", "widgets": [{"textParagraph": {"text": text}}]})
    footer = []
    if shown < len(found):
        footer.append({"textParagraph": {"text": f"<i>…and {len(found) - shown} more in the release notes.</i>"}})
    changes = f"{url}/compare/{previous}...{tag}" if previous else f"{url}/commits/{tag}"
    footer.append({"textParagraph": {"text": "Upgrade: <b>speq upgrade</b>"}})
    footer.append(
        {
            "buttonList": {
                "buttons": [
                    {"text": "Release notes", "onClick": {"openLink": {"url": f"{url}/releases/tag/{tag}"}}},
                    {"text": "Compare", "onClick": {"openLink": {"url": changes}}},
                ]
            }
        }
    )
    sections.append({"widgets": footer})
    since = f"since {previous}" if previous else "first release"
    count = f"{len(found)} commit{'s' if len(found) != 1 else ''}"
    header = {"title": f"speq {tag} released", "subtitle": f"{count} {since} · installs verified on Linux, macOS, Windows"}
    return {"cardsV2": [{"cardId": f"speq-release-{tag}", "card": {"header": header, "sections": sections}}]}


def main():
    args = sys.argv[1:]
    repo = "."
    if "--repo" in args:
        i = args.index("--repo")
        repo = args[i + 1]
        del args[i : i + 2]
    if len(args) != 3:
        sys.exit(__doc__.strip().splitlines()[-1])
    tag, url, out = args
    previous = last_tag(repo)
    found = commits(repo, previous)
    out = pathlib.Path(out)
    out.mkdir(parents=True, exist_ok=True)
    (out / "release-notes.md").write_text(markdown(found, url, previous, tag))
    (out / "chat-card.json").write_text(json.dumps(card(found, url, previous, tag), ensure_ascii=False, indent=1) + "\n")


if __name__ == "__main__":
    main()
