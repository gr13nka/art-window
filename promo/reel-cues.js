/* The one table of when things happen in the Art Window reel. reel.html
   draws from it and reel-score.js plays from it, so a painting cannot land
   on screen a beat away from its sound. Times are film seconds. */
window.CUES = {
  DURATION: 14.52,
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

  /* C: the camera moves in to the menu bar FOR the interaction, then back
     out to show the new painting whole */
  pushIn: [4.488, 5.28],
  pointerIn: [5.016, 5.412],
  menuOpen: 5.478,
  toRow: [5.61, 5.834],
  press: 5.94,
  menuClose: 6.072,
  pullOut: [6.098, 6.996],
  swap: [6.138, 6.6],    /* the new painting crossfades in, margins widen */

  /* D: the phone */
  laptopBack: [7.26, 7.92],
  phoneUp: 7.326,
  phoneLand: 7.92,

  /* E: the family; each lands 0.158 s after its cue, on a half-beat */
  family: { tv: 8.422, gnome: 8.752, phone: 9.082, mac: 9.412 },

  /* F: the end card — the pulse stops and the app icon lands */
  lift: [10.23, 10.692],
  iconIn: [10.692, 11.22],
  sun: 11.22,
  word: 11.352, sub: 11.55, pill: 11.748, pillPress: 12.21, platforms: 12.276,

  /* one bold line per beat: [in, out, words] */
  captions: [
    [0.198, 2.574, 'One painting a day.'],
    [3.168, 4.488, 'On your desktop.'],
    [6.468, 7.326, 'Never cropped.'],
    [7.59, 8.514, 'On your phone.'],
    [8.646, 10.098, 'And on your TV.']
  ]
};
