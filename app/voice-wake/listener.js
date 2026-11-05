/* The hidden "Hey Crundi" window's page (see listener.html).
 *
 * Opens the microphone, has Chromium resample it to 16 kHz mono (an
 * AudioContext at 16000 Hz), taps it with an AudioWorklet in 100 ms pieces
 * and hands each piece to window.voiceWake.pcm (listener-preload.cjs), which
 * passes it to the spotter's process. Main starts and stops it with
 * window.voiceWake.onCommand('start' | 'stop'). */
(function () {
  'use strict';
  var api = window.voiceWake;
  var live = null;   // { stream, ctx }
  var starting = false;

  function stop() {
    var l = live; live = null;
    if (!l) return;
    try { l.stream.getTracks().forEach(function (t) { t.stop(); }); } catch (e) { /* gone */ }
    try { l.ctx.close(); } catch (e) { /* gone */ }
    api.state('stopped', '');
  }

  function start() {
    if (live || starting) return;
    starting = true;
    navigator.mediaDevices.getUserMedia({
      audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true, autoGainControl: true },
      video: false,
    }).then(function (stream) {
      var ctx = new AudioContext({ sampleRate: 16000 });
      return ctx.audioWorklet.addModule('worklet.js').then(function () {
        var src = ctx.createMediaStreamSource(stream);
        var tap = new AudioWorkletNode(ctx, 'crundi-wake-tap', { numberOfInputs: 1, numberOfOutputs: 1, channelCount: 1, channelCountMode: 'explicit' });
        // Pulled through a silent gain so the graph runs; nothing is played.
        var mute = ctx.createGain(); mute.gain.value = 0;
        src.connect(tap); tap.connect(mute); mute.connect(ctx.destination);
        tap.port.onmessage = function (e) { if (live) api.pcm(e.data); };
        // A device unplugged or taken away ends the track: say so, main retries.
        stream.getAudioTracks().forEach(function (t) { t.onended = function () { if (live && live.stream === stream) { stop(); api.state('error', 'The microphone went away'); } }; });
        live = { stream: stream, ctx: ctx };
        starting = false;
        api.state('listening', ctx.sampleRate + ' Hz');
      }, function (err) { stream.getTracks().forEach(function (t) { t.stop(); }); try { ctx.close(); } catch (e) { /* gone */ } throw err; });
    }).catch(function (err) {
      starting = false;
      api.state('error', (err && (err.name + ': ' + err.message)) || String(err));
    });
  }

  api.onCommand(function (cmd) { if (cmd === 'start') start(); else if (cmd === 'stop') stop(); });
  api.state('loaded', '');
})();
