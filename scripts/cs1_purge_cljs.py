#!/usr/bin/env python3
"""
scripts/cs1_purge_cljs.py

Phase CS1 — Strip ClojureScript reader conditionals and rename .cljc → .clj.

Operation:
  1. For every .cljc file under common/:
     a. Strip all #? and #?@ reader conditionals, keeping the :clj branch.
     b. Remove the file, write the result as .clj.

Reader conditional rules:
  #?(:clj  E)              → E
  #?(:cljs E)              → <removed>
  #?(:clj  E1 :cljs E2)   → E1
  #?(:cljs E1 :clj  E2)   → E2
  #?@(:clj  [a b c])       → a b c   (spliced, no vector wrapper)
  #?@(:cljs [a b c])       → <removed>
  #?@(:clj  [a b c] :cljs [d]) → a b c

After stripping:
  - Consecutive blank lines are collapsed to one.
  - Lines that become entirely whitespace are left as single blank lines
    (the collapsing pass takes care of multiples).

Usage:
    cd /path/to/Logos
    python3 scripts/cs1_purge_cljs.py [--dry-run]
"""

import os
import re
import sys
import glob
import argparse


# ---------------------------------------------------------------------------
# Form-end finder
# ---------------------------------------------------------------------------

def find_matching(text: str, start: int, open_ch: str, close_ch: str) -> int:
    """Return index of the closing delimiter matching the one at text[start]."""
    depth = 0
    i = start
    in_str = False
    esc = False
    while i < len(text):
        c = text[i]
        if esc:
            esc = False
        elif c == '\\' and in_str:
            esc = True
        elif c == '"' and not in_str:
            in_str = True
        elif c == '"' and in_str:
            in_str = False
        elif not in_str:
            if c == open_ch:
                depth += 1
            elif c == close_ch:
                depth -= 1
                if depth == 0:
                    return i
        i += 1
    return -1  # unmatched


def form_end(text: str, start: int) -> int:
    """
    Return the exclusive end index of the Clojure form starting at text[start].
    Handles: (, [, {, strings, regex, sets, anon-fn, symbols, keywords,
              numbers, char literals, metadata ^, quote/unquote, deref.
    """
    if start >= len(text):
        return start
    c = text[start]

    # Paired delimiters
    if c == '(':
        end = find_matching(text, start, '(', ')')
        return end + 1 if end != -1 else len(text)
    if c == '[':
        end = find_matching(text, start, '[', ']')
        return end + 1 if end != -1 else len(text)
    if c == '{':
        end = find_matching(text, start, '{', '}')
        return end + 1 if end != -1 else len(text)

    # String
    if c == '"':
        j = start + 1
        while j < len(text):
            if text[j] == '\\':
                j += 2
            elif text[j] == '"':
                return j + 1
            else:
                j += 1
        return len(text)

    # Dispatch reader macros starting with #
    if c == '#' and start + 1 < len(text):
        nxt = text[start + 1]
        if nxt == '"':          # regex literal #"..."
            j = start + 2
            while j < len(text):
                if text[j] == '\\':
                    j += 2
                elif text[j] == '"':
                    return j + 1
                else:
                    j += 1
            return len(text)
        if nxt == '{':          # set #{...}
            end = find_matching(text, start + 1, '{', '}')
            return end + 1 if end != -1 else len(text)
        if nxt == '(':          # anonymous function #(...)
            end = find_matching(text, start + 1, '(', ')')
            return end + 1 if end != -1 else len(text)
        if nxt == '?':          # nested reader conditional — recurse
            return _reader_cond_end(text, start)
        if nxt == '_':          # discard macro #_
            j = start + 2
            while j < len(text) and text[j] in ' \t\n\r':
                j += 1
            return form_end(text, j)
        # Fall through: treat as symbol

    # Prefix single-character readers: quote, syntax-quote, unquote-splicing,
    # unquote, deref, metadata
    if c in ("'", '`', '@'):
        j = start + 1
        while j < len(text) and text[j] in ' \t\n\r':
            j += 1
        return form_end(text, j)
    if c == '~':
        j = start + 1
        if j < len(text) and text[j] == '@':
            j += 1
        while j < len(text) and text[j] in ' \t\n\r':
            j += 1
        return form_end(text, j)
    if c == '^':
        # metadata: ^meta form
        j = start + 1
        while j < len(text) and text[j] in ' \t\n\r':
            j += 1
        meta_end = form_end(text, j)
        while meta_end < len(text) and text[meta_end] in ' \t\n\r':
            meta_end += 1
        return form_end(text, meta_end)

    # Character literal: \a  \space  \newline etc.
    if c == '\\':
        j = start + 1
        if j < len(text):
            # named chars or unicode
            if text[j].isalpha():
                while j < len(text) and text[j] not in ' \t\n\r,([{"\')]})':
                    j += 1
            else:
                j += 1
        return j

    # Comment to end of line
    if c == ';':
        j = start
        while j < len(text) and text[j] != '\n':
            j += 1
        return j  # excludes newline (caller advances past it)

    # Symbol / keyword / number: everything up to whitespace or delimiter
    j = start
    while j < len(text) and text[j] not in ' \t\n\r,([{"\')]}':
        j += 1
    return j


def _reader_cond_end(text: str, start: int) -> int:
    """Return exclusive end of __entire__ #?(…) or #?@(…) form at text[start]."""
    j = start + 2  # past '#?'
    if j < len(text) and text[j] == '@':
        j += 1
    if j < len(text) and text[j] == '(':
        end = find_matching(text, j, '(', ')')
        return end + 1 if end != -1 else len(text)
    return j  # malformed


# ---------------------------------------------------------------------------
# Branch extractor
# ---------------------------------------------------------------------------

def parse_branches(text: str, paren_start: int):
    """
    Parse text[paren_start] == '(' and return:
       { ':clj': 'text-of-value', ':cljs': 'text-of-value' }
    for whichever keys are present.
    """
    close = find_matching(text, paren_start, '(', ')')
    if close == -1:
        return {}, paren_start + 1

    inner_start = paren_start + 1
    inner_end = close
    inner = text  # we work in the original text, using absolute indices
    i = inner_start
    branches = {}

    while i < inner_end:
        # Skip whitespace/commas
        while i < inner_end and inner[i] in ' \t\n\r,':
            i += 1
        if i >= inner_end:
            break

        # Expect a keyword
        if inner[i] != ':':
            break
        j = i + 1
        while j < inner_end and inner[j] not in ' \t\n\r,([{"\')]}':
            j += 1
        key = inner[i:j]
        i = j

        # Skip whitespace
        while i < inner_end and inner[i] in ' \t\n\r,':
            i += 1
        if i >= inner_end:
            break

        # Grab the value form
        val_start = i
        val_end = form_end(text, val_start)
        branches[key] = text[val_start:val_end]
        i = val_end

    return branches, close


# ---------------------------------------------------------------------------
# Main substitution
# ---------------------------------------------------------------------------

def strip_reader_conditionals(source: str) -> str:
    """
    Return `source` with all #? and #?@ reader conditionals resolved for :clj.
    Processes innermost forms first (right-to-left pass avoids index drift).
    """
    # Collect all (start, end, replacement) operations.
    ops = []
    i = 0
    while i < len(source):
        if source[i] == '#' and i + 1 < len(source) and source[i + 1] == '?':
            splice = False
            j = i + 2
            if j < len(source) and source[j] == '@':
                splice = True
                j += 1
            if j < len(source) and source[j] == '(':
                branches, close = parse_branches(source, j)
                full_end = close + 1

                clj_val = branches.get(':clj', None)

                if clj_val is not None:
                    if splice:
                        v = clj_val.strip()
                        # Unwrap vector [a b c] → a b c
                        if v.startswith('[') and v.endswith(']'):
                            v = v[1:-1].strip()
                    else:
                        v = clj_val
                    ops.append((i, full_end, v))
                else:
                    # :cljs-only — remove entirely
                    ops.append((i, full_end, ''))

                i = full_end
                continue
        i += 1

    # Apply in reverse order so earlier indices stay valid
    for start, end, repl in reversed(ops):
        source = source[:start] + repl + source[end:]

    # Collapse runs of blank lines (>1 consecutive blank → single blank)
    source = re.sub(r'\n{3,}', '\n\n', source)

    return source


# ---------------------------------------------------------------------------
# File processing
# ---------------------------------------------------------------------------

def process(path: str, dry_run: bool) -> tuple[str, bool, bool]:
    """
    Returns (new_path, was_renamed, had_conditionals).
    """
    with open(path, 'r', encoding='utf-8') as f:
        src = f.read()

    new_src = strip_reader_conditionals(src)
    had = new_src != src

    new_path = path[:-5] + '.clj' if path.endswith('.cljc') else path

    if not dry_run:
        with open(new_path, 'w', encoding='utf-8') as f:
            f.write(new_src)
        if new_path != path:
            os.remove(path)

    return new_path, new_path != path, had


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--dry-run', action='store_true',
                    help='Print what would be done without writing any files')
    ap.add_argument('root', nargs='?', default='common',
                    help='Root directory to scan (default: common)')
    args = ap.parse_args()

    files = sorted(glob.glob(f'{args.root}/**/*.cljc', recursive=True))
    if not files:
        print(f'No .cljc files found under {args.root}/')
        return

    renamed = stripped = 0
    for f in files:
        new_path, was_renamed, had_cond = process(f, args.dry_run)
        tag = ''
        if was_renamed:
            tag += 'R'
            renamed += 1
        if had_cond:
            tag += 'S'
            stripped += 1
        if not tag:
            tag = ' '
        print(f'  [{tag}] {f}')

    prefix = '[DRY RUN] ' if args.dry_run else ''
    print(f'\n{prefix}Renamed: {renamed}  Stripped conditionals: {stripped}  '
          f'Total: {len(files)}')


if __name__ == '__main__':
    main()
