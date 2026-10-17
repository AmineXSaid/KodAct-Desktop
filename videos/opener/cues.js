/*
 * cues.js: the beat sheet as data. The composition reads it in the browser
 * (window.CUES) and the scripts read it in Node (beat-sheet, place-audio,
 * stills, render). Scene code never holds a literal time: it asks b(bar, beat).
 * `at` / `from` / `to` are [bar, beat, fraction] on the grid, or seconds.
 */
window.CUES = {
  name: "forge-opener",
  title: "Forge",
  width: 1920,
  height: 1080,
  duration: 12.9, // 6 bars at 128 BPM (11.25 s), then the naming screen holds
  grid: { bpm: 128, firstBeat: 0, pickupBeats: 0, beatsPerBar: 4 },
  music: null, // silent: an app should not play music when it opens (BRAND.md)
  sfx: { dir: "assets/audio/sfx", peaks: "assets/audio/sfx/peaks.json" },
  safe: null, // 16:9 in the app window
  scenes: [
    { name: "Star", from: [1, 1], to: [2, 1], what: "a raw star in a teal aura falls over the dot field" },
    { name: "Land", from: [2, 1], to: [3, 1], what: "it lands on the anvil, a ring through the dots; it glows white hot" },
    { name: "Strike", from: [3, 1], to: [4, 1], what: "the hammer winds up slowly and strikes fast" },
    { name: "Shatter", from: [4, 1], to: [5, 1], what: "the ingot bursts into voxels that hang in the air, cooling" },
    { name: "Approve", from: [5, 1], to: [6, 1], what: "the voxels rebuild the Forge cube; its top cools to teal" },
    { name: "Lockup", from: [6, 1], to: 12.9, what: "the wordmark, the app's line, then the naming screen" },
  ],
  events: [
    { at: [1, 1], what: "the star enters, top left" },
    { at: [1, 3], what: "its light reaches the floor; the dots wake" },
    { at: [2, 1], what: "the star lands: ring, shake" },
    { at: [2, 3], what: "Describe it. (slam)" },
    { at: [3, 1], what: "hammer wind-up begins (slow)" },
    { at: [3, 3], what: "hammer at the top, behind; held" },
    { at: [3, 4], what: "STRIKE: flash, sparks, ring, shake" },
    { at: [4, 1], what: "Forge builds it. (slam); voxels rising" },
    { at: [4, 2], what: "voxels at their peak, slow motion" },
    { at: [4, 4], what: "voxels turn back" },
    { at: [5, 1], what: "the cube is whole: You approve. (slam)" },
    { at: [5, 2], what: "teal check lands" },
    { at: [5, 3], what: "the top cools to teal: ring through the dots" },
    { at: [6, 1], what: "flood to the stage; the wordmark" },
    { at: [6, 2], what: "Forge asks before it changes anything." },
    { at: [6, 4], what: "the letters part; the cube rises to the hero" },
    { at: [7, 1], what: "Hello. What should we call you? and the field" },
  ],
};
