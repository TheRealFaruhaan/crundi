#!/usr/bin/env python3
"""Summarise gdb-sample.py's samples.txt: busy vs idle samples, and for the
busy ones the innermost crundi view (owner) and the GPUI stage it is in.

    python3 -I native-v2/perf/gdb-summary.py /tmp/v2-prof/samples.txt
"""
import re, sys, collections

text = open(sys.argv[1], errors='replace').read()
samples = re.split(r'^=== \d+\n', text, flags=re.M)[1:]
IDLE = ('epoll_wait', 'poll ()', '__GI___poll', 'ppoll', 'wl_display_dispatch_queue', 'pthread_cond_wait', 'futex_wait')
STAGES = [('Window::draw', 'draw'), ('paint', 'paint'), ('prepaint', 'prepaint'), ('request_layout', 'layout'),
          ('compute_layout', 'layout'), ('layout_as_root', 'layout'), ('render', 'render')]
busy, idle = 0, 0
owner = collections.Counter(); stage = collections.Counter(); leaf = collections.Counter(); pair = collections.Counter()
fn_any = collections.Counter()
for s in samples:
    frames = [l for l in s.splitlines() if l.startswith('#')]
    if not frames: continue
    top = ' '.join(frames[:4])
    if any(k in top for k in IDLE): idle += 1; continue
    busy += 1
    m = re.search(r' in (\S+)', frames[0]); leaf[(m.group(1) if m else '?')[:70]] += 1
    crundi = None
    for f in frames:
        mm = re.search(r'(crundi_\w+::[\w:]+)', f)
        if mm: crundi = mm.group(1); break
    o = crundi or '(no crundi frame)'
    # The stage: the innermost gpui stage word below that owner.
    st = 'other'
    for f in frames:
        for k, v in STAGES:
            if k in f: st = v; break
        if st != 'other': break
    owner[o] += 1; stage[st] += 1; pair[(o, st)] += 1
    seen = set()
    for f in frames:
        mm = re.search(r'(crundi_\w+::[\w:]+|gpui::[\w:]+|gpui_component::[\w:]+|taffy::[\w:]+)', f)
        if mm and mm.group(1) not in seen: seen.add(mm.group(1)); fn_any[mm.group(1)] += 1
print(f'samples {busy + idle}: busy {busy}, idle {idle}')
print('\nstage:'); [print(f'  {n:4} {k}') for k, n in stage.most_common()]
print('\nowner x stage:'); [print(f'  {n:4} {o}  [{s}]') for (o, s), n in pair.most_common(25)]
print('\nleaf function:'); [print(f'  {n:4} {k}') for k, n in leaf.most_common(15)]
print('\nfunctions on the stack (inclusive):'); [print(f'  {n:4} {k}') for k, n in fn_any.most_common(40)]
