/* The one table of when things happen in the Art Window reel. reel.html
   draws from it and reel-score.js plays from it, so a painting cannot land
   on screen a beat away from its sound. Times are film seconds. */
window.CUES = {
  DURATION: 16.5,
  /* ~91 bpm: every landing below falls on the pulse's beat or half-beat */
  BEAT: 0.66,

  /* A: the walk. Each painting is centred at its t. The first is already
     in front of us at 0 — the hook is a face. */
  walk: [
    { name: 'selfportrait', t: 0.132,  h: 540 },
    { name: 'parliament',   t: 0.858, h: 540 },
    { name: 'dancers',      t: 1.584,  h: 540 }
  ],
  stop: 2.772,            /* the camera settles on the wheat field */

  /* B: the camera keeps pulling back, and the wall turns out to be a screen:
     the laptop comes into view round the painting, the rest of the wall
     shrinking away with it */
  pull: [2.64, 3.96],
  shell: [2.772, 3.696],    /* bezel and black glass fade up inside the move */
  formThud: 3.96,

  /* C: the camera moves in on the Settings window FOR the interaction: Still
     life is switched on, then the Blur placement, then Apply changes, each
     click left to be seen. Only after the window closes and the camera has
     backed out to the old wallpaper does the new painting arrive. */
  pushIn: [4.488, 5.28],
  panelIn: 4.488,
  toChip: [5.016, 5.412],
  chip: 5.478,
  toStyle: [5.80, 6.07],
  style: 6.14,
  toApply: [6.40, 6.67],
  apply: 6.80,
  panelOut: 6.93,
  pullOut: [7.00, 7.76],
  swap: [7.79, 8.25],    /* the new painting crossfades in, its margins blurred */

  /* D: the phone */
  laptopBack: [9.24, 9.90],
  phoneUp: 9.306,
  phoneLand: 9.90,

  /* E: the family; each lands 0.158 s after its cue, on a half-beat */
  family: { tv: 10.402, gnome: 10.732, phone: 11.062, mac: 11.392 },

  /* F: the end card — the pulse stops and the app icon lands */
  lift: [12.21, 12.672],
  iconIn: [12.672, 13.2],
  sun: 13.2,
  word: 13.332, sub: 13.53, pill: 13.728, pillPress: 14.19, platforms: 14.256,

  /* one bold line per beat: [in, out, words] */
  captions: [
    [0.198, 2.574, 'New picture every day.'],
    [3.168, 4.488, 'On your desktop.'],
    [7.95, 9.24, 'Adjust and filter as you like.'],
    [10.95, 12.078, 'On all your devices.']
  ]
};
