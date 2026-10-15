#!/usr/bin/env python3
"""Locator + verifier for docs/sourceReader/*.md against the Tabby repo.

Usage:
  sr_tools.py locate SYM [SYM...]     find symbol definitions across the repo
  sr_tools.py outline FILE [--top]    symbol skeleton + first docstring line of a file
  sr_tools.py offset FILE SYM         definition line number of SYM inside FILE
  sr_tools.py verify DOC [DOC...]     check file-path references in markdown docs
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPO = os.path.abspath(os.path.join(ROOT, "..", ".."))

SEARCH_ROOTS = [
    "crates", "ee", "clients", "python", "website", "ee/tabby-ui/src",
]
SKIP_DIRS = {".git", "target", "node_modules", "dist", "build", ".next",
             ".turbo", "out", "__pycache__", ".venv", "venv", ".progress",
             "llama.cpp"}
# fallback roots: only scanned when a symbol is not found in the production tree
FALLBACK_ROOTS = ["tests", "scripts", "docs"]

SRC_EXT = (".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".graphql", ".sql",
           ".toml", ".json", ".md", ".mdx", ".vue", ".yaml", ".yml", ".sh")

RUST_DEF = re.compile(
    r"^[ \t]*(?:#\[[^\]]*\][ \t]*)*"
    r"(?:pub(?:\([^)]*\))?[ \t]+)?"
    r"(?:default[ \t]+)?(?:async[ \t]+|unsafe[ \t]+|const[ \t]+)*?"
    r"\b(fn|struct|enum|trait|impl|type|mod|macro_rules!)\b"
    r"(?:[ \t]*<[^>]*>)?[ \t]+([A-Za-z_][A-Za-z0-9_]*)", re.M)
RUST_ITEM = re.compile(
    r"^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?"
    r"(?:const|static)[ \t]+([A-Z_][A-Za-z0-9_]*)", re.M)
TS_DEF = re.compile(
    r"^[ \t]*(?:export[ \t]+)?(?:default[ \t]+)?"
    r"(?:declare[ \t]+)?(?:async[ \t]+|function[ \t]+|class[ \t]+|"
    r"interface[ \t]+|type[ \t]+|enum[ \t]+|abstract[ \t]+)*"
    r"([A-Za-z_][A-Za-z0-9_]*)", re.M)
TS_NAMED = re.compile(
    r"^[ \t]*(?:export[ \t]+)?(?:const|let|var)[ \t]+"
    r"([A-Za-z_][A-Za-z0-9_]*)", re.M)


def walk(roots):
    for rel in roots:
        base = os.path.join(REPO, rel)
        if not os.path.isdir(base):
            continue
        for dirpath, dirnames, filenames in os.walk(base):
            dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
            for fn in filenames:
                if fn.endswith(SRC_EXT):
                    yield os.path.join(dirpath, fn)


def defs_of(path):
    """Return [(kind, name, lineno)] for definitions in a source file."""
    try:
        text = open(path, encoding="utf8", errors="replace").read()
    except OSError:
        return []
    rel = os.path.relpath(path, REPO)
    out = []
    if rel.endswith(".rs"):
        for m in RUST_DEF.finditer(text):
            out.append((m.group(1), m.group(2), text[:m.start()].count("\n") + 1))
        for m in RUST_ITEM.finditer(text):
            out.append(("const", m.group(1), text[:m.start()].count("\n") + 1))
    elif rel.endswith((".ts", ".tsx", ".js", ".jsx", ".vue")):
        for m in TS_DEF.finditer(text):
            out.append(("sym", m.group(1), text[:m.start()].count("\n") + 1))
        for m in TS_NAMED.finditer(text):
            out.append(("const", m.group(1), text[:m.start()].count("\n") + 1))
    elif rel.endswith(".py"):
        for m in re.finditer(r"^[ \t]*(?:async[ \t]+)?(def|class)[ \t]+(\w+)", text, re.M):
            out.append((m.group(1), m.group(2), text[:m.start()].count("\n") + 1))
    return out


def cmd_locate(syms):
    cache = []
    for path in walk(SEARCH_ROOTS):
        cache.append((path, defs_of(path)))
    fallback = []
    for sym in syms:
        last = sym.split(".")[-1]
        hits = [(os.path.relpath(p, REPO), k, n) for p, ds in cache
                for (k, n, _l) in ds if n == last]
        if not hits:
            if not fallback:
                fallback = [(p, defs_of(p)) for p in walk(FALLBACK_ROOTS)]
            hits = [(os.path.relpath(p, REPO), k, n) for p, ds in fallback
                    for (k, n, _l) in ds if n == last]
        if hits:
            print(f"{sym}:")
            seen = set()
            for p, k, n in hits:
                if (p, n) in seen:
                    continue
                seen.add((p, n))
                print(f"    {p}  [{k}]")
        else:
            print(f"{sym}:  \u2717 not found")


def cmd_outline(args):
    top = "--top" in args
    paths = [a for a in args if not a.startswith("--")]
    for p in paths:
        full = os.path.join(REPO, p)
        if not os.path.isfile(full):
            print(f"{p}: \u2717 missing")
            continue
        ds = defs_of(full)
        print(f"# {p}  ({sum(1 for _ in open(full, encoding='utf8', errors='replace'))} lines, {len(ds)} defs)")
        for k, n, ln in ds if not top else ds[:40]:
            print(f"  {ln:>5}  {k} {n}")


def cmd_offset(file_sym):
    if len(file_sym) != 2:
        print("offset FILE SYM")
        return
    f, sym = file_sym
    full = os.path.join(REPO, f)
    last = sym.split(".")[-1]
    for k, n, ln in defs_of(full):
        if n == last:
            print(f"{f}:{ln}  [{k}]")
            return
    print(f"{f} \u00b7 {sym}: \u2717 not found")


# --- verify ---------------------------------------------------------------

PATH_RE = re.compile(
    r"(?<![A-Za-z0-9_./-])([A-Za-z0-9_][A-Za-z0-9_./-]*"
    r"\.(?:rs|ts|tsx|js|jsx|vue|py|toml|md|mdx|graphql|json|sql|yaml|yml|sh|lock))\b")
NEGATIVE = ("不存在", "已删除", "已被移除", "已移除", "不再", "没有", "曾经", "旧版",
            "not exist", "removed", "deleted", "no longer", "obsolete")
RUNTIME_NAMES = {"config.toml", "tabby.json", ".gitignore", "package-lock.json",
                 "Cargo.lock", "tsconfig.json", "webpack.config.js", "vite.config.ts",
                 "docker-compose.yml", "docker-compose.yaml", ".env", "LICENSE",
                 "authorized_keys", "known_hosts", "id_rsa", "node.js", "config.yaml"}
PLACEHOLDER = ("YYYY", "MM-DD", "your_", "YourName", "xxx", "*", "<", "foo", "bar")
# repo-wide index of basenames + full paths, built once
_BASE_INDEX = None
_PATH_INDEX = None


def basename_index():
    global _BASE_INDEX
    if _BASE_INDEX is None:
        _BASE_INDEX = set()
        for p in walk(SEARCH_ROOTS):
            _BASE_INDEX.add(os.path.basename(p))
    return _BASE_INDEX


def path_index():
    global _PATH_INDEX
    if _PATH_INDEX is None:
        seen = set()
        _PATH_INDEX = []
        roots = list(SEARCH_ROOTS) + FALLBACK_ROOTS + [".github"]
        for p in walk(roots):
            rel = os.path.relpath(p, REPO)
            if rel not in seen:
                seen.add(rel)
                _PATH_INDEX.append(rel)
        for extra in (".github/workflows", ".github/ISSUE_TEMPLATE"):
            d = os.path.join(REPO, extra)
            if os.path.isdir(d):
                for fn in sorted(os.listdir(d)):
                    rel = f"{extra}/{fn}"
                    if rel not in seen:
                        seen.add(rel)
                        _PATH_INDEX.append(rel)
    return _PATH_INDEX


def suffix_matches(ref):
    ref_norm = ref.lower().lstrip("./")
    out = []
    for full in path_index():
        low = full.lower()
        if low == ref_norm or low.endswith("/" + ref_norm):
            out.append(full)
    return out



def resolve(ref, doc_dir):
    if os.path.isabs(ref):
        return os.path.exists(ref)
    chains = [REPO]
    parts = doc_dir.split(os.sep)
    cur = REPO
    for part in parts:
        cur = os.path.join(cur, part)
        chains.append(cur)
    for base in chains:
        if os.path.exists(os.path.join(base, ref)):
            return True
    for base in chains:
        if os.path.isdir(os.path.join(base, ref)):
            return True
    return False


def cmd_verify(docs):
    total = bad = 0
    buckets = {"negated": 0, "runtime": 0, "bare": 0, "placeholder": 0, "short": 0}
    shorts = []
    for doc in docs:
        doc = os.path.abspath(doc)
        doc_dir = os.path.relpath(os.path.dirname(doc), REPO)
        unresolved = []
        for lineno, line in enumerate(open(doc, encoding="utf8"), 1):
            for ref in PATH_RE.findall(line):
                total += 1
                if resolve(ref, doc_dir):
                    continue
                low = line.lower()
                matches = suffix_matches(ref)
                if any(t in line or t in low for t in NEGATIVE):
                    buckets["negated"] += 1
                elif any(t in ref for t in PLACEHOLDER):
                    buckets["placeholder"] += 1
                elif ref.lower() in RUNTIME_NAMES:
                    buckets["runtime"] += 1
                elif len(matches) == 1 and ("/" not in ref or matches[0].lower().endswith(ref.lower())):
                    # shortened / prefix-omitted but unambiguous path -> style, report separately
                    if "/" in ref:
                        buckets["short"] += 1
                        shorts.append((os.path.relpath(doc, REPO), lineno, ref, matches[0]))
                    else:
                        buckets["bare"] += 1
                elif "/" not in ref and os.path.basename(ref) in basename_index():
                    buckets["bare"] += 1
                elif len(matches) > 1:
                    bad += 1
                    unresolved.append((lineno, f"{ref}  -> ambiguous: {', '.join(matches[:4])}"))
                else:
                    unresolved.append((lineno, ref))
                    bad += 1
        name = os.path.relpath(doc, REPO)
        if unresolved:
            print(f"\u2717 {name}")
            for lineno, ref in unresolved:
                print(f"    L{lineno}: {ref}")
        else:
            print(f"\u2713 {name}")
    if shorts:
        print("\n~ prefix-omitted paths (style; expand if the doc claims a location):")
        for name, lineno, ref, full in shorts:
            print(f"    {name} L{lineno}: {ref}  ->  {full}")
    print(f"\nrefs={total} suspect={bad} "
          f"(negated={buckets['negated']} placeholder={buckets['placeholder']} "
          f"runtime-config={buckets['runtime']} bare-name={buckets['bare']} "
          f"shortened={buckets['short']})")


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return
    cmd, args = sys.argv[1], sys.argv[2:]
    if cmd == "locate":
        cmd_locate(args)
    elif cmd == "outline":
        cmd_outline(args)
    elif cmd == "offset":
        cmd_offset(args)
    elif cmd == "verify":
        cmd_verify(args)
    else:
        print(f"unknown command {cmd}")


main()
