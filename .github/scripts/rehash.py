#!/usr/bin/env python3
"""Refreshes the ?v=<md5[:8]> cache-busting hashes in the site's html files.
Every script/style reference carries the first 8 hex digits of the file's md5, so returning visitors (and the
service worker) fetch a changed file instead of a stale cached one. Run it after editing anything under web/.
usage: rehash.py <web-dir> [relative/file ...]     (no file list = check every referenced file)"""
import glob, hashlib, os, re, sys
web = sys.argv[1]; only = set(sys.argv[2:])
ref = re.compile(r'((?:src|href)=")([^"?]+)\?v=([0-9a-f]{8})(")')
changed = 0
for page in sorted(glob.glob(os.path.join(web, '*.html'))):
    text = open(page, encoding='utf-8').read()
    def fix(m):
        global changed
        rel = m.group(2)
        path = os.path.join(web, rel)
        if (only and rel not in only) or not os.path.isfile(path): return m.group(0)
        h = hashlib.md5(open(path, 'rb').read()).hexdigest()[:8]
        if h != m.group(3): changed += 1; print(f"{os.path.basename(page)}: {rel} {m.group(3)} -> {h}")
        return f"{m.group(1)}{rel}?v={h}{m.group(4)}"
    new = ref.sub(fix, text)
    if new != text: open(page, 'w', encoding='utf-8').write(new)
print(f"{changed} hash(es) updated")
