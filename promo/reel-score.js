"use strict";
/* =============================================================================
   SCORE — the Art Window reel's soundtrack (14.5 s).

   Sound design, not music: the language of a rendered product demo. No
   melody at all. Under everything, an ambient bed — a low A drone, a band
   of air, a slow cloud of quiet high partials breathing in and out. On top,
   sounds that belong to what the camera does:
     - air moves when the camera moves: a whoosh for every pass along the
       wall, every push-in and pull-out, panned the way things travel;
     - a riser builds under every reveal and resolves into a soft sub impact
       as the thing lands: the laptop, the phone, the icon;
     - one quiet shimmer, and only one: as the icon lands;
     - small, precise clicks for the menu, and a soft thock for each device
       taking its place in the family;
     - under it all a soft pulse at about 91 bpm, a muted clock the cuts land on,
       which stops as the icon lands.

   One classic script, no samples: every sound is synthesised from fixed
   seeds, so the live start() and the offline render agree. window.SCORE is
   the whole interface (start / renderOffline / toWav / duration), the same
   shape as the tapeshelf reel's, so tools/export.js drives it unchanged.
   Cue times come from reel-cues.js, loaded first; nothing here invents one.
   ============================================================================= */

(function () {

var C = window.CUES, DURATION = C.DURATION;

function mulberry32(a) {
  return function () {
    a |= 0; a = a + 0x6D2B79F5 | 0;
    var t = Math.imul(a ^ a >>> 15, 1 | a);
    t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t;
    return ((t ^ t >>> 14) >>> 0) / 4294967296;
  };
}
function gainOf(ctx, v) { var g = ctx.createGain(); g.gain.value = v; return g; }
function biq(ctx, type, f, q) {
  var b = ctx.createBiquadFilter(); b.type = type; b.frequency.value = f; if (q != null) b.Q.value = q; return b;
}
function panner(ctx, v) { var p = ctx.createStereoPanner(); p.pan.value = v || 0; return p; }

var NOISE = [];
function noise(ctx, secs, seed, kind) {
  for (var k = 0; k < NOISE.length; k++) {
    var e = NOISE[k];
    if (e.ctx === ctx && e.secs === secs && e.seed === seed && e.kind === kind) return e.buf;
  }
  var n = Math.max(1, Math.floor(ctx.sampleRate * secs)), buf = ctx.createBuffer(1, n, ctx.sampleRate);
  var d = buf.getChannelData(0), rnd = mulberry32(seed), last = 0, b0 = 0, b1 = 0, b2 = 0;
  for (var i = 0; i < n; i++) {
    var w = rnd() * 2 - 1;
    if (kind === 'brown') { last = (last + 0.023 * w) / 1.023; d[i] = last * 3.4; }
    else if (kind === 'pink') {
      b0 = 0.99765 * b0 + w * 0.0990460; b1 = 0.96300 * b1 + w * 0.2965164; b2 = 0.57000 * b2 + w * 0.1050186;
      d[i] = (b0 + b1 + b2 + w * 0.1848) * 0.28;
    } else d[i] = w;
  }
  NOISE.push({ ctx: ctx, secs: secs, seed: seed, kind: kind, buf: buf });
  return buf;
}
function src(ctx, secs, seed, kind, loop) {
  var s = ctx.createBufferSource(); s.buffer = noise(ctx, secs, seed, kind); s.loop = !!loop; return s;
}

/* --- the sounds ------------------------------------------------------------ */

/* Air moving past: pink noise through a wide band-pass that opens and closes,
   swelling to its peak just after the middle, travelling pan0 -> pan1. */
function whoosh(ctx, out, t, dur, amp, pan0, pan1, fLo, fHi, seed) {
  var s = src(ctx, dur + 0.1, seed, 'pink'), bp = biq(ctx, 'bandpass', fLo, 0.7), g = ctx.createGain(), p = panner(ctx, pan0);
  bp.frequency.setValueAtTime(fLo, t);
  bp.frequency.exponentialRampToValueAtTime(fHi, t + dur * 0.58);
  bp.frequency.exponentialRampToValueAtTime(fLo, t + dur);
  g.gain.setValueAtTime(0.0001, t);
  g.gain.exponentialRampToValueAtTime(amp, t + dur * 0.58);
  g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
  p.pan.setValueAtTime(pan0, t);
  p.pan.linearRampToValueAtTime(pan1, t + dur);
  s.connect(bp); bp.connect(g); g.connect(p); p.connect(out);
  s.start(t); s.stop(t + dur + 0.1);
}
/* Build-up into a reveal: noise whose high-pass climbs, and a quiet sine
   gliding up an octave under it, both growing until the moment they stop. */
function riser(ctx, out, t, dur, amp, seed) {
  var s = src(ctx, dur + 0.1, seed, 'white'), hp = biq(ctx, 'highpass', 250, 0.6), lp = biq(ctx, 'lowpass', 5000, 0.5);
  var g = ctx.createGain();
  hp.frequency.setValueAtTime(250, t);
  hp.frequency.exponentialRampToValueAtTime(2600, t + dur);
  g.gain.setValueAtTime(0.0001, t);
  g.gain.exponentialRampToValueAtTime(amp * 0.5, t + dur * 0.97);
  g.gain.linearRampToValueAtTime(0.0001, t + dur + 0.04);
  s.connect(hp); hp.connect(lp); lp.connect(g); g.connect(out);
  s.start(t); s.stop(t + dur + 0.1);
  var o = ctx.createOscillator(), og = ctx.createGain();
  o.type = 'sine';
  o.frequency.setValueAtTime(110, t);
  o.frequency.exponentialRampToValueAtTime(220, t + dur);
  og.gain.setValueAtTime(0.0001, t);
  og.gain.exponentialRampToValueAtTime(amp * 0.35, t + dur * 0.97);
  og.gain.linearRampToValueAtTime(0.0001, t + dur + 0.04);
  o.connect(og); og.connect(out); o.start(t); o.stop(t + dur + 0.1);
}
/* Something landing: a sub that drops in pitch, a soft low thock, and a
   breath of low noise that the room carries on. size scales all three. */
function impact(ctx, dry, room, t, size) {
  var o = ctx.createOscillator(), g = ctx.createGain();
  o.type = 'sine';
  o.frequency.setValueAtTime(66, t);
  o.frequency.exponentialRampToValueAtTime(36, t + 0.9);
  g.gain.setValueAtTime(0.0001, t);
  g.gain.linearRampToValueAtTime(0.9 * size, t + 0.012);
  g.gain.exponentialRampToValueAtTime(0.0001, t + 0.6 + 0.9 * size);
  o.connect(g); g.connect(dry); o.start(t); o.stop(t + 1.8);
  var th = ctx.createOscillator(), tg = ctx.createGain();
  th.type = 'sine';
  th.frequency.setValueAtTime(170, t);
  th.frequency.exponentialRampToValueAtTime(95, t + 0.12);
  tg.gain.setValueAtTime(0.0001, t);
  tg.gain.linearRampToValueAtTime(0.35 * size, t + 0.004);
  tg.gain.exponentialRampToValueAtTime(0.0001, t + 0.18);
  th.connect(tg); tg.connect(dry); th.start(t); th.stop(t + 0.25);
  var s = src(ctx, 1.4, 1200, 'pink'), lp = biq(ctx, 'lowpass', 420, 0.5), ng = ctx.createGain();
  ng.gain.setValueAtTime(0.0001, t);
  ng.gain.linearRampToValueAtTime(0.5 * size, t + 0.01);
  ng.gain.exponentialRampToValueAtTime(0.0001, t + 1.2);
  s.connect(lp); lp.connect(ng); ng.connect(room); s.start(t); s.stop(t + 1.4);
}
/* A picture materialising: a scatter of quiet, high, clean tones from the
   A-major overtone set, each a soft ping, spread across the stereo field.
   Used once, for the icon: more than that reads as a jingle. */
var SHIMMER = [880, 1108.7, 1318.5, 1760, 2217.5, 2637];
function shimmer(ctx, out, t, dur, amp, seed) {
  var rnd = mulberry32(seed), n = Math.round(8 + dur * 8);
  for (var i = 0; i < n; i++) {
    var at = t + Math.pow(rnd(), 1.6) * dur * 0.6, f = SHIMMER[Math.floor(rnd() * SHIMMER.length)];
    var len = 0.5 + rnd() * 1.1, o = ctx.createOscillator(), g = ctx.createGain(), p = panner(ctx, rnd() * 1.4 - 0.7);
    o.type = 'sine'; o.frequency.value = f * (1 + (rnd() - 0.5) * 0.004);
    g.gain.setValueAtTime(0.0001, at);
    g.gain.linearRampToValueAtTime(amp * (0.4 + rnd() * 0.6) / Math.sqrt(n), at + 0.008);
    g.gain.exponentialRampToValueAtTime(0.0001, at + len);
    o.connect(g); g.connect(p); p.connect(out); o.start(at); o.stop(at + len + 0.05);
  }
}
/* A precise interface click: a very short band of noise and a tiny blip. */
function uiClick(ctx, out, t, f, amp, seed) {
  var s = src(ctx, 0.03, seed, 'white'), bp = biq(ctx, 'bandpass', f * 1.4, 3.5), g = ctx.createGain();
  g.gain.setValueAtTime(0.0001, t);
  g.gain.linearRampToValueAtTime(amp, t + 0.0008);
  g.gain.exponentialRampToValueAtTime(0.0001, t + 0.012);
  s.connect(bp); bp.connect(g); g.connect(out); s.start(t); s.stop(t + 0.03);
  var o = ctx.createOscillator(), og = ctx.createGain();
  o.type = 'sine'; o.frequency.value = f;
  og.gain.setValueAtTime(0.0001, t);
  og.gain.linearRampToValueAtTime(amp * 0.25, t + 0.002);
  og.gain.exponentialRampToValueAtTime(0.0001, t + 0.05);
  o.connect(og); og.connect(out); o.start(t); o.stop(t + 0.07);
}
/* A device taking its place: a short, rounded thock, panned where it sits. */
function thock(ctx, out, t, f, amp, pan) {
  var o = ctx.createOscillator(), g = ctx.createGain(), p = panner(ctx, pan);
  o.type = 'sine';
  o.frequency.setValueAtTime(f, t);
  o.frequency.exponentialRampToValueAtTime(f * 0.62, t + 0.1);
  g.gain.setValueAtTime(0.0001, t);
  g.gain.linearRampToValueAtTime(amp, t + 0.003);
  g.gain.exponentialRampToValueAtTime(0.0001, t + 0.16);
  o.connect(g); g.connect(p); p.connect(out); o.start(t); o.stop(t + 0.2);
  var s = src(ctx, 0.02, 1300 + Math.round(f), 'pink'), lp = biq(ctx, 'lowpass', 900, 0.7), ng = ctx.createGain();
  ng.gain.setValueAtTime(0.0001, t);
  ng.gain.linearRampToValueAtTime(amp * 0.5, t + 0.001);
  ng.gain.exponentialRampToValueAtTime(0.0001, t + 0.015);
  s.connect(lp); lp.connect(ng); ng.connect(p); s.start(t); s.stop(t + 0.02);
}

/* The pulse: a muted low thump on every beat and a tiny tick between, like
   a precise clock the cuts land on. Every fourth beat leans a little
   harder. It skips any beat an impact already lands on, and stops for good
   as the icon lands, so the end rings out on its own. */
function pulse(ctx, out, t0, from) {
  var B = C.BEAT, stops = [0.02, C.formThud, C.swap[1], C.phoneLand];
  function taken(t) { for (var i = 0; i < stops.length; i++) if (Math.abs(stops[i] - t) < 0.05) return true; return false; }
  for (var n = 0; n * B < C.sun - 0.01; n++) {
    var t = n * B, grow = 0.55 + 0.45 * Math.min(1, t / C.pull[0]);
    if (t >= from - 0.01 && !taken(t)) {
      var tt = t0 + t, acc = n % 4 === 0 ? 1 : 0.72, o = ctx.createOscillator(), g = ctx.createGain();
      o.type = 'sine';
      o.frequency.setValueAtTime(78, tt);
      o.frequency.exponentialRampToValueAtTime(46, tt + 0.1);
      g.gain.setValueAtTime(0.0001, tt);
      g.gain.linearRampToValueAtTime(0.42 * acc * grow, tt + 0.004);
      g.gain.exponentialRampToValueAtTime(0.0001, tt + 0.16);
      o.connect(g); g.connect(out); o.start(tt); o.stop(tt + 0.2);
      var s = src(ctx, 0.02, 1500 + n, 'pink'), lp = biq(ctx, 'lowpass', 700, 0.7), ng = ctx.createGain();
      ng.gain.setValueAtTime(0.0001, tt);
      ng.gain.linearRampToValueAtTime(0.12 * acc * grow, tt + 0.001);
      ng.gain.exponentialRampToValueAtTime(0.0001, tt + 0.012);
      s.connect(lp); lp.connect(ng); ng.connect(out); s.start(tt); s.stop(tt + 0.02);
    }
    var th = t + B / 2;
    if (th >= from - 0.01 && th < C.sun - 0.05) {
      var t2 = t0 + th, k = src(ctx, 0.02, 1600 + n, 'white'), bp = biq(ctx, 'bandpass', 1900, 5), kg = ctx.createGain();
      kg.gain.setValueAtTime(0.0001, t2);
      kg.gain.linearRampToValueAtTime(0.05 * grow, t2 + 0.0006);
      kg.gain.exponentialRampToValueAtTime(0.0001, t2 + 0.009);
      k.connect(bp); bp.connect(kg); kg.connect(out); k.start(t2); k.stop(t2 + 0.02);
    }
  }
}

/* The ambient bed: a low A drone with a fifth, a band of air, and a slow
   cloud of quiet high partials, each breathing at its own slow rate. It
   swells under the reveals and settles under the end card. */
function bed(ctx, out, t0, from) {
  var level = ctx.createGain(), start = t0 + Math.max(0, from), end = t0 + DURATION + 0.1;
  level.gain.setValueAtTime(0.0001, t0);
  [[0.3, 0.55], [C.pull[0], 0.6], [C.formThud + 0.1, 0.85], [C.laptopBack[0], 0.65], [C.family.tv, 0.7], [C.sun, 1], [DURATION, 0.7]].forEach(function (p) {
    level.gain.linearRampToValueAtTime(p[1], t0 + p[0]);
  });
  level.connect(out);
  [[55, 0.5], [82.41, 0.26], [110, 0.12]].forEach(function (d, i) {
    var o = ctx.createOscillator(), g = gainOf(ctx, d[1]), lfo = ctx.createOscillator(), lg = gainOf(ctx, d[1] * 0.25);
    o.type = 'sine'; o.frequency.value = d[0];
    lfo.frequency.value = 0.09 + i * 0.05; lfo.connect(lg); lg.connect(g.gain);
    o.connect(g); g.connect(level); o.start(start); lfo.start(start); o.stop(end); lfo.stop(end);
  });
  var air = src(ctx, 5, 1400, 'pink', true), abp = biq(ctx, 'bandpass', 2800, 0.6), ag = gainOf(ctx, 0.16);
  var alfo = ctx.createOscillator(), alg = gainOf(ctx, 900);
  alfo.frequency.value = 0.07; alfo.connect(alg); alg.connect(abp.frequency);
  air.connect(abp); abp.connect(ag); ag.connect(level);
  air.start(start, (from % 5 + 5) % 5); alfo.start(start); air.stop(end); alfo.stop(end);
  [[440, 0.05], [660, 0.04], [880, 0.03]].forEach(function (c, i) {
    var o = ctx.createOscillator(), g = gainOf(ctx, c[1] * 0.5), lfo = ctx.createOscillator(), lg = gainOf(ctx, c[1] * 0.5);
    o.type = 'sine'; o.frequency.value = c[0] * (1 + (i % 2 ? 0.0015 : -0.001));
    lfo.frequency.value = 0.05 + i * 0.023; lfo.connect(lg); lg.connect(g.gain);
    var p = panner(ctx, (i % 2 ? 0.5 : -0.5));
    o.connect(g); g.connect(p); p.connect(level); o.start(start); lfo.start(start); o.stop(end); lfo.stop(end);
  });
}

/* --- the mix -------------------------------------------------------------- */
function reverbIR(ctx, secs, seed, pre) {
  var n = Math.floor(ctx.sampleRate * secs), p = Math.floor(ctx.sampleRate * (pre || 0));
  var buf = ctx.createBuffer(2, n + p, ctx.sampleRate);
  for (var c = 0; c < 2; c++) {
    var d = buf.getChannelData(c), rnd = mulberry32(seed + c), lp = 0;
    for (var i = 0; i < n; i++) { lp += ((rnd() * 2 - 1) - lp) * 0.3; d[p + i] = lp * Math.pow(1 - i / n, 3.4); }
  }
  return buf;
}
function buildMaster(ctx, dest, t0) {
  var sum = ctx.createGain();
  var shelf = biq(ctx, 'highshelf', 7000, 0.7); shelf.gain.value = -3;
  var glue = ctx.createDynamicsCompressor();
  glue.threshold.value = -18; glue.knee.value = 10; glue.ratio.value = 2.2;
  glue.attack.value = 0.01; glue.release.value = 0.25;
  var fade = ctx.createGain();
  fade.gain.setValueAtTime(1, t0);
  fade.gain.setValueAtTime(1, t0 + DURATION - 0.7);
  fade.gain.linearRampToValueAtTime(0, t0 + DURATION);
  sum.connect(shelf); shelf.connect(glue); glue.connect(fade); fade.connect(dest);

  var verb = ctx.createConvolver(); verb.normalize = true; verb.buffer = reverbIR(ctx, 5, 31, 0.04);
  var wetOut = gainOf(ctx, 0.55); verb.connect(wetOut); wetOut.connect(sum);
  function bus(dry, send) {
    var b = gainOf(ctx, dry), sd = gainOf(ctx, send);
    b.connect(sum); b.connect(sd); sd.connect(verb);
    return b;
  }
  return {
    fx: bus(0.5, 0.35),        /* whooshes, risers */
    sub: bus(0.55, 0.08),      /* impacts' low end: almost dry */
    room: bus(0.2, 0.9),       /* what the room carries on */
    ui: bus(0.28, 0.12),       /* clicks and thocks: close and dry */
    shine: bus(0.35, 0.9),     /* shimmers: mostly reverb */
    bed: bus(0.16, 0.5),
    beat: bus(0.5, 0.06)       /* the pulse: dry, up front */
  };
}

/* --- assembly: the one place cues become sounds --------------------------- */
function build(ctx, dest, t0, from) {
  var M = buildMaster(ctx, dest, t0);
  function at(t, fn) { if (t >= from - 0.01) fn(t0 + t); }

  bed(ctx, M.bed, t0, from);
  pulse(ctx, M.beat, t0, from);

  /* A: a soft landing on the first frame, then air past each painting,
     travelling right to left as the paintings do */
  at(0.02, function (tt) { impact(ctx, M.sub, M.room, tt, 0.45); });
  C.walk.forEach(function (p, i) {
    at(Math.max(0.05, p.t - 0.26), function (tt) { whoosh(ctx, M.fx, tt, 0.6, 0.3, 0.6, -0.6, 300, 1400, 20 + i); });
  });

  /* B: the pull-back builds, and the laptop lands */
  at(C.pull[0], function (tt) {
    whoosh(ctx, M.fx, tt, C.pull[1] - C.pull[0], 0.38, 0, 0, 180, 900, 30);
    riser(ctx, M.fx, tt, C.formThud - C.pull[0], 0.4, 31);
  });
  at(C.formThud, function (tt) { impact(ctx, M.sub, M.room, tt, 1); });

  /* C: in, click, click, out — the new picture settles on the way out */
  at(C.pushIn[0], function (tt) { whoosh(ctx, M.fx, tt, C.pushIn[1] - C.pushIn[0], 0.3, -0.15, 0.2, 250, 1600, 40); });
  at(C.menuOpen - 0.01, function (tt) { uiClick(ctx, M.ui, tt, 1150, 0.5, 41); });
  at(C.press, function (tt) { uiClick(ctx, M.ui, tt, 950, 0.55, 42); });
  at(C.pullOut[0], function (tt) { whoosh(ctx, M.fx, tt, C.pullOut[1] - C.pullOut[0], 0.3, 0.2, -0.1, 220, 1200, 43); });
  at(C.swap[1], function (tt) { impact(ctx, M.sub, M.room, tt, 0.35); });

  /* D: the laptop leaves, the phone rises and lands */
  at(C.laptopBack[0], function (tt) { whoosh(ctx, M.fx, tt, 0.72, 0.26, 0.1, -0.7, 200, 1100, 50); });
  at(C.phoneUp, function (tt) { riser(ctx, M.fx, tt, C.phoneLand - C.phoneUp, 0.3, 51); whoosh(ctx, M.fx, tt, 0.6, 0.26, 0, 0, 260, 1500, 52); });
  at(C.phoneLand, function (tt) { impact(ctx, M.sub, M.room, tt, 0.6); });

  /* E: each device takes its place */
  var f = C.family;
  [[f.tv, 150, 0.5, 0], [f.gnome, 200, 0.4, -0.5], [f.phone, 260, 0.35, 0], [f.mac, 210, 0.4, 0.5]].forEach(function (d, i) {
    at(d[0] + 0.158, function (tt) { thock(ctx, M.ui, tt, d[1], d[2], d[3]); });
    at(d[0] - 0.2, function (tt) { whoosh(ctx, M.fx, tt, 0.38, 0.14, d[3] * 1.4, d[3], 300, 1300, 60 + i); });
  });

  /* F: everything lifts away, and the icon lands */
  at(C.lift[0], function (tt) { whoosh(ctx, M.fx, tt, 0.54, 0.3, 0, 0, 300, 2000, 70); });
  at(C.iconIn[0] - 0.2, function (tt) { riser(ctx, M.fx, tt, C.sun - C.iconIn[0] + 0.2, 0.45, 71); });
  at(C.sun, function (tt) { impact(ctx, M.sub, M.room, tt, 1.15); shimmer(ctx, M.shine, tt, 2, 0.7, 72); });
  at(C.pillPress, function (tt) { uiClick(ctx, M.ui, tt, 1000, 0.45, 73); });
  return M;
}

function start(ctx, dest, t0, from) {
  var out = gainOf(ctx, 1); out.connect(dest);
  build(ctx, out, t0, from || 0);
  return { stop: function () {
    try {
      out.gain.cancelScheduledValues(ctx.currentTime);
      out.gain.setValueAtTime(out.gain.value, ctx.currentTime);
      out.gain.linearRampToValueAtTime(0, ctx.currentTime + 0.03);
      setTimeout(function () { out.disconnect(); }, 80);
    } catch (e) {}
  } };
}

function renderOffline(sampleRate) {
  var OC = window.OfflineAudioContext || window.webkitOfflineAudioContext, sr = sampleRate || 48000;
  var ctx = new OC(2, Math.ceil(DURATION * sr), sr);
  build(ctx, ctx.destination, 0, 0);
  return ctx.startRendering();
}

function toWav(buffer) {
  var numCh = buffer.numberOfChannels, len = buffer.length, sr = buffer.sampleRate;
  var blockAlign = numCh * 2, dataSize = len * blockAlign, ab = new ArrayBuffer(44 + dataSize), view = new DataView(ab);
  function str(off, s) { for (var i = 0; i < s.length; i++) view.setUint8(off + i, s.charCodeAt(i)); }
  str(0, 'RIFF'); view.setUint32(4, 36 + dataSize, true); str(8, 'WAVE');
  str(12, 'fmt '); view.setUint32(16, 16, true); view.setUint16(20, 1, true);
  view.setUint16(22, numCh, true); view.setUint32(24, sr, true);
  view.setUint32(28, sr * blockAlign, true); view.setUint16(32, blockAlign, true); view.setUint16(34, 16, true);
  str(36, 'data'); view.setUint32(40, dataSize, true);
  var ch = []; for (var c = 0; c < numCh; c++) ch.push(buffer.getChannelData(c));
  for (var i = 0, off = 44; i < len; i++) for (var k = 0; k < numCh; k++, off += 2) {
    var s = Math.max(-1, Math.min(1, ch[k][i]));
    view.setInt16(off, s < 0 ? s * 0x8000 : s * 0x7FFF, true);
  }
  return ab;
}

window.SCORE = { duration: DURATION, start: start, renderOffline: renderOffline, toWav: toWav };

})();
