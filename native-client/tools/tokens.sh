#!/bin/bash
# Lists the numbers typed in the screens (ui/*.slint but the theme and the
# generated icons), and exits non-zero if there are any (DESIGN.md, rule 2:
# every size and colour is a token from ui/theme.slint). It flags:
#   - a number with a unit (px phx rem % ms s deg turn rad), but 0 and ±1px
#     hairlines, a gradient's 100% and the directions 90/180/270deg; negative and leading-dot
#     ones too (-6px, .5px);
#   - any decimal, unit or not (0.86, .55, the numbers in cubic-bezier(),
#     opacity: c ? 0.6 : 1);
#   - an integer above 2 multiplied or divided with (20 * 1px, sp-8 * 3), but
#     100 (a percentage); plain index and counting math (i == 2, / 2) is fine;
#   - a colour: #hex, rgb()/rgba(), hsl()/hsla(), a named colour (white,
#     black, red… and Colors.*; transparent is no colour);
#   - any number given to opacity, font-weight, line-height-factor or
#     letter-spacing.
# Comments and strings don't count.
#
#   native-client/tools/tokens.sh
cd "$(dirname "$0")/../ui"
python3 - *.slint <<'PY'
import re, sys
UNITS = r'(?:px|phx|rem|%|ms|s|deg|turn|rad)'
unit = re.compile(r'(?<![\w.])(-?)(\d*\.\d+|\d+)(' + UNITS + r')(?![\w])')
dec = re.compile(r'(?<![\w.#])-?\d*\.\d+(?![\w.])')
mul = re.compile(r'(?:[*/]\s*(\d+)(?![\w.]))|(?:(?<![\w.#-])(\d+)\s*[*/])')
colour = re.compile(r'#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?)\s*\(')
NAMES = ('aliceblue antiquewhite aqua aquamarine azure beige bisque black blanchedalmond blue blueviolet brown '
    'burlywood cadetblue chartreuse chocolate coral cornflowerblue cornsilk crimson cyan darkblue darkcyan '
    'darkgoldenrod darkgray darkgreen darkgrey darkkhaki darkmagenta darkolivegreen darkorange darkorchid darkred '
    'darksalmon darkseagreen darkslateblue darkslategray darkslategrey darkturquoise darkviolet deeppink '
    'deepskyblue dimgray dimgrey dodgerblue firebrick floralwhite forestgreen fuchsia gainsboro ghostwhite gold '
    'goldenrod gray green greenyellow grey honeydew hotpink indianred indigo ivory khaki lavender lavenderblush '
    'lawngreen lemonchiffon lightblue lightcoral lightcyan lightgoldenrodyellow lightgray lightgreen lightgrey '
    'lightpink lightsalmon lightseagreen lightskyblue lightslategray lightslategrey lightsteelblue lightyellow lime '
    'limegreen linen magenta maroon mediumaquamarine mediumblue mediumorchid mediumpurple mediumseagreen '
    'mediumslateblue mediumspringgreen mediumturquoise mediumvioletred midnightblue mintcream mistyrose moccasin '
    'navajowhite navy oldlace olive olivedrab orange orangered orchid palegoldenrod palegreen paleturquoise '
    'palevioletred papayawhip peachpuff peru pink plum powderblue purple rebeccapurple red rosybrown royalblue '
    'saddlebrown salmon sandybrown seagreen seashell sienna silver skyblue slateblue slategray slategrey snow '
    'springgreen steelblue tan teal thistle tomato turquoise violet wheat white whitesmoke yellow yellowgreen').split()
named = re.compile(r'\bColors\.[\w-]+|(?<![\w.-])(?:' + '|'.join(NAMES) + r')(?![\w-])')
prop = re.compile(r'\b(opacity|font-weight|line-height-factor|letter-spacing)\s*:\s*[^;]*?(?<![\w.-])-?\d')
bad = 0
for path in sys.argv[1:]:
    if path in ('theme.slint', 'icons.slint'): continue
    text = open(path, encoding='utf-8').read()
    text = re.sub(r'/\*.*?\*/', lambda m: '\n' * m.group(0).count('\n'), text, flags=re.S)
    for n, line in enumerate(text.split('\n'), 1):
        code = re.sub(r'"(?:[^"\\]|\\.)*"', '""', line).split('//', 1)[0]
        found = []
        for m in unit.finditer(code):
            v = float(m.group(2))
            if v == 0 or (m.group(3) == 'px' and v == 1): continue
            if m.group(3) == 'deg' and v in (90, 180, 270): continue
            if m.group(3) == '%' and v == 100: continue
            found.append(m.group(0))
        found += [m.group(0) for m in dec.finditer(code) if not any(m.start() >= u.start() and m.end() <= u.end() for u in unit.finditer(code))]
        for m in mul.finditer(code):
            v = int(m.group(1) or m.group(2))
            if v > 2 and v != 100: found.append(m.group(0).strip())
        found += [m.group(0) for m in colour.finditer(code)]
        found += [m.group(0) for m in named.finditer(code)]
        for m in prop.finditer(code):
            # a 0 or 1 opacity is a switch, not a look
            tail = code[m.start():].split(';', 1)[0]
            nums = re.findall(r'(?<![\w.-])-?\d*\.?\d+', tail.split(':', 1)[1])
            # 0 is no number; a 0 or 1 opacity is a switch, not a look
            if all(float(x) == 0 for x in nums): continue
            if m.group(1) == 'opacity' and all(x in ('0', '1') for x in nums): continue
            found.append(m.group(0))
        if found:
            bad += 1
            print(f'{path}:{n}: {", ".join(dict.fromkeys(found))}    | {line.strip()[:100]}')
print(f'{bad} lines with typed numbers', file=sys.stderr)
sys.exit(1 if bad else 0)
PY
