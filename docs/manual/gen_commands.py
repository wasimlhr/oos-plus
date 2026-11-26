# SPDX-License-Identifier: GPL-2.0-or-later
"""Generates docs/manual/commands.json (the command reference of the SCPI Programming Manual) from the SCPI reference.

    python docs/manual/gen_commands.py [--date YYYY-MM-DD]
    python docs/manual/gen_commands.py --check

Inputs: docs/scpi/*.md (one Markdown table row per header: Command, Set/Query, Parameters and range, Default, Menu
equivalent, Notes), docs/scpi/setup-keys.md, ui/Cargo.toml (the firmware version) and static.json beside this script
(the hand-written front matter: chapter intro, dialect, the item/error appendices).

Nothing is edited by hand in commands.json: rerun this script after the reference changes, then build.py.
`--check` writes nothing and fails when the file is not what would be written now (the date of the edition aside);
tools/tests/test_manual_commands.py runs it with the host checks, so the file cannot fall behind the reference.

Brace forms in a Command cell are expanded into the concrete headers (`:CURSor:{MANual|TRACk|XY}:{AXValue|...}` is
21 headers). `<n>` stays a numeric suffix (`:CHANnel<n>:SCALe` is `:CHANnel1:SCALe` ... `:CHANnel4:SCALe`), as in every
programming manual. Facts only come from the reference. A row may have two more cells, What it does and See also
(docs/scpi/README.md, "How to read the tables"): the first is the command's description as written, the second the
headers of related commands, each of which must be a header of the reference. A row without them gets a one-line
summary built from the Menu equivalent column; tools/scpi-coverage.py counts those rows (`rows-action`), and the
count only goes down.
"""
import argparse
import datetime
import json
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
DOCS = ROOT / 'docs' / 'scpi'
OUT = HERE / 'commands.json'

# Group pages in reference order (the README's table).
PAGES = ['common', 'system', 'acquire', 'channel', 'timebase', 'trigger', 'waveform', 'measure', 'cursor', 'math',
         'reference', 'search', 'decode', 'logic', 'mask', 'spectrum', 'eye', 'power', 'bode', 'generator', 'extgen',
         'sim', 'display', 'layout', 'storage', 'logger', 'table', 'calibration']
# The pages of docs/scpi that are not a command group: the index, and the setup keys (an appendix).
OTHER = {'README', 'setup-keys'}

SPLIT = re.compile(r'(?<!\\)\|')
CODE = re.compile(r'`([^`]+)`')
CHOICE = re.compile(r'^[A-Za-z0-9_+.\-]+(?:\|[A-Za-z0-9_+.\-]+)+$')
UNITS = {'seconds', 'volts', 'hertz', 'percent', 'number', 'degrees'}
BOOLEAN = '{ON|OFF|1|0}'


def cells(line):
    return [p.strip().replace('\\|', '|') for p in SPLIT.split(line.strip())[1:-1]]


def plain_links(text):
    """Markdown links become their text (the PDF has no files to link to)."""
    return re.sub(r'\[([^\]]+)\]\([^)]+\)', r'\1', text)


def unquote(text):
    m = CODE.fullmatch(text.strip())
    return m.group(1) if m else text.strip()


def firmware_version():
    for line in (ROOT / 'ui' / 'Cargo.toml').read_text(encoding='utf-8').splitlines():
        m = re.match(r'version\s*=\s*"([^"]+)"', line)
        if m:
            return m.group(1)
    raise SystemExit('no version in ui/Cargo.toml')


# ---- headers --------------------------------------------------------------------------------------------------------

def header_forms(raw):
    """`:CURSor:{MANual|TRACk}:AX` names several headers; every brace group expands (their product)."""
    m = re.search(r'\{([^}]+)\}', raw)
    if not m:
        return [raw]
    return [f for alt in m.group(1).split('|') for f in header_forms(raw[:m.start()] + alt + raw[m.end():])]


def short_key(s):
    return re.sub(r'[a-z]', '', s).upper()


def keyword_regex(header):
    """Matches the header in long or short form, with any numeric suffix, as a script writes it."""
    out = []
    for part in re.split(r'(:)', header.rstrip('?')):
        if part == ':':
            out.append(':')
            continue
        if not part:
            continue
        if part.startswith('*'):
            out.append(re.escape(part))
            continue
        part = part.replace('[<n>]', '<n>')
        m = re.fullmatch(r'([A-Z]+)([a-z]*)((?:<n>)?)', part)
        if not m:
            return re.compile(r'(?!x)x')
        short, rest, suffix = m.groups()
        word = re.escape(short) + (f'(?:{re.escape(rest.upper())})?' if rest else '')
        out.append(word + (r'\d*' if suffix else ''))
    body = ''.join(out).replace(':', ':?', 1) if header.startswith(':') else ''.join(out)
    return re.compile(rf'^{body}(\?)?(?=\s|$)', re.IGNORECASE)


def optional_free(header):
    return re.sub(r'\[:[A-Za-z]+\]', '', header)


# ---- page parsing ---------------------------------------------------------------------------------------------------

def blocks(lines):
    """Markdown lines to blocks: ('p', text) ('ul', [items]) ('pre', text)."""
    out, para, items, i = [], [], [], 0

    def flush():
        nonlocal para, items
        if para:
            out.append(('p', plain_links(' '.join(para))))
            para = []
        if items:
            out.append(('ul', [plain_links(x) for x in items]))
            items = []
    while i < len(lines):
        ln = lines[i]
        if ln.startswith('```'):
            flush()
            j = i + 1
            while j < len(lines) and not lines[j].startswith('```'):
                j += 1
            out.append(('pre', '\n'.join(lines[i + 1:j])))
            i = j + 1
            continue
        if not ln.strip():
            flush()
        elif ln.startswith('- '):
            if para:
                flush()
            items.append(ln[2:].strip())
        elif ln.startswith('| '):
            flush()
        elif items and ln.startswith('  '):
            items[-1] += ' ' + ln.strip()
        else:
            if items:
                flush()
            para.append(ln.strip())
        i += 1
    flush()
    return out


def parse_page(name):
    text = (DOCS / f'{name}.md').read_text(encoding='utf-8')
    lines = text.splitlines()
    title = lines[0].lstrip('# ').strip()
    page = {'name': name, 'title': title, 'intro': [], 'sections': [], 'after': [], 'examples_text': ''}
    area, section = 'intro', None
    buf = []

    def close():
        nonlocal buf
        if area == 'intro':
            page['intro'] = blocks(buf)
        elif area == 'after':
            page['after'].extend(blocks(buf))
        buf = []
    for ln in lines[1:]:
        if ln.startswith('## '):
            close()
            h = ln[3:].strip()
            if h == 'Commands':
                area, section = 'commands', {'title': None, 'rows': []}
                page['sections'].append(section)
            else:
                area = 'after'
                page['after'].append(('h', h))
            continue
        if area == 'commands' and ln.startswith('### '):
            section = {'title': ln[4:].strip(), 'rows': []}
            if page['sections'][-1]['title'] is None and not page['sections'][-1]['rows']:
                page['sections'].pop()
            page['sections'].append(section)
            continue
        if area == 'commands':
            if ln.startswith('| `'):
                section['rows'].append(cells(ln))
            continue
        buf.append(ln)
    close()
    # the examples of the page, as program lines with their replies
    ex = []
    for b in re.findall(r'```(?:text)?\n(.*?)```', text.split('## Examples')[-1], re.S):
        ex.extend(b.splitlines())
    page['examples_text'] = ex
    return page


# ---- command records ------------------------------------------------------------------------------------------------

def humanize(header):
    last = re.split(r'[:*]', header.rstrip('?'))[-1]
    return re.sub(r'(?<=[a-z])(?=[A-Z])', ' ', last).strip() or header


def sentences(text):
    """Sentences of a cell; code spans are kept whole (`:MEASure:ITEM? <item>` has a question mark inside)."""
    spans = []

    def hide(m):
        spans.append(m.group(0))
        return f'@@{len(spans) - 1}@@'
    hidden = CODE.sub(hide, text.strip())
    parts = [p for p in re.split(r'(?<=[.!?])\s+(?=[A-Z`"<@])', hidden) if p]
    return [re.sub(r'@@(\d+)@@', lambda m: spans[int(m.group(1))], p) for p in parts]


def strip_period(s):
    return s.rstrip('.').strip()


def lead_of(params):
    m = re.match(r'`([^`]+)`\s*', params.strip())
    return (m.group(1).strip(), params.strip()[m.end():].strip()) if m else (None, params.strip())


def value_list(text):
    """`POSitive, NEGative` or `A`, `B` or `C`: a text that is nothing but a list of keywords is a complete value list."""
    t = text.strip().rstrip('.')
    if not t or ' ' in t.replace(', ', ',').replace(' or ', ',') and re.search(r'[a-z]{4,} [a-z]{3,}', t):
        return None
    parts = [p.strip().strip('`') for p in re.split(r',\s*|\s+or\s+|\|', t)]
    if len(parts) >= 2 and all(re.fullmatch(r'[A-Za-z0-9_+.\-]+', p) for p in parts):
        return parts
    return None


REAL = re.compile(r'^(Volts|Seconds|Hz|Hertz|dB|Degrees|Percent|Divisions|Number|Points|Bins|Samples|Count|Amps|Ohms|Watts)\b|\bRange -?\d', re.I)


def classify_lead(lead):
    """What a leading backticked token of the Parameters cell is: choice, placeholder, unit or nothing."""
    if lead is None:
        return None
    if CHOICE.match(lead):
        return 'choice'
    if lead.startswith(('<', '[', '"')) or lead == 'CONFIRM' or '..' in lead and not lead.isalpha():
        return 'form'
    if lead in UNITS:
        return 'unit'
    return None


def convert_example(line):
    """`cmd    -> reply` becomes the manual's `cmd    /*The query returns reply.*/`."""
    if '->' in line:
        cmd, reply = (x.strip() for x in line.split('->', 1))
        return f'{cmd}    /*The query returns {reply}.*/' if reply else cmd
    return line.strip()


def record(header, kind, params, default, menu, note, page_examples, version, ctx='', does='', also=''):
    base = header.rstrip('?')
    does = plain_links(does).strip()
    does = '' if does == '-' else does
    params, note, menu = plain_links(params), plain_links(note), plain_links(menu)
    lead, rest = lead_of(params)
    lk = classify_lead(lead)
    sents = sentences(rest if lk else params)
    # sentences that describe the reply go to the Return format
    ret = [s for s in sents if re.match(r'(The )?quer(y|ies)\b', s, re.I)]
    body = [s for s in sents if s not in ret]
    default = default.strip()
    has_default = default not in ('', '-')
    suffix = '<n>' in base

    set_form = '<value>'
    if lk in ('form',) or (lk == 'choice' and False):
        set_form = lead
    elif lk == 'unit':
        set_form = f'<{lead}>'
    elif lk == 'choice':
        set_form = '<value>'
    qargs = []
    for tok in CODE.findall(params):
        m = re.match(r'^(\S+)\?\s+(.+)$', tok)
        if m and short_key(m.group(1)) == short_key(base) and ' ' + m.group(2) not in qargs:
            qargs.append(' ' + m.group(2))
    taken = re.search(r'the query takes `([^`]+)`', params)
    if taken:
        qargs = [' ' + taken.group(1)]
    if kind == 'Query' and lk == 'form' and not qargs:
        qargs = [' ' + lead]
    qargs = qargs or ['']

    menu_remark = ''
    rec = {'header': header}
    if kind == 'Set / Query':
        rec['set'] = f'{base} {set_form}'
        rec['query'] = '\n'.join(f'{base}?{q}' for q in qargs)
    elif kind == 'Query':
        rec['query'] = '\n'.join(f'{base}?{q}' for q in qargs)
    elif kind == 'Set':
        rec['set'] = f'{base} {set_form}'
    else:
        rec['set'] = base + (f' {lead}' if lk == 'form' else '')

    where = re.sub(r'\*\*', '', re.sub(r'\[([^\]]+)\]\([^)]+\)', r'\1', menu)).replace('`', '').strip()
    has_menu = bool(where) and where != '-'
    leaf = where.split('>')[-1].strip() if has_menu else humanize(base)
    verb = {'Set / Query': 'Sets or queries', 'Query': 'Queries', 'Set': 'Sets', 'Action': 'Runs'}[kind]
    desc = f'{verb} {leaf}{ctx}'
    if has_menu and '>' in where:
        desc += f' (menu: {where})'
    desc += '.'
    if not does and kind in ('Action', 'Query') and lk != 'form' and body:
        # an action or query without a typed argument and without a sentence of its own: its parameter text is the
        # description
        desc = ' '.join(body)
        if has_menu:
            menu_remark = f'- Menu equivalent: {where}.'
        body = []
    elif not has_menu and body:
        desc = body[0] if body[0].endswith(('.', '!', '?')) else body[0] + '.'
    if does:
        # the row says what the command does: that is its description, and the menu item a remark
        desc = does
        if has_menu and not menu_remark:
            menu_remark = f'- Menu equivalent: {where}.'
    rec['desc'] = desc
    related = [h for h in CODE.findall(also) if h.startswith((':', '*'))]
    if related:
        rec['related'] = related

    prm = []
    if suffix or '[<n>]' in base:
        prm.append(['<n>', 'Integer', 'Numeric suffix of the header' + ('' if '[<n>]' not in base else ' (optional)') + f'; `{re.sub(r"<n>", "1", base)}` is `n` = 1', '—'])
    remarks = [menu_remark] if menu_remark else []
    if kind in ('Set / Query', 'Set') or (kind in ('Action', 'Query') and lk == 'form'):
        if lk == 'choice':
            vals = '{' + lead + '}'
            typ = 'Boolean' if lead.replace(' ', '').upper() in ('ON|OFF', 'ON|OFF|1|0') else 'Discrete'
            if typ == 'Boolean':
                vals = BOOLEAN
            prm.append(['<value>', typ, vals, unquote(default) if has_default and kind == 'Set / Query' else '—'])
            remarks += ['- ' + s for s in body]
        else:
            if lk == 'form':
                name, typ = lead, 'Text'
                rng = ' '.join(body) or '—'
            elif lk == 'unit':
                name, typ = f'<{lead}>', 'Real'
                rng = f'In {lead}.' + (' ' + ' '.join(body) if body else '')
            elif re.match(r'^Boolean', rest if lk else params):
                name, typ, rng = '<value>', 'Boolean', BOOLEAN
                remarks += ['- ' + s for s in body if not re.match(r'^Boolean\.?$', s)]
            else:
                name, typ, rng = '<value>', ('Real' if REAL.search(' '.join(body)) else 'Text'), ' '.join(body) or '—'
                vl = value_list(' '.join(body))
                if vl:
                    typ, rng = 'Discrete', '{' + '|'.join(vl) + '}'
            if lk == 'form' and re.fullmatch(r'<(wave|channel|source|Rn)>', name):
                typ = 'Discrete'
            prm.append([name, typ, rng, unquote(default) if has_default and kind == 'Set / Query' else '—'])
    elif body:
        remarks += ['- ' + s for s in body]
    if prm:
        rec['params'] = prm

    if re.search(r'\bitem\b', lead or '') or (lk == 'form' and '<item>' in (lead or '')):
        ref = {':EYE': 'Appendix C', ':MEAS': 'Appendices A and B'}.get(next((k for k in (':EYE', ':MEAS') if base.upper().startswith(k)), ''), 'Appendix D')
        remarks.append(f'- The items are listed in {ref}.')
    if re.search(r'\bCONFIRM\b', params):
        remarks.append('- Needs the literal `CONFIRM` argument; without it the command fails with -221 and does nothing. Ask the user before sending it.')
    n = re.sub(r'\*\*', '', plain_links(note)).strip()
    n = re.sub(r',? not listed in the coverage report', ', not yet checked against the DHO guide', n)
    n = re.sub(r',? flagged \"?verify\"? by the coverage report', '', n)
    n = re.sub(r'\s*One row for the \d+ query headers\.', '', n)
    if n and n != '-':
        remarks.append(f'- Origin: {n}')
    if remarks:
        rec['remarks'] = remarks

    r = []
    if kind in ('Set / Query', 'Query'):
        r += ret
        if has_default:
            r.append(f'On a freshly started default model the query returns `{unquote(default)}`.')
    if r:
        rec['returns'] = ' '.join(r)

    if 'OOS extension' in note:
        rec['ext'] = True
    if re.search(r'\bCONFIRM\b', params):
        rec['confirm'] = True

    rx, rx_full = keyword_regex(optional_free(header)), keyword_regex(header)
    found = []
    for e in page_examples:
        cmd = e.split('->')[0].strip()
        if (rx.search(cmd) or rx_full.search(cmd)) and convert_example(e) not in found:
            found.append(convert_example(e))
    if found:
        rec['example'] = '\n'.join(found[:5])
    return rec


def build_subsystems(version):
    subs, seen = [], {}
    for name in PAGES:
        page = parse_page(name)
        examples = page['examples_text']
        multi = len([s for s in page['sections'] if s['rows']]) > 1
        first = True
        for sec in page['sections']:
            if not sec['rows']:
                continue
            title = page['title'] if not multi or not sec['title'] else f"{page['title']}: {sec['title']}"
            cmds = []
            for row in sec['rows']:
                if len(row) not in (6, 8):
                    raise SystemExit(f'{name}: expected 6 or 8 cells, got {len(row)}: {row}')
                raw, kind, params, default, menu, note = row[:6]
                does, also = row[6:] or ('', '')
                forms = header_forms(unquote(raw))
                if len(forms) > 1 and does not in ('', '-'):
                    raise SystemExit(f'{name}: {raw} is {len(forms)} headers with one sentence: give each its row')
                for h in forms:
                    if h.upper() in seen:
                        raise SystemExit(f'duplicate header {h} ({name} and {seen[h.upper()]})')
                    seen[h.upper()] = name
                    ctx = f' for {sec["title"]}' if multi and sec['title'] else ''
                    cmds.append(record(h, kind, params, default, menu, note, examples, version, ctx, does, also))
            subs.append({'title': title, 'page': name, 'intro': page['intro'] if first else [], 'commands': cmds, 'after': []})
            first = False
        if subs and subs[-1]['page'] == name:
            subs[-1]['after'] = page['after']
    # a See also cell names headers of the reference as their rows write them, nothing else
    for s in subs:
        for c in s['commands']:
            for h in c.get('related', []):
                if h.upper() not in seen:
                    raise SystemExit(f"{s['page']}: {c['header']} names {h} in See also: no header of the reference")
                if h.upper() == c['header'].upper():
                    raise SystemExit(f"{s['page']}: {c['header']} names itself in See also")
    return subs


def blk(b):
    return {'p': b[1]} if b[0] == 'p' else {'ul': b[1]} if b[0] == 'ul' else {'pre': b[1]} if b[0] == 'pre' else {'h': b[1]}


def jsonable(subs, version):
    for s in subs:
        s['intro'] = [blk(b) for b in s['intro']]
        s['after'] = [blk(b) for b in s['after']]
        del s['page']
        s['commands'] = [json.loads(json.dumps(c).replace('\\u2014', '—')) for c in s['commands']]
        for c in s['commands']:
            for k in ('desc', 'returns', 'example', 'set', 'query'):
                if k in c:
                    c[k] = re.sub(r'oos-ui \d+\.\d+\.\d+', f'oos-ui {version}', c[k])
            if 'params' in c:
                c['params'] = [[re.sub(r'oos-ui \d+\.\d+\.\d+', f'oos-ui {version}', str(x)) for x in p] for p in c['params']]
    return subs


def setup_keys():
    """The setup-keys appendix: one table per section of docs/scpi/setup-keys.md."""
    text = (DOCS / 'setup-keys.md').read_text(encoding='utf-8')
    head, *sections = re.split(r'^## ', text, flags=re.M)
    intro = [blk(b) for b in blocks(head.splitlines()[1:])]
    tables, count = [], 0
    for sec in sections:
        lines = sec.splitlines()
        title = lines[0].strip()
        rows, prev = [], None
        for ln in lines[1:]:
            if not ln.startswith('| `'):
                continue
            key, value, default, header = cells(ln)
            count += 1
            rows.append([key, 'as above' if value == prev else plain_links(value), default.replace('`-`', '-'), header])
            prev = value
        if rows:
            tables.append({'title': title, 'columns': ['Key', 'Value', 'Default', 'Equivalent header'], 'rows': rows})
    return {'title': 'Setup Keys', 'intro': intro, 'tables': tables}, count


def unlisted_pages():
    """Pages of docs/scpi that PAGES does not name: the commands of such a page would not be in the manual."""
    return sorted(f.stem for f in DOCS.glob('*.md') if f.stem not in PAGES and f.stem not in OTHER)


def build(date):
    """The content of commands.json for an edition of this date, and the line that says how much it holds."""
    missing = unlisted_pages()
    if missing:
        raise SystemExit(f'docs/scpi/{missing[0]}.md is in neither PAGES nor OTHER of docs/manual/gen_commands.py')
    version = firmware_version()
    static = json.loads((HERE / 'static.json').read_text(encoding='utf-8'))
    subs = jsonable(build_subsystems(version), version)
    keys, nkeys = setup_keys()
    appendices = static['appendices'] + [keys]
    data = {
        'meta': {'software': f'oos-ui {version}', 'version': version, 'edition': date.strftime('%B %Y'),
                 'date': f'{date.day} {date.strftime("%B %Y")}', 'dialect': static['dialect'],
                 'commands_chapter': static['commands_chapter']},
        'intro': static['intro'], 'subsystems': subs, 'appendices': appendices,
    }
    n = sum(len(s['commands']) for s in subs)
    return data, f'oos-ui {version}, {len(subs)} subsystems, {n} headers, {nkeys} setup keys'


def difference(have, want):
    """In a few words, what the content `have` of a commands.json lacks of `want`. None: nothing."""
    if have == want:
        return None
    ours, theirs = have.get('meta', {}).get('version'), want['meta']['version']
    if ours != theirs:
        return f'it is of oos-ui {ours}, the tree is {theirs}'
    old = {c['header']: c for s in have.get('subsystems', []) for c in s['commands']}
    new = {c['header']: c for s in want['subsystems'] for c in s['commands']}
    kinds = (('not in it', [h for h in new if h not in old]),
             ('no longer in the reference', [h for h in old if h not in new]),
             ('changed', [h for h in new if h in old and new[h] != old[h]]))
    parts = []
    for what, headers in kinds:
        if headers:
            parts.append(f'{len(headers)} {what} ({", ".join(headers[:4])}{", ..." if len(headers) > 4 else ""})')
    return '; '.join(parts) or 'a text beside the commands changed (a page introduction, an appendix or the setup keys)'


def stale():
    """Why commands.json is not what this script would write now, or None. The edition keeps the date the file has:
    a check on a later day finds no difference."""
    try:
        have = json.loads(OUT.read_text(encoding='utf-8'))
        want, _ = build(datetime.date.today())
    except (OSError, ValueError) as e:
        return f'{OUT.name} cannot be read: {e}'
    except SystemExit as e:
        return str(e)
    for k in ('edition', 'date'):
        want['meta'][k] = have.get('meta', {}).get(k)
    return difference(have, want)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--date', default=datetime.date.today().isoformat())
    ap.add_argument('--check', action='store_true', help='write nothing; fail when commands.json would change')
    a = ap.parse_args()
    if a.check:
        why = stale()
        if why:
            raise SystemExit(f'commands.json is behind the reference: {why}\nrun: python docs/manual/gen_commands.py')
        print('commands.json: up to date')
        return
    data, summary = build(datetime.date.fromisoformat(a.date))
    # LF on every system, as the tree has it
    with open(OUT, 'w', encoding='utf-8', newline='\n') as f:
        f.write(json.dumps(data, indent=1, ensure_ascii=False) + '\n')
    print(f'commands.json: {summary}')


if __name__ == '__main__':
    main()
