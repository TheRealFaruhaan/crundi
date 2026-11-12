#!/usr/bin/env python3
"""
make-vad-weights.py: app/vendor/silero-vad-16k.bin from Silero VAD's ONNX model.

The speech detector that decides barge-in (the owner talking over Cru) is
Silero VAD v5 (MIT, https://github.com/snakers4/silero-vad), run by our own
code: app/vendor/crundi-voice.js createVad (web, PC) and
android/.../voice/SpeechDetector.kt (phone). Neither needs ONNX Runtime: the
16 kHz half of the model is a fixed STFT (a Hann-windowed DFT, computed in
code), four 1-D convolutions, one LSTM cell and a 1x1 convolution. This
script takes those weights out of the model, as float16, in the order the
readers expect.

  python3 -m venv /tmp/vad-venv && /tmp/vad-venv/bin/pip install onnx numpy
  curl -L -o /tmp/silero_vad.onnx \
    https://github.com/snakers4/silero-vad/raw/v5.1.2/src/silero_vad/data/silero_vad.onnx
  /tmp/vad-venv/bin/python scripts/make-vad-weights.py /tmp/silero_vad.onnx app/vendor/silero-vad-16k.bin

The model used: v5.1.2, SHA-256
2623a2953f6ff3d2c1e61740c6cdb7168133479b267dfef114a4a3cc5bdd788f.

File: "SVAD", uint32 LE version (1), then float16 LE, in this order:
  enc0 w [128,129,3] b [128]   kernel 3, pad 1, stride 1, ReLU
  enc1 w [64,128,3]  b [64]    stride 2
  enc2 w [64,64,3]   b [64]    stride 2
  enc3 w [128,64,3]  b [128]   stride 1
  lstm w_ih [512,128] w_hh [512,128] b [512] (b_ih + b_hh), gates i f g o
  out  w [128] b [1]           ReLU(h) . w + b, sigmoid
"""
import struct
import sys

import numpy as np
import onnx
from onnx import numpy_helper

src, dst = sys.argv[1], sys.argv[2]
m = onnx.load(src)
branch = [a.g for a in m.graph.node[2].attribute if a.name == 'then_branch'][0]  # sr == 16000
W = {}


def walk(g):
    for x in g.node:
        if x.op_type == 'Constant':
            W[x.output[0].split('Inline_0__')[-1]] = numpy_helper.to_array(x.attribute[0].t)
        for at in x.attribute:
            if at.type == onnx.AttributeProto.GRAPH:
                walk(at.g)


walk(branch)
P = 'encoder.%d.reparam_conv.'
parts = []
for i in range(4):
    parts += [W[P % i + 'weight'], W[P % i + 'bias']]
parts += [W['decoder.rnn.weight_ih'], W['decoder.rnn.weight_hh'], W['decoder.rnn.bias_ih'] + W['decoder.rnn.bias_hh'],
          W['decoder.decoder.2.weight'].reshape(-1), W['decoder.decoder.2.bias'].reshape(-1)]
flat = np.concatenate([p.astype(np.float32).ravel() for p in parts]).astype('<f2')
with open(dst, 'wb') as f:
    f.write(b'SVAD' + struct.pack('<I', 1) + flat.tobytes())
print(dst, flat.size, 'weights')
