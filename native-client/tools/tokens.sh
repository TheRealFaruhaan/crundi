#!/bin/bash
# Lists the numbers typed in the screens (ui/*.slint but the theme and the
# generated icons): lengths, durations, angles and percentages, colours, and
# literal weights, opacities and line heights. 0 and 1px hairlines are allowed
# (DESIGN.md, rule 2); comments and strings don't count. Nothing printed (and
# exit 0) means every number is a token from ui/theme.slint.
#
#   native-client/tools/tokens.sh
cd "$(dirname "$0")/../ui"
python3 - *.slint <<'PY'
import re, sys
bad = 0
num = re.compile(r'(?<![\w.#-])(\d+(?:\.\d+)?)(px|ms|s|deg|%)(?![\w])')
hexc = re.compile(r'#[0-9a-fA-F]{3,8}\b')
lit = re.compile(r'\b(font-weight|opacity|line-height-factor)\s*:\s*-?\d')
for path in sys.argv[1:]:
    if path in ('theme.slint', 'icons.slint'): continue
    for n, line in enumerate(open(path, encoding='utf-8'), 1):
        code = re.sub(r'"(?:[^"\\]|\\.)*"', '""', line)       # strings
        code = code.split('//', 1)[0]                          # comments
        found = [m.group(0) for m in num.finditer(code) if not (m.group(2) == 'px' and float(m.group(1)) in (0, 1))]
        found += hexc.findall(code) + [m.group(0) for m in lit.finditer(code)]
        if found:
            bad += 1
            print(f'{path}:{n}: {", ".join(found)}    | {line.strip()[:100]}')
print(f'{bad} lines with typed numbers', file=sys.stderr)
sys.exit(1 if bad else 0)
PY
